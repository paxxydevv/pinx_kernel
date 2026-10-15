//! Physical and virtual memory management, plus the kernel heap.
//!
//! - The PMM hands out 4 KiB frames from a bitmap built over Limine's memory
//!   map.
//! - The VMM walks and builds the four-level page tables, reached through the
//!   HHDM.
//! - The heap is a first-fit free list carved out of a fixed virtual window
//!   that is backed by PMM frames on demand at init.
//!
//! Every page-table structure is accessed through the HHDM, so a physical
//! address is turned into a pointer by adding the HHDM offset, and never by
//! dereferencing the physical address itself.

use crate::{io, kheap, kpanic};
use crate::limine;
use crate::limine::MEMMAP_USABLE;

pub const PAGE_SIZE: u64 = 4096;

/// Returned by `vmm_virt_to_phys` when the address is not mapped.
pub const VMM_NO_PHYS: u64 = u64::MAX;
/// Returned by `pmm_alloc_page` when the bitmap is exhausted.
pub const PMM_NO_PAGE: u64 = u64::MAX;

const KHEAP_START: u64 = 0xFFFF_9000_0000_0000;
const KHEAP_END: u64 = 0xFFFF_A000_0000_0000;

/* Page table entry bits. */
const PTE_PRESENT: u64 = 1 << 0;
const PTE_WRITABLE: u64 = 1 << 1;
/// User/Supervisor. A page is reachable from ring 3 only if this bit is set in
/// *every* level of the walk, not just the leaf.
const PTE_USER: u64 = 1 << 2;
const PTE_PWT: u64 = 1 << 3;
const PTE_PCD: u64 = 1 << 4;
const PTE_HUGE: u64 = 1 << 7;
const PTE_ADDR_MASK: u64 = 0x000F_FFFF_FFFF_F000;
const PTE_ADDR_MASK_1G: u64 = 0x000F_FFFF_C000_0000;
const PTE_ADDR_MASK_2M: u64 = 0x000F_FFFF_FFE0_0000;

static mut BITMAP: *mut u64 = core::ptr::null_mut();
static mut BITMAP_ENTRIES: u64 = 0;

/// Where `pmm_alloc_page` resumes its scan. Without it every allocation walks
/// the bitmap from bit 0, which gets expensive as the bitmap grows.
static mut ALLOC_HINT: u64 = 0;

static mut PML4: *mut u64 = core::ptr::null_mut();

/// A free-list node. `free` is a `u64` rather than a `bool` on purpose: with
/// this member order the header is exactly 32 bytes, so every payload starts
/// on a 16-byte boundary and stays there across splits.
#[repr(C)]
struct HeapBlock {
    size: u64,
    next: *mut HeapBlock,
    free: u64,
    reserved: u64,
}

const HEAP_BLOCK_SIZE: u64 = core::mem::size_of::<HeapBlock>() as u64;
const HEAP_ALIGN: u64 = 16;

static mut HEAP_HEAD: *mut HeapBlock = core::ptr::null_mut();

#[inline]
unsafe fn tlb_flush(virtual_address: u64) {
    core::arch::asm!("invlpg [{}]", in(reg) virtual_address, options(nostack));
}

/// Every page-table structure is walked through the HHDM, so a missing HHDM
/// response turns into a wild pointer instead of an error. Fail loudly.
fn hhdm_base() -> u64 {
    let response = limine::hhdm_response();
    if response.is_null() {
        kpanic!("Limine returned no HHDM response");
    }
    unsafe { (*response).offset }
}

#[inline]
unsafe fn phys_to_ptr(physical: u64) -> *mut u64 {
    (physical + hhdm_base()) as *mut u64
}

/// Turn a physical address into the virtual address the kernel reaches it at
/// through the HHDM. Used to fill in a freshly allocated frame before it has a
/// user mapping.
pub fn phys_to_virt(physical: u64) -> u64 {
    physical + hhdm_base()
}

