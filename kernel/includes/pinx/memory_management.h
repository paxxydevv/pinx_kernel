#pragma once
#include "stdint.h"
void pmm_init(void);
uint64_t pmm_alloc_page(void);
void pmm_free_page(uint64_t page);
void vmm_init(void);
void vmm_map_page(uint64_t virtual, uint64_t physical);
void vmm_unmap_page(uint64_t virtual);