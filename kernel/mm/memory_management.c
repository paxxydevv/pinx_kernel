#include "pinx/mm/memory_management.h"
#include "3party/limine.h"
#include <stdint.h>
extern volatile struct limine_memmap_request memmap_request;
extern volatile struct limine_hhdm_request hhdm_request;
static uint64_t *bitmap;
static uint64_t bitmap_entries;
#define PAGE_SIZE 4096
#define PMM_NO_PAGE UINT64_MAX
#define KHEAP_START 0xFFFF900000000000ULL
#define KHEAP_END   0xFFFFA00000000000ULL
static uint64_t *pml4;
static struct heap_block *heap_start;
struct heap_block {
    uint64_t size;
    bool free;
    struct heap_block *next;
};

static uint64_t find_bitmap_region(uint64_t bitmap_size) {
    for (uint64_t i = 0; i < memmap_request.response->entry_count; i++) {
        struct limine_memmap_entry *entry = memmap_request.response->entries[i];

        if (entry->type != LIMINE_MEMMAP_USABLE)
            continue;
        if (entry->length >= bitmap_size)
            return entry->base;
    }
    return 0;
}
void pmm_init() {
    if (memmap_request.response == nullptr)
        return;
    uint64_t highest_address = 0;

    for (uint64_t i = 0; i < memmap_request.response->entry_count; i++) {
        struct limine_memmap_entry *entry = memmap_request.response->entries[i];
        uint64_t end = entry->base + entry->length;
        if (end > highest_address) {
            highest_address = end;
        }
    }
    uint64_t total_pages = highest_address / 4096;
    bitmap_entries = (total_pages + 63) / 64;
    uint64_t bitmap_size = bitmap_entries * sizeof(uint64_t);
    uint64_t bitmap_physical = find_bitmap_region(bitmap_size);
    if (bitmap_physical == 0)
        return;
    uint64_t bitmap_pages = (bitmap_size + PAGE_SIZE - 1) / PAGE_SIZE;
    bitmap = (uint64_t *)(bitmap_physical + hhdm_request.response->offset);
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
uint64_t pmm_alloc_page() {
    for (uint64_t i = 0; i < bitmap_entries; i++) {
        if (bitmap[i] != UINT64_MAX) {
            for (uint64_t bit = 0; bit < 64; bit++) {
                if ((bitmap[i] & (1ULL << bit)) == 0) {
                    uint64_t page = i * 64 + bit;
                    bitmap[i] |= (1ULL << bit);
                    return page * PAGE_SIZE;
                }
            }
        }
    }
    return PMM_NO_PAGE;
}
void pmm_free_page(uint64_t page) {
    uint64_t page_number = page / PAGE_SIZE;
    bitmap[page_number / 64] &= ~(1ULL << (page_number % 64));
}
void vmm_init(void) {
    uint64_t cr3;
    __asm__ volatile(
        "mov %%cr3, %0"
        : "=r"(cr3)
    );
    if (cr3 == 0)
        return;
    uint64_t pml4_physical = cr3 & ~0xFFFULL;
    pml4 = (uint64_t *)(pml4_physical + hhdm_request.response->offset);
}
void vmm_map_page(uint64_t virtual, uint64_t physical) {
    uint64_t pml4_index = (virtual >> 39) & 0x1FF;
    uint64_t pdpt_index = (virtual >> 30) & 0x1FF;
    uint64_t pd_index   = (virtual >> 21) & 0x1FF;
    uint64_t pt_index   = (virtual >> 12) & 0x1FF;
    uint64_t pml4_entry = pml4[pml4_index];
    if (!(pml4_entry & 1)) {
    uint64_t new_page = pmm_alloc_page();
    if (new_page == PMM_NO_PAGE)
        return;

    pml4[pml4_index] = new_page | 0x3;

    uint64_t *pdpt = (uint64_t *)(new_page + hhdm_request.response->offset);
    for (uint64_t i = 0; i < 512; i++)
        pdpt[i] = 0;
    }
    pml4_entry = pml4[pml4_index];
    uint64_t pdpt_physical = pml4_entry & ~0xFFFULL;
    uint64_t *pdpt = (uint64_t *)(pdpt_physical + hhdm_request.response->offset);


    uint64_t pdpt_entry = pdpt[pdpt_index];
    if (!(pdpt_entry & 1)) {
    uint64_t new_page = pmm_alloc_page();
    if (new_page == PMM_NO_PAGE)
        return;

    pdpt[pdpt_index] = new_page | 0x3;

    uint64_t *pd = (uint64_t *)(new_page + hhdm_request.response->offset);
    for (uint64_t i = 0; i < 512; i++)
        pd[i] = 0;
    }
    pdpt_entry = pdpt[pdpt_index];
    uint64_t pd_physical = pdpt_entry & ~0xFFFULL;
    uint64_t *pd = (uint64_t *)(pd_physical + hhdm_request.response->offset);

    uint64_t pd_entry = pd[pd_index];
    if (!(pd_entry & 1)) {
    uint64_t new_page = pmm_alloc_page();
    if (new_page == PMM_NO_PAGE)
        return;

    pd[pd_index] = new_page | 0x3;

    uint64_t *pt = (uint64_t *)(new_page + hhdm_request.response->offset);
    for (uint64_t i = 0; i < 512; i++)
        pt[i] = 0;
    }
    pd_entry = pd[pd_index];
    uint64_t pt_physical = pd_entry & ~0xFFFULL;
    uint64_t *pt = (uint64_t *)(pt_physical + hhdm_request.response->offset);
    pt[pt_index] = physical | 0x3;
}
void vmm_unmap_page(uint64_t virtual) {
    uint64_t pml4_index = (virtual >> 39) & 0x1FF;
    uint64_t pdpt_index = (virtual >> 30) & 0x1FF;
    uint64_t pd_index   = (virtual >> 21) & 0x1FF;
    uint64_t pt_index   = (virtual >> 12) & 0x1FF;

    // PML4
    uint64_t pml4_entry = pml4[pml4_index];

    if (!(pml4_entry & 1))
        return;

    uint64_t pdpt_physical = pml4_entry & ~0xFFFULL;
    uint64_t *pdpt =
        (uint64_t *)(pdpt_physical + hhdm_request.response->offset);

    // PDPT
    uint64_t pdpt_entry = pdpt[pdpt_index];

    if (!(pdpt_entry & 1))
        return;

    uint64_t pd_physical = pdpt_entry & ~0xFFFULL;
    uint64_t *pd =
        (uint64_t *)(pd_physical + hhdm_request.response->offset);

    // PD
    uint64_t pd_entry = pd[pd_index];

    if (!(pd_entry & 1))
        return;

    uint64_t pt_physical = pd_entry & ~0xFFFULL;
    uint64_t *pt =
        (uint64_t *)(pt_physical + hhdm_request.response->offset);

    // PT
    if (!(pt[pt_index] & 1))
        return;

    pt[pt_index] = 0;

    // Forget the old virtual -> physical translation in the TLB.
    __asm__ volatile(
        "invlpg (%0)"
        :
        : "r"(virtual)
        : "memory"
    );
}
void kheap_init() {
    uint64_t page = pmm_alloc_page();
    if (page == PMM_NO_PAGE)
        return;
    vmm_map_page(KHEAP_START, page);
    heap_start = (struct heap_block *)KHEAP_START;
    heap_start->size = PAGE_SIZE - sizeof(struct heap_block); // 4000 ISH bytes
    heap_start->free = true;
    heap_start->next = nullptr;
}
void *kmalloc(uint64_t size)
{
    if (size == 0 || size == UINT64_MAX)
        return nullptr;
    struct heap_block *current = heap_start;
    while (current != nullptr) {
        if (current->free && current->size >= size) {

            if (current->size > size + sizeof(struct heap_block)) {
                struct heap_block *next =
                    (struct heap_block *)((uint8_t *)(current + 1) + size);

                next->free = true;
                next->next = current->next;
                next->size =
                    current->size - size - sizeof(struct heap_block);

                current->size = size;
                current->next = next;
            }

            current->free = false;

            return (void *)(current + 1);
        }

        current = current->next;
    }

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
    struct heap_block *block = (struct heap_block *)ptr - 1;
    block->free = true;
    coalesce();
}