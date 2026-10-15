//! Preemptive round-robin scheduler over ring-3 tasks.
//!
//! There is one global run queue guarded by [`RUNQ`], and every CPU keeps its
//! own *current* pointer in `PERCPU`. A task is either `Ready`, `Running` (on
//! exactly one CPU), `Blocked`, or `Dead`. User tasks all share one address
//! space; only their kernel stacks and register state differ.
//!
//! Scheduling happens on the return path of an interrupt: the trap handler
//! saves the interrupted context as an [`InterruptFrame`] and calls
//! [`schedule`], which hands back the frame to resume. `asm.rs` then loads that
//! frame's `rsp` and `iretq`s into it, so both user tasks and the per-CPU idle
//! task resume through the exact same path.

use crate::apic;
use crate::gdt::{
    self, KERNEL_CODE_SELECTOR, KERNEL_DATA_SELECTOR, USER_CODE_SELECTOR, USER_DATA_SELECTOR,
};
use crate::interrupts::InterruptFrame;
use crate::io;
use crate::mm::kmalloc;
use crate::smp;
use crate::spinlock::SpinLock;
use core::sync::atomic::Ordering;

const MAX_TASKS: usize = 64;
const KERNEL_STACK_SIZE: u64 = 64 * 1024;

#[derive(Copy, Clone, PartialEq, Eq)]
enum TaskState {
    Unused,
    Ready,
    Running,
    /// Parked: not in the run queue until something wakes it. `wake_tick`/`wait_input`
    /// say what it is waiting for.
    Blocked,
    Dead,
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum TaskKind {
    User,
    Idle,
}

#[repr(C)]
struct Task {
    state: TaskState,
    kind: TaskKind,
    cpu: usize,
    /// Where the task's saved context sits on its kernel stack.
    frame: *mut InterruptFrame,
    /// Top of the kernel stack (`rsp0` while the task runs).
    kstack: u64,
    /// If blocked on time: the `apic::uptime_ms()` value at which to wake. Zero
    /// when the wake is not time-based.
    wake_tick: u64,
    /// If blocked on input: set so the keyboard IRQ can find and wake it.
    wait_input: bool,
}

const EMPTY_TASK: Task = Task {
    state: TaskState::Unused,
    kind: TaskKind::Idle,
    cpu: 0,
    frame: core::ptr::null_mut(),
    kstack: 0,
    wake_tick: 0,
    wait_input: false,
};

#[repr(C)]
struct PerCpu {
    current: *mut Task,
    /// A task this CPU switched away from but has not published as `Ready`
    /// yet. It is only safe to make it runnable once this CPU has actually
    /// finished the switch, which is why the flag is deferred to the next
    /// scheduling pass on the same CPU.
    deferred: *mut Task,
}

const EMPTY_PERCPU: PerCpu = PerCpu {
    current: core::ptr::null_mut(),
    deferred: core::ptr::null_mut(),
};

static RUNQ: SpinLock = SpinLock::new();
/// Set once `init_global` has finished. Until then the timer may fire (the
/// APIC timer is started before the scheduler exists) and `schedule` must leave
/// the interrupted context alone.
static INITIALIZED: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
static mut TASKS: [Task; MAX_TASKS] = [EMPTY_TASK; MAX_TASKS];
static mut IDLE: [Task; gdt::MAX_CPUS] = [EMPTY_TASK; gdt::MAX_CPUS];
static mut PERCPU: [PerCpu; gdt::MAX_CPUS] = [EMPTY_PERCPU; gdt::MAX_CPUS];

/// `15 GPRs + vector + error code + RIP/CS/RFLAGS/RSP/SS`.
const FRAME_WORDS: usize = 22;

unsafe fn task_slot(i: usize) -> *mut Task {
    core::ptr::addr_of_mut!(TASKS[i])
}

unsafe fn per_cpu(cpu: usize) -> *mut PerCpu {
    core::ptr::addr_of_mut!(PERCPU[cpu])
}

unsafe fn idle_slot(cpu: usize) -> *mut Task {
    core::ptr::addr_of_mut!(IDLE[cpu])
}

unsafe fn frame_at(kstack_top: u64) -> *mut u64 {
    (kstack_top - (FRAME_WORDS * 8) as u64) as *mut u64
}

/// Build a ring-3 frame: the context `iretq` lowers privilege to.
unsafe fn build_user_frame(kstack_top: u64, entry: u64, stack: u64) -> *mut InterruptFrame {
    let f = frame_at(kstack_top);
    for i in 0..15 {
        *f.add(i) = 0;
    }
    *f.add(15) = crate::idt::SYSCALL_VECTOR as u64;
    *f.add(16) = 0;
    *f.add(17) = entry;
    *f.add(18) = USER_CODE_SELECTOR as u64;
    *f.add(19) = 0x202;
    *f.add(20) = stack;
    *f.add(21) = USER_DATA_SELECTOR as u64;
    f as *mut InterruptFrame
}

/// Build a ring-0 frame for an idle CPU, resuming in [`idle_loop`].
unsafe fn build_idle_frame(kstack_top: u64) -> *mut InterruptFrame {
    let f = frame_at(kstack_top);
    for i in 0..15 {
        *f.add(i) = 0;
    }
    *f.add(15) = apic::TIMER_VECTOR as u64;
    *f.add(16) = 0;
    *f.add(17) = idle_loop as *const () as u64;
    *f.add(18) = KERNEL_CODE_SELECTOR as u64;
    *f.add(19) = 0x202;
    *f.add(20) = kstack_top;
    *f.add(21) = KERNEL_DATA_SELECTOR as u64;
    f as *mut InterruptFrame
}

unsafe fn pick(cur: *mut Task) -> *mut Task {
    for i in 0..MAX_TASKS {
        let t = task_slot(i);
        if (*t).state == TaskState::Ready && t != cur {
            return t;
        }
    }
    core::ptr::null_mut()
}

/// Set up the shared tables and one idle task per discovered CPU. Must run
/// after `smp::discover` (so the CPU count is known) and before any AP starts,
/// since the APs call [`init_cpu`] the moment they are released.
pub fn init_global() {
    unsafe {
        for i in 0..MAX_TASKS {
            (*task_slot(i)) = EMPTY_TASK;
        }

        for cpu in 0..smp::cpu_count() {
            let top = gdt::kstack_top(cpu);
            let idle = idle_slot(cpu);
            (*idle).state = TaskState::Unused;
            (*idle).kind = TaskKind::Idle;
            (*idle).cpu = cpu;
            (*idle).kstack = top;
            (*idle).frame = build_idle_frame(top);
        }
    }
    INITIALIZED.store(true, Ordering::Release);
}

/// Point CPU `cpu` at its idle task and arm its `rsp0`. Called by the BSP just
/// before it idles, and by each AP from [`smp`]'s entry point.
pub fn init_cpu(cpu: usize) {
    unsafe {
        let idle = idle_slot(cpu);
        (*idle).state = TaskState::Running;
        let per = per_cpu(cpu);
        (*per).current = idle;
        (*per).deferred = core::ptr::null_mut();
        gdt::set_kernel_stack(cpu, (*idle).kstack);
    }
}

/// Stop this CPU until the next interrupt. The timer interrupt wakes it and
/// [`schedule`] may hand it a real task to resume.
pub extern "C" fn idle_loop() -> ! {
    loop {
        io::hlt();
    }
}

/// Register a new ring-3 task starting at `entry` with its user stack top at
/// `user_stack`. Returns the task's slot, or `None` when the table is full or
/// the kernel stack could not be allocated.
pub fn spawn_user(entry: u64, user_stack: u64) -> Option<usize> {
    // Allocate before taking the lock: `kmalloc` is not reentrant and should
    // not run with interrupts disabled any longer than necessary.
    let stack = kmalloc(KERNEL_STACK_SIZE);
    if stack.is_null() {
        return None;
    }
    let top = (stack as u64 + KERNEL_STACK_SIZE) & !0xF;

    let _guard = RUNQ.lock();
    unsafe {
        for i in 0..MAX_TASKS {
            let t = task_slot(i);
            if (*t).state != TaskState::Unused {
                continue;
            }
            (*t).state = TaskState::Ready;
            (*t).kind = TaskKind::User;
            (*t).cpu = 0;
            (*t).kstack = top;
            (*t).frame = build_user_frame(top, entry, user_stack);
            (*t).wake_tick = 0;
            (*t).wait_input = false;
            return Some(i);
        }
    }
    None
}

/// Mark this CPU's current task dead so [`schedule`] will not pick it again.
pub fn mark_current_dead() {
    let cpu = smp::cpu_index();
    let _guard = RUNQ.lock();
    unsafe {
        let cur = (*per_cpu(cpu)).current;
        if !cur.is_null() {
            (*cur).state = TaskState::Dead;
        }
    }
}

/// Park the current task until `wake_tick` (an [`apic::uptime_ms`] value). The
/// caller must follow with [`schedule`] to actually leave the CPU.
pub fn block_current_until(wake_tick: u64) {
    let cpu = smp::cpu_index();
    let _guard = RUNQ.lock();
    unsafe {
        let cur = (*per_cpu(cpu)).current;
        if !cur.is_null() {
            (*cur).state = TaskState::Blocked;
            (*cur).wake_tick = wake_tick;
            (*cur).wait_input = false;
        }
    }
}

/// Park the current task until input arrives. The caller must follow with
/// [`schedule`].
pub fn block_current_input() {
    let cpu = smp::cpu_index();
    let _guard = RUNQ.lock();
    unsafe {
        let cur = (*per_cpu(cpu)).current;
        if !cur.is_null() {
            (*cur).state = TaskState::Blocked;
            (*cur).wake_tick = 0;
            (*cur).wait_input = true;
        }
    }
}

/// Make runnable every task whose sleep deadline has passed. Called from the
/// timer interrupt on every CPU; the global clock is `now`.
pub fn wake_sleepers(now: u64) {
    let _guard = RUNQ.lock();
    unsafe {
        for i in 0..MAX_TASKS {
            let t = task_slot(i);
            if (*t).state == TaskState::Blocked
                && !(*t).wait_input
                && (*t).wake_tick != 0
                && (*t).wake_tick <= now
            {
                (*t).state = TaskState::Ready;
                (*t).wake_tick = 0;
            }
        }
    }
}

/// Make runnable every task blocked on input. Called from the keyboard IRQ once
/// the key has been buffered.
pub fn wake_input_waiters() {
    let _guard = RUNQ.lock();
    unsafe {
        for i in 0..MAX_TASKS {
            let t = task_slot(i);
            if (*t).state == TaskState::Blocked && (*t).wait_input {
                (*t).state = TaskState::Ready;
                (*t).wait_input = false;
            }
        }
    }
}

/// Choose what this CPU runs next given the frame it was interrupted on, and
/// return the frame to resume. The caller (`asm.rs`) swaps `rsp` to it.
///
/// # Safety
/// Must be called from the interrupt path with interrupts disabled.
pub unsafe fn schedule(frame: *mut InterruptFrame) -> *mut InterruptFrame {
    // Before the scheduler exists the timer still fires; keep running whatever
    // was interrupted.
    if !INITIALIZED.load(Ordering::Acquire) {
        return frame;
    }

    let cpu = smp::cpu_index();
    let _guard = RUNQ.lock();

    let per = per_cpu(cpu);

    // A task that was switched away from last time has definitely left this
    // CPU by now, so it is finally safe to mark it runnable.
    if !(*per).deferred.is_null() {
        (*(*per).deferred).state = TaskState::Ready;
        (*per).deferred = core::ptr::null_mut();
    }

    let cur = (*per).current;
    if !cur.is_null() {
        (*cur).frame = frame;
    }

    let mut next = pick(cur);
    if next.is_null() {
        // Nothing else is runnable.
        let keep_current = !cur.is_null()
            && (*cur).kind == TaskKind::User
            && (*cur).state == TaskState::Running;
        if keep_current {
            next = cur;
        } else if !cur.is_null() && (*cur).kind == TaskKind::Idle {
            // Already running this CPU's idle context (the idle loop, or the
            // fallback kernel shell). There is nowhere better to go; continue
            // the interrupted context rather than jump to the idle frame.
            return frame;
        } else {
            // Dead user task, or no current at all: fall back to idle.
            next = idle_slot(cpu);
        }
    }

    if next != cur {
        if !cur.is_null() && (*cur).kind == TaskKind::User && (*cur).state == TaskState::Running {
            // Still executing on this CPU's stack until the caller's `iretq`;
            // publish it only on the next pass.
            (*per).deferred = cur;
        }
        (*per).current = next;
        gdt::set_kernel_stack(cpu, (*next).kstack);
    }

    (*next).state = TaskState::Running;
    (*next).cpu = cpu;
    (*next).frame
}
