#include "pinx/mm/memory_management.h"
#include "pinx/io.h"
#include "pinx/kdebug.h"
#include "pinx/terminal.h"
#include "3party/limine.h"
#include <stdint.h>

extern volatile struct limine_memmap_request memmap_request;
extern volatile struct limine_hhdm_request hhdm_request;

static uint64_t *bitmap;
static uint64_t bitmap_entries;

/*
 * Where pmm_alloc_page resumes its scan. Without it every allocation walks
 * the bitmap from bit 0, which gets expensive as the bitmap grows.
 */
static uint64_t alloc_hint;

#define PAGE_SIZE 4096
#define PMM_NO_PAGE UINT64_MAX
#define KHEAP_START 0xFFFF900000000000ULL
#define KHEAP_END   0xFFFFA00000000000ULL

/* Page table entry bits, named so the mapping code stops being a pile of
 * magic numbers. */
#define PTE_PRESENT   (1ULL << 0)
#define PTE_WRITABLE  (1ULL << 1)
#define PTE_USER      (1ULL << 2)
#define PTE_PWT       (1ULL << 3)
#define PTE_PCD       (1ULL << 4)
#define PTE_ACCESSED  (1ULL << 5)
#define PTE_DIRTY     (1ULL << 6)
#define PTE_HUGE      (1ULL << 7)
#define PTE_GLOBAL    (1ULL << 8)
#define PTE_NX        (1ULL << 63)
#define PTE_ADDR_MASK 0x000FFFFFFFFFF000ULL

static uint64_t *pml4;

static struct heap_block *heap_start;

/*
 * `free` is a uint64_t rather than a bool on purpose: with this member order
 * the header is exactly 32 bytes, so every payload starts on a 16-byte
 * boundary and stays there across splits. A 24-byte header would hand out
 * 8-mod-16 pointers, which breaks the SysV stack alignment the scheduler
 * depends on.
 */
struct heap_block {
    uint64_t size;               /* usable payload bytes, excluding this header */
    struct heap_block *next;
    uint64_t free;               /* 0 = in use, 1 = free */
    uint64_t reserved;
};

#define HEAP_ALIGN 16

static inline void tlb_flush(uint64_t virtual) {
    __asm__ volatile (
        "invlpg (%0)"
        :
        : "r"(virtual)
        : "memory"
    );
}

/* Every page-table structure is walked through the HHDM, so a missing HHDM
 * response turns into a wild pointer instead of an error. Fail loudly. */
static uint64_t hhdm_base(void) {
    if (hhdm_request.response == nullptr) {
        panic("Limine returned no HHDM response");
    }
    return hhdm_request.response->offset;
}

void pmm_init(void) {
    if (memmap_request.response == nullptr) {
        panic("Limine returned no memmap response");
    }
    if (hhdm_request.response == nullptr) {
        panic("Limine returned no HHDM response");
    }

    uint64_t highest_address = 0;
    uint64_t largest_base = 0;
    uint64_t largest_len = 0;

    for (uint64_t i = 0; i < memmap_request.response->entry_count; i++) {
        struct limine_memmap_entry *entry = memmap_request.response->entries[i];

        if (entry->type != LIMINE_MEMMAP_USABLE)
            continue;
        if (entry->length > largest_len) {
            largest_len = entry->length;
            largest_base = entry->base;
        }
        uint64_t end = entry->base + entry->length;
        if (end > highest_address) {
            highest_address = end;
        }
    }
    if (largest_len == 0) {
        panic("memmap reports no usable memory");
    }

    uint64_t total_pages = highest_address / PAGE_SIZE;
    bitmap_entries = (total_pages + 63) / 64;
    uint64_t bitmap_size = bitmap_entries * sizeof(uint64_t);
    uint64_t bitmap_pages = (bitmap_size + PAGE_SIZE - 1) / PAGE_SIZE;

    /*
     * Previously this clamped bitmap_pages down to fit and left bitmap_entries
     * at the larger value, so pmm_alloc_page could index past the end of the
     * bitmap and hand out frames backed by arbitrary memory. If the bitmap
     * does not fit there is no correct answer here, so stop.
     */
    if (bitmap_pages > largest_len / PAGE_SIZE) {
        panic("pmm: largest usable region cannot hold the page bitmap");
    }

    uint64_t bitmap_physical = largest_base + largest_len - bitmap_pages * PAGE_SIZE;
    bitmap_physical &= ~(uint64_t)(PAGE_SIZE - 1);
    bitmap = (uint64_t *)(bitmap_physical + hhdm_base());
    alloc_hint = 0;

    for (uint64_t i = 0; i < bitmap_entries; i++) {
        bitmap[i] = UINT64_MAX;
    }
    for (uint64_t i = 0; i < memmap_request.response->entry_count; i++) {

        struct limine_memmap_entry *entry = memmap_request.response->entries[i];

        if (entry->type != LIMINE_MEMMAP_USABLE)
            continue;

        uint64_t start_page = entry->base / PAGE_SIZE;
        uint64_t page_count = entry->length / PAGE_SIZE;

        for (uint64_t page = 0; page < page_count; page++) {
            uint64_t page_number = start_page + page;
            bitmap[page_number / 64] &= ~(1ULL << (page_number % 64));
        }
    }
    for (uint64_t i = 0; i < bitmap_pages; i++) {
        uint64_t page = bitmap_physical / PAGE_SIZE + i;
        bitmap[page / 64] |= (1ULL << (page % 64));
    }
}