/// The HHDM offset. A virtual address that points into the HHDM (for example a
/// Limine-provided framebuffer pointer) is turned into a physical address by
/// subtracting this.
pub fn hhdm_offset() -> u64 {
    hhdm_base()
}

// ------------------------------------------------------------------- PMM

pub fn pmm_init() {
    let memmap = limine::memmap_response();
    if memmap.is_null() {
        kpanic!("Limine returned no memmap response");
    }
    if limine::hhdm_response().is_null() {
        kpanic!("Limine returned no HHDM response");
    }

    let entry_count = unsafe { (*memmap).entry_count };

    let mut highest_address = 0u64;
    let mut largest_base = 0u64;
    let mut largest_len = 0u64;

    for i in 0..entry_count {
        let entry = unsafe { *(*memmap).entries.add(i as usize) };
        if entry.is_null() {
            continue;
        }
        let entry = unsafe { &*entry };

        if entry.type_ != MEMMAP_USABLE {
            continue;
        }
        if entry.length > largest_len {
            largest_len = entry.length;
            largest_base = entry.base;
        }
        let end = entry.base + entry.length;
        if end > highest_address {
            highest_address = end;
        }
    }
    if largest_len == 0 {
        kpanic!("memmap reports no usable memory");
    }

    let total_pages = highest_address / PAGE_SIZE;
    let bitmap_entries = (total_pages + 63) / 64;
    let bitmap_size = bitmap_entries * 8;
    let bitmap_pages = (bitmap_size + PAGE_SIZE - 1) / PAGE_SIZE;

    // If the bitmap does not fit there is no correct answer here, so stop
    // rather than clamp and let the PMM index past its own end.
    if bitmap_pages > largest_len / PAGE_SIZE {
        kpanic!("pmm: largest usable region cannot hold the page bitmap");
    }

    let bitmap_physical =
        (largest_base + largest_len - bitmap_pages * PAGE_SIZE) & !(PAGE_SIZE - 1);
    let bitmap = (bitmap_physical + hhdm_base()) as *mut u64;

    unsafe {
        BITMAP = bitmap;
        BITMAP_ENTRIES = bitmap_entries;
        ALLOC_HINT = 0;
    }

    // Start with everything reserved, then free what the memory map calls
    // usable, then take the bitmap's own pages back.
    for i in 0..bitmap_entries {
        unsafe { *bitmap.add(i as usize) = u64::MAX };
    }
    for i in 0..entry_count {
        let entry = unsafe { *(*memmap).entries.add(i as usize) };
        if entry.is_null() {
            continue;
        }
        let entry = unsafe { &*entry };
        if entry.type_ != MEMMAP_USABLE {
            continue;
        }
        let start_page = entry.base / PAGE_SIZE;
        let page_count = entry.length / PAGE_SIZE;
        for page in 0..page_count {
            let page_number = start_page + page;
            unsafe {
                *bitmap.add((page_number / 64) as usize) &= !(1u64 << (page_number % 64));
            }
        }
    }
    for i in 0..bitmap_pages {
        let page = bitmap_physical / PAGE_SIZE + i;
        unsafe {
            *bitmap.add((page / 64) as usize) |= 1u64 << (page % 64);
        }
    }
}

pub fn pmm_alloc_page() -> u64 {
    let flags = io::irq_save();

    let bitmap = unsafe { BITMAP };
    let entries = unsafe { BITMAP_ENTRIES };
    let hint = unsafe { ALLOC_HINT };

    let mut result = PMM_NO_PAGE;

    if entries != 0 {
        'outer: for scanned in 0..entries {
            let index = (hint + scanned) % entries;
            let word = unsafe { *bitmap.add(index as usize) };
            if word == u64::MAX {
                continue;
            }
            for bit in 0..64 {
                if word & (1u64 << bit) == 0 {
                    unsafe {
                        *bitmap.add(index as usize) = word | (1u64 << bit);
                        ALLOC_HINT = (index * 64 + bit + 1) % entries;
                    }
                    result = (index * 64 + bit) * PAGE_SIZE;
                    break 'outer;
                }
            }
        }
    }

    io::irq_restore(flags);
    result
}

