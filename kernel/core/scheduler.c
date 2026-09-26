#include <stdint.h>
#include "pinx/mm/memory_management.h"
#include "pinx/scheduler.h"
#include "pinx/shell.h"
#include "pinx/terminal.h"

#define MAX_TASKS 64
#define TASK_CREATE_FAILED UINT64_MAX
#define TASK_STACK_SIZE (32 * 1024)
#define TASK_STACK_ALIGN 16

enum task_state {
    TASK_READY,
    TASK_RUNNING,
    TASK_BLOCKED,
    TASK_UNUSED,
    TASK_DEAD
};
struct task {
    uint64_t id;
    uint64_t rsp;
    void *stack;
    enum task_state state;
    struct task *next;
    void (*entry)(void);
};
static struct task tasks[MAX_TASKS];
static struct task kernel_task;
static struct task *current_task;

extern void context_switch(uint64_t *current_rsp, uint64_t new_rsp);

static void task_end(void) {
    current_task->state = TASK_DEAD;

    /*
     * The old version halted here forever, which meant a task returning from
     * its entry point took the whole machine down with it. Hand control back
     * to the kernel task instead; its stack is never popped past the switch
     * point, so this never returns.
     *
     * Note the task's stack is not reclaimed. TASK_DEAD slots are never
     * recycled, so the leak is bounded by MAX_TASKS for now.
     */
    scheduler_yield();
    panic("task_end: scheduler_yield returned");
}

uint64_t task_create(void (*entry)(void)) {
    if (entry == nullptr) {
        return TASK_CREATE_FAILED;
    }

    for (uint64_t i = 0; i != MAX_TASKS; i++) {
        if (tasks[i].state == TASK_UNUSED) {
            tasks[i].stack = kmalloc(TASK_STACK_SIZE);
            if (tasks[i].stack == nullptr) {
                return TASK_CREATE_FAILED;
            }
            tasks[i].entry = entry;
            tasks[i].state = TASK_READY;
            tasks[i].next = nullptr;

            /*
             * kmalloc hands back 16-byte aligned memory, but mask the top of
             * the stack anyway: the SysV ABI requires 16-byte alignment at
             * function entry, and a single missed assumption here only shows
             * up as a movaps #GP in some unrelated function later on.
             */
            uint64_t top = ((uint64_t)tasks[i].stack + TASK_STACK_SIZE)
                           & ~((uint64_t)TASK_STACK_ALIGN - 1);
            uint64_t *sp = (uint64_t *)top;

            /*
             * context_switch pops six callee-saved registers and then rets.
             * That leaves `entry` as the return address and task_end waiting
             * on the stack above it, so the task runs its entry point with a
             * plausible return address already in place. All six zeros are
             * required: one short and the pops eat `entry` itself.
             */
            *--sp = (uint64_t)task_end;
            *--sp = (uint64_t)entry;
            *--sp = 0;
            *--sp = 0;
            *--sp = 0;
            *--sp = 0;
            *--sp = 0;
            *--sp = 0;
            tasks[i].rsp = (uint64_t)sp;

            return tasks[i].id;
        }
    }
    return TASK_CREATE_FAILED;
}

/*
 * Pick the next task to run: the first READY slot after the current one,
 * falling back to the kernel task when nothing else is runnable. Returns
 * NULL when the current task should keep running.
 */
static struct task *scheduler_pick(void) {
    uint64_t start = (current_task->id + 1) % MAX_TASKS;

    for (uint64_t offset = 0; offset < MAX_TASKS; offset++) {
        uint64_t i = (start + offset) % MAX_TASKS;

        if (tasks[i].state == TASK_READY) {
            return &tasks[i];
        }
    }

    /*
     * Nothing runnable. The kernel task is outside tasks[] so it never shows up
     * in the scan above, which is what stops the machine from wedging when the
     * last user task dies.
     */
    if (current_task != &kernel_task) {
        return &kernel_task;
    }

    return nullptr;
}

void scheduler_yield(void)
{
    struct task *prev = current_task;
    struct task *next = scheduler_pick();

    if (next == nullptr || next == prev) {
        return;
    }

    /*
     * Only demote the outgoing task once we know a different one is taking
     * over. Setting the state first used to leave a RUNNING task marked READY
     * whenever the scan came up empty.
     */
    if (prev->state == TASK_RUNNING) {
        prev->state = TASK_READY;
    }

    current_task = next;
    current_task->state = TASK_RUNNING;

    context_switch(&prev->rsp, current_task->rsp);
}

void scheduler_init(void)
{
    for (uint64_t i = 0; i != MAX_TASKS; i++) {
        tasks[i].state = TASK_UNUSED;
        tasks[i].id = i;
        tasks[i].rsp = 0;
        tasks[i].stack = nullptr;
        tasks[i].next = nullptr;
        tasks[i].entry = nullptr;
    }

    kernel_task.id = UINT64_MAX;
    kernel_task.state = TASK_RUNNING;
    kernel_task.rsp = 0;
    kernel_task.stack = nullptr;
    kernel_task.next = nullptr;
    kernel_task.entry = nullptr;
    current_task = &kernel_task;

    task_create(run_shell);
}
