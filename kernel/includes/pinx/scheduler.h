#pragma once
#include <stdint.h>

#define TASK_CREATE_FAILED UINT64_MAX

void scheduler_init(void);
void scheduler_yield(void);
uint64_t task_create(void (*entry)(void));