pub fn pmm_free_page(page: u64) {
    if page % PAGE_SIZE != 0 {
        kpanic!("pmm_free_page: address is not page aligned");
    }

    let page_number = page / PAGE_SIZE;
    if page_number / 64 >= unsafe { BITMAP_ENTRIES } {
        kpanic!("pmm_free_page: frame is outside the bitmap");
    }

    let flags = io::irq_save();
    let bitmap = unsafe { BITMAP };

    unsafe {
        *bitmap.add((page_number / 64) as usize) &= !(1u64 << (page_number % 64));

        // Keep the scan cursor ahead of the frame we just released.
        if page_number / 64 < ALLOC_HINT {
            ALLOC_HINT = page_number / 64;
        }
    }

    io::irq_restore(flags);
}

// ------------------------------------------------------------------- VMM

pub fn vmm_init() {
    let cr3: u64;
    unsafe {
        core::arch::asm!("mov {}, cr3", out(reg) cr3, options(nomem, nostack));
    }
    if cr3 == 0 {
        kpanic!("vmm_init: CR3 is zero, no usable page tables");
    }
    let pml4_physical = cr3 & !0xFFF;
    unsafe {
        PML4 = (pml4_physical + hhdm_base()) as *mut u64;
    }
}

unsafe fn alloc_table() -> *mut u64 {
    let page = pmm_alloc_page();
    if page == PMM_NO_PAGE {
        kpanic!("vmm: out of memory allocating a page table");
    }
    let table = phys_to_ptr(page);
    core::ptr::write_bytes(table, 0, 512);
    table
}

pub fn vmm_map_page_flags(virtual_address: u64, physical: u64, flags: u64) {
    let pml4 = unsafe { PML4 };
    if pml4.is_null() {
        kpanic!("vmm_map_page: vmm_init was not called");
    }
    if virtual_address & (PAGE_SIZE - 1) != 0 || physical & (PAGE_SIZE - 1) != 0 {
        kpanic!("vmm_map_page: address is not page aligned");
    }

    let pml4_index = ((virtual_address >> 39) & 0x1FF) as usize;
    let pdpt_index = ((virtual_address >> 30) & 0x1FF) as usize;
    let pd_index = ((virtual_address >> 21) & 0x1FF) as usize;
    let pt_index = ((virtual_address >> 12) & 0x1FF) as usize;

    // When the leaf is user-accessible the upper levels must allow ring 3 to
    // descend through them, so the same U/S bit goes on the tables too. A page
    // is only reachable from ring 3 if every level says so.
    let table_flags = PTE_PRESENT | PTE_WRITABLE | (flags & PTE_USER);
    let user = flags & PTE_USER != 0;

    unsafe {
        let mut entry = *pml4.add(pml4_index);
        if entry & PTE_PRESENT != 0 {
            if entry & PTE_HUGE != 0 {
                kpanic!("vmm_map_page: PML4 entry covers 512GiB, split it first");
            }
            if user {
                *pml4.add(pml4_index) = entry | PTE_USER;
            }
        } else {
            let new_page = pmm_alloc_page();
            if new_page == PMM_NO_PAGE {
                kpanic!("vmm_map_page: out of memory allocating a PML4 entry");
            }
            *pml4.add(pml4_index) = new_page | table_flags;
            core::ptr::write_bytes(phys_to_ptr(new_page), 0, 512);
        }
        entry = *pml4.add(pml4_index);
        let pdpt = phys_to_ptr(entry & PTE_ADDR_MASK);

        let mut entry = *pdpt.add(pdpt_index);
        if entry & PTE_PRESENT != 0 {
            if entry & PTE_HUGE != 0 {
                kpanic!("vmm_map_page: PDPT entry covers 1GiB, split it first");
            }
            if user {
                *pdpt.add(pdpt_index) = entry | PTE_USER;
            }
        } else {
            let new_page = pmm_alloc_page();
            if new_page == PMM_NO_PAGE {
                kpanic!("vmm_map_page: out of memory allocating a PDPT entry");
            }
            *pdpt.add(pdpt_index) = new_page | table_flags;
            core::ptr::write_bytes(phys_to_ptr(new_page), 0, 512);
        }
        entry = *pdpt.add(pdpt_index);
        let pd = phys_to_ptr(entry & PTE_ADDR_MASK);

        let mut entry = *pd.add(pd_index);
        if entry & PTE_PRESENT != 0 {
            if entry & PTE_HUGE != 0 {
                kpanic!("vmm_map_page: PD entry covers 2MiB, split it first");
            }
            if user {
                *pd.add(pd_index) = entry | PTE_USER;
            }
        } else {
            let new_page = pmm_alloc_page();
            if new_page == PMM_NO_PAGE {
                kpanic!("vmm_map_page: out of memory allocating a PD entry");
            }
            *pd.add(pd_index) = new_page | table_flags;
            core::ptr::write_bytes(phys_to_ptr(new_page), 0, 512);
        }
        entry = *pd.add(pd_index);
        let pt = phys_to_ptr(entry & PTE_ADDR_MASK);

        *pt.add(pt_index) = (physical & PTE_ADDR_MASK) | PTE_PRESENT | flags;

        // Installing a PTE does not drop a cached non-present entry, so a
        // translation that previously faulted here can keep faulting.
        tlb_flush(virtual_address);
    }
}

