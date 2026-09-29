#pragma once
#include "stdint.h"

/*
 * Returned by vmm_virt_to_phys when the address is not mapped.
 *
 * UINT64_MAX rather than 0: the frame at physical 0 is a real frame that
 * Limine reports as available, so pmm_alloc_page can hand it out and the PMM
 * can map it. 0 would be an ambiguous answer, whereas no physical address can
 * have all 52 of its address bits set.
 */
#define VMM_NO_PHYS UINT64_MAX

/*
 * Returned by pmm_alloc_page when the bitmap is exhausted.
 *
 * Same reasoning as VMM_NO_PHYS: 0 is a real, allocatable frame, so it cannot
 * double as a failure value. Lives here rather than in the .c because a caller
 * outside memory_management.c has to be able to test for it.
 */
#define PMM_NO_PAGE UINT64_MAX

void pmm_init(void);
uint64_t pmm_alloc_page(void);
void pmm_free_page(uint64_t page);
void vmm_init(void);
void vmm_map_page(uint64_t virtual, uint64_t physical);
void vmm_map_page_flags(uint64_t virtual, uint64_t physical, uint64_t flags);
void vmm_map_mmio(uint64_t virtual, uint64_t physical);
void vmm_unmap_page(uint64_t virtual);
uint64_t vmm_virt_to_phys(uint64_t virtual);
void kheap_init();
void *kmalloc(uint64_t size);
void kfree(void* ptr);