uint64_t pmm_alloc_page(void) {
    uint64_t flags = irq_save();

    uint64_t result = PMM_NO_PAGE;

    for (uint64_t scanned = 0; scanned < bitmap_entries; scanned++) {
        uint64_t index = (alloc_hint + scanned) % bitmap_entries;

        if (bitmap[index] == UINT64_MAX)
            continue;

        for (uint64_t bit = 0; bit < 64; bit++) {
            if ((bitmap[index] & (1ULL << bit)) == 0) {
                bitmap[index] |= (1ULL << bit);
                alloc_hint = (index * 64 + bit + 1) % bitmap_entries;
                result = (index * 64 + bit) * PAGE_SIZE;
                goto out;
            }
        }
    }

out:
    irq_restore(flags);
    return result;
}

void pmm_free_page(uint64_t page) {
    if (page % PAGE_SIZE != 0) {
        panic("pmm_free_page: address is not page aligned");
    }

    uint64_t page_number = page / PAGE_SIZE;

    if (page_number / 64 >= bitmap_entries) {
        panic("pmm_free_page: frame is outside the bitmap");
    }

    uint64_t flags = irq_save();

    bitmap[page_number / 64] &= ~(1ULL << (page_number % 64));

    /* Keep the scan cursor ahead of the frame we just released. */
    if ((page_number / 64) < alloc_hint)
        alloc_hint = page_number / 64;

    irq_restore(flags);
}

void vmm_init(void) {
    uint64_t cr3;
    __asm__ volatile(
        "mov %%cr3, %0"
        : "=r"(cr3)
    );
    if (cr3 == 0) {
        panic("vmm_init: CR3 is zero, no usable page tables");
    }
    uint64_t pml4_physical = cr3 & ~0xFFFULL;
    pml4 = (uint64_t *)(pml4_physical + hhdm_base());
}