pub fn vmm_map_page(virtual_address: u64, physical: u64) {
    vmm_map_page_flags(virtual_address, physical, PTE_PRESENT | PTE_WRITABLE);
}

/// Map a page that ring 3 is allowed to touch. The U/S bit is applied to every
/// level by `vmm_map_page_flags`.
pub fn vmm_map_user_page(virtual_address: u64, physical: u64) {
    vmm_map_page_flags(
        virtual_address,
        physical,
        PTE_PRESENT | PTE_WRITABLE | PTE_USER,
    );
}

/// Like `vmm_map_user_page`, but lets the caller choose the leaf permissions
/// (for example to map a read-only or non-executable user page).
pub fn vmm_map_user_page_flags(virtual_address: u64, physical: u64, flags: u64) {
    vmm_map_page_flags(virtual_address, physical, flags | PTE_USER);
}

/// Map a device region (for example the framebuffer) so ring 3 can write it.
/// Uncached, like `vmm_map_mmio`, so pixel writes reach the device directly.
pub fn vmm_map_user_mmio(virtual_address: u64, physical: u64) {
    vmm_map_page_flags(
        virtual_address,
        physical,
        PTE_PRESENT | PTE_WRITABLE | PTE_PWT | PTE_PCD | PTE_USER,
    );
}

pub fn vmm_map_mmio(virtual_address: u64, physical: u64) {
    // PTE_PRESENT | PTE_WRITABLE | PTE_PWT | PTE_PCD = uncacheable.
    vmm_map_page_flags(
        virtual_address,
        physical,
        PTE_PRESENT | PTE_WRITABLE | PTE_PWT | PTE_PCD,
    );
}

