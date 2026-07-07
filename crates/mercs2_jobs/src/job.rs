//! The **job** — one dequeued Pimp work item + the fork/join completion fence.
//!
//! Code map §2 ("Job dispatch — the hot line"). A dequeued entry is a **≤0x60 / 96-byte** record
//! (the ring stride, [`JOB_ELEM_SIZE`]) laid out as:
//!
//! | off | field |
//! |---|---|
//! | +0x00 | **Jobtype index** → the per-CPU handler table `{handler@+0, jobtypeHash@+4}` (indexed `jobtype*2`) |
//! | +0x04 | **completion-fence `LONG*`** (`NULL` = fire-and-forget) |
//! | +0x08 | **param / context ptr** |
//! | +0x0c…0x60 | inline param list (the Xbox *"Too many params for this job"* cap) |
//!
//! The engine dispatches a job as two machine instructions (§2):
//!
//! ```c
//! (*(code*)(&PTR_LAB_019f904c)[cpu*0x4b + job[0]*2])(job[2]);  // handler(arg), keyed by Jobtype index
//! InterlockedExchangeAdd(job[1], 1);                           // fork/join done-counter
//! ```
//!
//! **Faithful collapse:** the exe keeps the *handler* (in a per-CPU handler table, keyed by Jobtype
//! index) and its *arg* (the +0x08 context ptr) separate, then calls `handler(arg)`. In a single
//! address space that indirection buys nothing, so we bind them into one `Box<dyn FnOnce()>` at
//! enqueue time — the closure captures the arg exactly as the engine passes `job[2]`. We keep the
//! **Jobtype hash** on the job for identity/routing (it is what the engine's handler table is keyed
//! by), and the completion fence stays a first-class field so fork/join is modeled, not collapsed.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

/// Ring element stride — the Job struct size, `0x60` = 96 bytes (code map §2/§3, the ring `elem_size`).
pub const JOB_ELEM_SIZE: usize = 0x60;

/// Registered Jobtype hashes recovered from the exe (§2, `hash → handler`). These are **data** — the
/// handlers are engine `FUN_` bodies (e.g. the `0x724xxx` **AnimCpu\*Job** trio) we do not reimplement
/// here; they are recorded so a caller can register its own handler under the authentic Jobtype hash.
///
/// | hash | handler (VA) | note |
/// |---|---|---|
/// | `0xcd4a518c` | `FUN_00724cc0` | AnimCpu\*Job candidate |
/// | `0x6b336727` | `FUN_00724480` | AnimCpu\*Job candidate |
/// | `0xcad07407` | `FUN_00724c20` | AnimCpu\*Job candidate |
/// | `0xc28ef815` | `FUN_0084ac00` | |
/// | `0x3300b3c8` | `FUN_0082c100` | |
/// | `0x84f0d9c6` | `FUN_008780d0` | |
/// | `0xebfea356` | `FUN_0082d040` | |
pub const REGISTERED_JOBTYPES: [u32; 7] = [
    0xcd4a518c, 0x6b336727, 0xcad07407, 0xc28ef815, 0x3300b3c8, 0x84f0d9c6, 0xebfea356,
];

/// A job's dispatch priority. The worker (§1) keeps **two per-CPU queues** — `+0x04` high-pri and
/// `+0x08` low-pri — and drains them asymmetrically (high fully, then *one* low per iteration).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Priority {
    /// `+0x04` high-priority queue — drained **completely** every worker iteration.
    High,
    /// `+0x08` low-priority queue — **one** entry serviced per worker iteration.
    Low,
}

/// The fork/join **completion fence** — the exe's `job[1]` `LONG*` incremented by the *only*
/// Interlocked op left in the whole system: `InterlockedExchangeAdd(completion, 1)` (§2/§3).
///
/// A submitter that forks N jobs sharing one fence joins by waiting for the fence to reach N. We back
/// it with an [`AtomicU32`] so [`signal`](Self::signal) *is* the atomic add — the one place the engine
/// still needs a memory fence rather than the surrounding critical section. A job with **no** fence
/// (`+0x04 == NULL`) is fire-and-forget.
#[derive(Clone, Debug, Default)]
pub struct CompletionFence(Arc<AtomicU32>);

impl CompletionFence {
    /// A fresh fence at 0 (no jobs completed yet).
    pub fn new() -> Self {
        CompletionFence(Arc::new(AtomicU32::new(0)))
    }

    /// `InterlockedExchangeAdd(completion, 1)` — record one completed job. Called by the worker after
    /// the handler returns.
    pub fn signal(&self) {
        self.0.fetch_add(1, Ordering::AcqRel);
    }

    /// Current completion count — the join test (`fence.count() == forked_n`).
    pub fn count(&self) -> u32 {
        self.0.load(Ordering::Acquire)
    }
}

/// One Pimp work item: its Jobtype hash (identity — what the per-CPU handler table is keyed by), an
/// optional [`CompletionFence`] (`None` = fire-and-forget), and the bound `handler(arg)` payload.
pub struct Job {
    /// Jobtype hash — the `+0x04` field of the handler-table entry the exe dispatches through. Kept
    /// for identity/routing; `0` for an ad-hoc closure job with no registered Jobtype.
    pub jobtype: u32,
    /// `+0x04` completion-fence `LONG*` — `None` when fire-and-forget.
    pub fence: Option<CompletionFence>,
    /// The bound `handler(arg)` call (§2's collapse — see the module docs).
    work: Box<dyn FnOnce()>,
}

impl Job {
    /// Build a job from a bound handler-call closure, an optional Jobtype hash, and an optional fence.
    pub fn new(jobtype: u32, fence: Option<CompletionFence>, work: impl FnOnce() + 'static) -> Self {
        Job { jobtype, fence, work: Box::new(work) }
    }

    /// A fire-and-forget job with no registered Jobtype (`jobtype = 0`, `fence = None`).
    pub fn from_fn(work: impl FnOnce() + 'static) -> Self {
        Job::new(0, None, work)
    }

    /// Run the handler, then (§2) fire the completion fence if one is attached. This is the exact
    /// two-step the worker performs per dequeued entry: `handler(arg)` → `InterlockedExchangeAdd`.
    pub fn run(self) {
        (self.work)();
        if let Some(fence) = self.fence {
            fence.signal();
        }
    }
}

impl std::fmt::Debug for Job {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Job")
            .field("jobtype", &format_args!("{:#010x}", self.jobtype))
            .field("has_fence", &self.fence.is_some())
            .finish_non_exhaustive()
    }
}