void vmm_map_page_flags(uint64_t virtual, uint64_t physical, uint64_t flags) {
    if (pml4 == nullptr) {
        panic("vmm_map_page: vmm_init was not called");
    }

    if ((virtual & (PAGE_SIZE - 1)) != 0 || (physical & (PAGE_SIZE - 1)) != 0) {
        panic("vmm_map_page: address is not page aligned");
    }

    uint64_t pml4_index = (virtual >> 39) & 0x1FF;
    uint64_t pdpt_index = (virtual >> 30) & 0x1FF;
    uint64_t pd_index   = (virtual >> 21) & 0x1FF;
    uint64_t pt_index   = (virtual >> 12) & 0x1FF;

    uint64_t pml4_entry = pml4[pml4_index];
    if (pml4_entry & PTE_PRESENT) {
        if (pml4_entry & PTE_HUGE) {
            panic("vmm_map_page: PML4 entry covers 512GiB, split it first");
        }
    } else {
    uint64_t new_page = pmm_alloc_page();
    if (new_page == PMM_NO_PAGE) {
        panic("vmm_map_page: out of memory allocating a PML4 entry");
    }

    pml4[pml4_index] = new_page | PTE_PRESENT | PTE_WRITABLE;

    uint64_t *pdpt = (uint64_t *)(new_page + hhdm_base());
    for (uint64_t i = 0; i < 512; i++)
        pdpt[i] = 0;
    }
    pml4_entry = pml4[pml4_index];
    uint64_t pdpt_physical = pml4_entry & PTE_ADDR_MASK;
    uint64_t *pdpt = (uint64_t *)(pdpt_physical + hhdm_base());


    uint64_t pdpt_entry = pdpt[pdpt_index];
    if (pdpt_entry & PTE_PRESENT) {
        /* A 1GiB leaf here would be mistaken for the address of a page
         * directory, and the write below would land on unrelated memory. */
        if (pdpt_entry & PTE_HUGE) {
            panic("vmm_map_page: PDPT entry covers 1GiB, split it first");
        }
    } else {
    uint64_t new_page = pmm_alloc_page();
    if (new_page == PMM_NO_PAGE) {
        panic("vmm_map_page: out of memory allocating a PDPT entry");
    }

    pdpt[pdpt_index] = new_page | PTE_PRESENT | PTE_WRITABLE;

    uint64_t *pd = (uint64_t *)(new_page + hhdm_base());
    for (uint64_t i = 0; i < 512; i++)
        pd[i] = 0;
    }
    pdpt_entry = pdpt[pdpt_index];
    uint64_t pd_physical = pdpt_entry & PTE_ADDR_MASK;
    uint64_t *pd = (uint64_t *)(pd_physical + hhdm_base());

    uint64_t pd_entry = pd[pd_index];
    if (pd_entry & PTE_PRESENT) {
        if (pd_entry & PTE_HUGE) {
            panic("vmm_map_page: PD entry covers 2MiB, split it first");
        }
    } else {
    uint64_t new_page = pmm_alloc_page();
    if (new_page == PMM_NO_PAGE) {
        panic("vmm_map_page: out of memory allocating a PD entry");
    }

    pd[pd_index] = new_page | PTE_PRESENT | PTE_WRITABLE;

    uint64_t *pt = (uint64_t *)(new_page + hhdm_base());
    for (uint64_t i = 0; i < 512; i++)
        pt[i] = 0;
    }
    pd_entry = pd[pd_index];
    uint64_t pt_physical = pd_entry & PTE_ADDR_MASK;
    uint64_t *pt = (uint64_t *)(pt_physical + hhdm_base());

    pt[pt_index] = (physical & PTE_ADDR_MASK) | PTE_PRESENT | flags;

    /*
     * Installing a PTE does not drop a cached non-present entry, so a
     * translation that previously faulted here can keep faulting. The hardware
     * only fills the TLB on access, but firmware and the bootloader may have
     * already walked this range.
     */
    tlb_flush(virtual);
}

void vmm_map_page(uint64_t virtual, uint64_t physical) {
    vmm_map_page_flags(virtual, physical, PTE_PRESENT | PTE_WRITABLE);
}

void vmm_map_mmio(uint64_t virtual, uint64_t physical) {
    /* PTE_PRESENT | PTE_WRITABLE | PTE_PWT | PTE_PCD = uncacheable. */
    vmm_map_page_flags(virtual, physical,
                       PTE_PRESENT | PTE_WRITABLE | PTE_PWT | PTE_PCD);
}

void vmm_unmap_page(uint64_t virtual) {
    if (pml4 == nullptr) {
        panic("vmm_unmap_page: vmm_init was not called");
    }

    uint64_t pml4_index = (virtual >> 39) & 0x1FF;
    uint64_t pdpt_index = (virtual >> 30) & 0x1FF;
    uint64_t pd_index   = (virtual >> 21) & 0x1FF;
    uint64_t pt_index   = (virtual >> 12) & 0x1FF;

    // PML4
    uint64_t pml4_entry = pml4[pml4_index];

    if (!(pml4_entry & PTE_PRESENT))
        return;
    if (pml4_entry & PTE_HUGE) {
        panic("vmm_unmap_page: PML4 entry covers 512GiB, not a page table");
    }

    uint64_t pdpt_physical = pml4_entry & PTE_ADDR_MASK;
    uint64_t *pdpt =
        (uint64_t *)(pdpt_physical + hhdm_base());

    // PDPT
    uint64_t pdpt_entry = pdpt[pdpt_index];

    if (!(pdpt_entry & PTE_PRESENT))
        return;
    if (pdpt_entry & PTE_HUGE) {
        panic("vmm_unmap_page: PDPT entry covers 1GiB, not a page table");
    }

    uint64_t pd_physical = pdpt_entry & PTE_ADDR_MASK;
    uint64_t *pd =
        (uint64_t *)(pd_physical + hhdm_base());

    // PD
    uint64_t pd_entry = pd[pd_index];

    if (!(pd_entry & PTE_PRESENT))
        return;
    if (pd_entry & PTE_HUGE) {
        panic("vmm_unmap_page: PD entry covers 2MiB, not a page table");
    }

    uint64_t pt_physical = pd_entry & PTE_ADDR_MASK;
    uint64_t *pt =
        (uint64_t *)(pt_physical + hhdm_base());

    // PT
    if (!(pt[pt_index] & PTE_PRESENT))
        return;

    /*
     * The backing frame is deliberately not returned to the PMM here, and
     * neither are the page table pages this walk leaves behind. Doing it
     * correctly needs a TLB shootdown across every CPU plus a walk that frees
     * empty tables, and handing a frame back without the shootdown would let
     * another CPU still writing through a stale TLB entry corrupt its new
     * owner.
     */
    pt[pt_index] = 0;

    // Forget the old virtual -> physical translation in the TLB.
    tlb_flush(virtual);
}