pub fn vmm_unmap_page(virtual_address: u64) {
    let pml4 = unsafe { PML4 };
    if pml4.is_null() {
        kpanic!("vmm_unmap_page: vmm_init was not called");
    }

    let pml4_index = ((virtual_address >> 39) & 0x1FF) as usize;
    let pdpt_index = ((virtual_address >> 30) & 0x1FF) as usize;
    let pd_index = ((virtual_address >> 21) & 0x1FF) as usize;
    let pt_index = ((virtual_address >> 12) & 0x1FF) as usize;

    unsafe {
        let pml4_entry = *pml4.add(pml4_index);
        if pml4_entry & PTE_PRESENT == 0 {
            return;
        }
        if pml4_entry & PTE_HUGE != 0 {
            kpanic!("vmm_unmap_page: PML4 entry covers 512GiB, not a page table");
        }
        let pdpt = phys_to_ptr(pml4_entry & PTE_ADDR_MASK);

        let pdpt_entry = *pdpt.add(pdpt_index);
        if pdpt_entry & PTE_PRESENT == 0 {
            return;
        }
        if pdpt_entry & PTE_HUGE != 0 {
            kpanic!("vmm_unmap_page: PDPT entry covers 1GiB, not a page table");
        }
        let pd = phys_to_ptr(pdpt_entry & PTE_ADDR_MASK);

        let pd_entry = *pd.add(pd_index);
        if pd_entry & PTE_PRESENT == 0 {
            return;
        }
        if pd_entry & PTE_HUGE != 0 {
            kpanic!("vmm_unmap_page: PD entry covers 2MiB, not a page table");
        }
        let pt = phys_to_ptr(pd_entry & PTE_ADDR_MASK);

        if *pt.add(pt_index) & PTE_PRESENT == 0 {
            return;
        }

        // The backing frame is deliberately not returned to the PMM here, and
        // neither are the page-table pages this walk leaves behind: doing it
        // correctly needs a TLB shootdown across every CPU.
        *pt.add(pt_index) = 0;
        tlb_flush(virtual_address);
    }
}

/// Resolve the physical address of `virtual_address` by walking the four-level
/// tables. Takes any address, not just a page-aligned one, and never panics:
/// it is meant to be callable from a fault handler.
pub fn vmm_virt_to_phys(virtual_address: u64) -> u64 {
    let pml4 = unsafe { PML4 };
    if pml4.is_null() {
        return VMM_NO_PHYS;
    }

    let offset = virtual_address & (PAGE_SIZE - 1);
    let addr = virtual_address & !0xFFF;

    unsafe {
        let mut entry = *pml4.add(((addr >> 39) & 0x1FF) as usize);
        if entry & PTE_PRESENT == 0 || entry & PTE_HUGE != 0 {
            return VMM_NO_PHYS;
        }

        let pdpt = phys_to_ptr(entry & PTE_ADDR_MASK);
        entry = *pdpt.add(((addr >> 30) & 0x1FF) as usize);
        if entry & PTE_PRESENT == 0 {
            return VMM_NO_PHYS;
        }
        if entry & PTE_HUGE != 0 {
            return (entry & PTE_ADDR_MASK_1G) | (addr & 0x3FFF_FFFF) | offset;
        }

        let pd = phys_to_ptr(entry & PTE_ADDR_MASK);
        entry = *pd.add(((addr >> 21) & 0x1FF) as usize);
        if entry & PTE_PRESENT == 0 {
            return VMM_NO_PHYS;
        }
        if entry & PTE_HUGE != 0 {
            return (entry & PTE_ADDR_MASK_2M) | (addr & 0x1F_FFFF) | offset;
        }

        let pt = phys_to_ptr(entry & PTE_ADDR_MASK);
        entry = *pt.add(((addr >> 12) & 0x1FF) as usize);
        if entry & PTE_PRESENT == 0 {
            return VMM_NO_PHYS;
        }

        (entry & PTE_ADDR_MASK) | offset
    }
}

// ------------------------------------------------------------------ heap