void kheap_init(void) {
    uint64_t heap_pages = 64;

    /*
     * The first frame has to be mapped before anything can be written to
     * KHEAP_START, so it goes in first and on its own. Publishing the block
     * header before this point is a page fault on an address nothing maps yet.
     */
    uint64_t page = pmm_alloc_page();
    if (page == PMM_NO_PAGE) {
        panic("kheap_init: out of memory for the first kernel heap page");
    }
    vmm_map_page(KHEAP_START, page);

    heap_start = (struct heap_block *)KHEAP_START;

    /*
     * Now that the header is reachable, make it a valid but empty heap before
     * asking for the remaining pages. If we run out partway through, kmalloc()
     * then walks a well-formed exhausted heap instead of whatever happens to
     * be in the fresh page.
     */
    heap_start->size = 0;
    heap_start->free = 0;
    heap_start->next = nullptr;
    heap_start->reserved = 0;

    uint64_t mapped = 1;

    for (uint64_t i = 1; i < heap_pages; i++) {
        page = pmm_alloc_page();
        if (page == PMM_NO_PAGE) {
            /* Grow the heap by what we actually got rather than dying: a
             * short heap is still a usable heap, and the size is recomputed
             * from `mapped` below. */
            KHEAP("kheap_init: stopped at %lu of %lu pages\n",
                  (unsigned long)mapped, (unsigned long)heap_pages);
            break;
        }
        vmm_map_page(KHEAP_START + i * PAGE_SIZE, page);
        mapped++;
    }

    heap_start->size = mapped * PAGE_SIZE - sizeof(struct heap_block);
    heap_start->free = 1;
    heap_start->next = nullptr;
}

void *kmalloc(uint64_t size)
{
    if (size == 0 || size > UINT64_MAX - (HEAP_ALIGN - 1)) {
        return nullptr;
    }
    if (heap_start == nullptr) {
        panic("kmalloc: kheap_init was not called");
    }

    /*
     * Round up so that every split point stays on a 16-byte boundary and
     * stays there as the heap fragments.
     */
    size = (size + (HEAP_ALIGN - 1)) & ~(uint64_t)(HEAP_ALIGN - 1);

    uint64_t flags = irq_save();

    struct heap_block *current = heap_start;
    while (current != nullptr) {
        if (current->free && current->size >= size) {

            if (current->size > size + sizeof(struct heap_block)) {
                struct heap_block *next =
                    (struct heap_block *)((uint8_t *)(current + 1) + size);

                next->free = 1;
                next->next = current->next;
                next->size =
                    current->size - size - sizeof(struct heap_block);
                next->reserved = 0;

                current->size = size;
                current->next = next;
            }

            current->free = 0;

            void *result = (void *)(current + 1);
            irq_restore(flags);
            return result;
        }

        current = current->next;
    }

    irq_restore(flags);
    return nullptr;
}

void coalesce(void)
{
    struct heap_block *current = heap_start;

    while (current != nullptr && current->next != nullptr) {
        struct heap_block *next = current->next;

        if (current->free && next->free) {
            uint8_t *current_end =
                (uint8_t *)(current + 1) + current->size;

            if (current_end == (uint8_t *)next) {
                current->size += sizeof(struct heap_block) + next->size;
                current->next = next->next;

                continue;
            }
        }

        current = current->next;
    }
}

void kfree(void* ptr) {
    if (ptr == nullptr)
        return;

    uint64_t flags = irq_save();

    struct heap_block *block = (struct heap_block *)ptr - 1;

    if (block->free) {
        irq_restore(flags);
        return;                 /* double free: mark and bail, do not merge twice */
    }

    block->free = 1;
    coalesce();

    irq_restore(flags);
}