pub fn kheap_init() {
    // Large enough to hold a whole `/init` image in one allocation: the loader
    // buffers the file up to `MAX_INIT_SIZE` (4 MiB) before mapping it.
    let heap_pages: u64 = 2048; // 8 MiB

    // The first frame has to be mapped before anything can be written to
    // KHEAP_START, so it goes in first and on its own.
    let page = pmm_alloc_page();
    if page == PMM_NO_PAGE {
        kpanic!("kheap_init: out of memory for the first kernel heap page");
    }
    vmm_map_page(KHEAP_START, page);

    let heap_start = KHEAP_START as *mut HeapBlock;

    // Now that the header is reachable, make it a valid but empty heap before
    // asking for the remaining pages.
    unsafe {
        HEAP_HEAD = heap_start;
        (*heap_start).size = 0;
        (*heap_start).free = 0;
        (*heap_start).next = core::ptr::null_mut();
        (*heap_start).reserved = 0;
    }

    let mut mapped: u64 = 1;
    for i in 1..heap_pages {
        let page = pmm_alloc_page();
        if page == PMM_NO_PAGE {
            // Grow the heap by what we actually got rather than dying.
            kheap!(
                "kheap_init: stopped at {} of {} pages\n",
                mapped,
                heap_pages
            );
            break;
        }
        vmm_map_page(KHEAP_START + i * PAGE_SIZE, page);
        mapped += 1;
    }

    unsafe {
        (*heap_start).size = mapped * PAGE_SIZE - HEAP_BLOCK_SIZE;
        (*heap_start).free = 1;
        (*heap_start).next = core::ptr::null_mut();
    }

    let _ = KHEAP_END;
}

pub fn kmalloc(size: u64) -> *mut u8 {
    if size == 0 || size > u64::MAX - (HEAP_ALIGN - 1) {
        return core::ptr::null_mut();
    }

    let heap_start = unsafe { HEAP_HEAD };
    if heap_start.is_null() {
        kpanic!("kmalloc: kheap_init was not called");
    }

    // Round up so every split point stays on a 16-byte boundary.
    let size = (size + (HEAP_ALIGN - 1)) & !(HEAP_ALIGN - 1);

    let flags = io::irq_save();

    let mut current = heap_start;
    while !current.is_null() {
        unsafe {
            if (*current).free != 0 && (*current).size >= size {
                if (*current).size > size + HEAP_BLOCK_SIZE {
                    let split =
                        (current as *mut u8).add(HEAP_BLOCK_SIZE as usize).add(size as usize)
                            as *mut HeapBlock;

                    (*split).free = 1;
                    (*split).next = (*current).next;
                    (*split).size = (*current).size - size - HEAP_BLOCK_SIZE;
                    (*split).reserved = 0;

                    (*current).size = size;
                    (*current).next = split;
                }

                (*current).free = 0;

                let result = (current as *mut u8).add(HEAP_BLOCK_SIZE as usize);
                io::irq_restore(flags);
                return result;
            }

            current = (*current).next;
        }
    }

    io::irq_restore(flags);
    core::ptr::null_mut()
}

pub fn coalesce() {
    let mut current = unsafe { HEAP_HEAD };

    while !current.is_null() {
        unsafe {
            let next = (*current).next;
            if next.is_null() {
                break;
            }

            if (*current).free != 0 && (*next).free != 0 {
                let current_end = (current as *mut u8).add(HEAP_BLOCK_SIZE as usize).add((*current).size as usize);
                if current_end == next as *mut u8 {
                    (*current).size += HEAP_BLOCK_SIZE + (*next).size;
                    (*current).next = (*next).next;
                    continue;
                }
            }

            current = (*current).next;
        }
    }
}

pub fn kfree(ptr: *mut u8) {
    if ptr.is_null() {
        return;
    }

    let flags = io::irq_save();

    unsafe {
        let block = (ptr as usize - HEAP_BLOCK_SIZE as usize) as *mut HeapBlock;

        if (*block).free != 0 {
            io::irq_restore(flags);
            return; // double free: mark and bail, do not merge twice
        }

        (*block).free = 1;
        coalesce();
    }

    io::irq_restore(flags);
}
