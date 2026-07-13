//! `mercs2_jobs` — the **Pimp job system** (scoreboard row 15), the engine's parallel-work spine.
//!
//! **Code map:** `docs/reverse_engineer/pimp_job_system_code_map.md` (companion to the Keystone PC
//! code-map recovery — master tick order, event bus, Pimp jobs). Xbox oracle
//! `docs/mercs2-pdb-analysis/jobs-threading.md`.
//!
//! Pandemic's in-house **"Pimp"** multi-CPU job library (Xbox source `d:\mainline\mercs2\pimp\`) was
//! the scoreboard's *"true novel system"* — **100% string-only on Xbox** (0 decomp hits). The PC build
//! keeps the real architecture, recovered in the code map:
//!
//! - a **per-CPU worker pool** ([`WorkerPool`], §1) — one worker per CPU (`FUN_00876400`), CPU count
//!   from `GetProcessAffinityMask` popcount (`pimpGetNumCpus FUN_008767b0`);
//! - a **CS-guarded bounded ring** ([`ring::JobRing`], §3) — the 360's lock-free VMX "a64" queue
//!   degraded to a `CRITICAL_SECTION`-guarded 96-byte-element ring on x86;
//! - **96-byte jobs** ([`Job`], §2) — `{Jobtype index, completion-fence LONG*, param ptr, inline params}`,
//!   dispatched `handler(arg)` then `InterlockedExchangeAdd(fence, 1)` (the *only* Interlocked op left);
//! - **asymmetric drain priority** (§1) — high-pri queue drained fully, then **one** low-pri entry per
//!   worker iteration.
//!
//! ## Faithful synchronous model
//!
//! The reimpl render loop is single-threaded, so this crate models the **job-graph / ordering
//! semantics** — which is the load-bearing value — without OS threads. Exclusive `&mut` access *is* the
//! critical section; a single producer-in-flight makes the multi-producer reservation spin a no-op
//! (see [`ring`]). The default [`WorkerPool::run_all`] is a **deterministic** linearization (high
//! before low, FIFO within each); [`WorkerPool::run_worker_iteration`] reproduces the exact per-worker
//! drain policy. Where the exe's N workers give a nondeterministic interleave, the **fork/join
//! [`CompletionFence`]** is how a submitter recovers an ordering guarantee — modeled first-class.
//!
//! ## Where dispatch sits in the frame
//!
//! (Cross-ref `scheduler_tick_code_map.md` — the master tick is `mercs2_core::frame::LayerStack`, a
//! 5-layer 0→4 climb.) The Pimp workers run **continuously, parallel to `RunFrame`**; the sim *submits*
//! jobs while it ticks — notably the streaming LOD-budget notify `FUN_0084ae70` fired inside the master
//! update `FUN_004c14f0` (before the layer-4 gameplay walk), and the `0x724xxx` **AnimCpu\*Job** trio
//! forked during animation. A submitter that needs results *this frame* joins on the fence before
//! reading them. In the single-threaded model this maps to: **submit during the tick → drain (run_all)
//! at the fork/join point → the fence reads back complete.** The crate is not wired into the engine
//! (that is the integration layer's job); it provides the mechanism.
//!
//! ## Deliberately NOT built (code map marks unrecovered / confirm-live)
//!
//! - the worker `CreateThread` spawn site + per-CPU control-block alloc loop live in the
//!   **SecuROM-relocated `pimpInit`** island (§1 `callers=[]`, spawn site *confirm-live*) — we model the
//!   pool shape the spawn produces, not the relocated spawn code;
//! - the per-CPU **RootTimer profiler** node struct and the PC **profiler-zone table global** +
//!   `SyncCPUGPU` fence have their name strings **stripped** (§4/§5, *confirm-live*) — the ms/µs QPC
//!   scales prove the subsystem exists but the node layout is not string-grounded, so no profiler tree
//!   is invented here;
//! - the registered Jobtype **handlers** themselves are engine `FUN_` bodies (§2) — recorded as hashes
//!   in [`job::REGISTERED_JOBTYPES`], not reimplemented (they belong to the animation/streaming crates).
//!
//! ## Module map
//!
//! | module | owns |
//! |---|---|
//! | [`job`] | [`Job`] (Jobtype hash + optional fence + bound handler closure), [`CompletionFence`], [`Priority`], [`JOB_ELEM_SIZE`] (`0x60`), [`REGISTERED_JOBTYPES`] (the 7 recovered hashes). |
//! | [`ring`] | [`JobRing`] — the bounded CS-guarded FIFO — plus the recovered capacities [`RING_CAP_INIT`] (`0x10`), [`RING_CAP_WORKER_ITER`] (`0x1000`), [`RING_CAP_WORKER_DRAIN`] (`0x400`). |
//!
//! The crate root owns [`WorkerPool`] (submit / fork / drain) and [`pimp_num_cpus`].
//!
//! ## Example — fork/join
//!
//! ```
//! use mercs2_jobs::{CompletionFence, Job, Priority, WorkerPool};
//! use std::cell::RefCell;
//! use std::rc::Rc;
//!
//! let mut pool = WorkerPool::new();          // one worker per usable CPU (pimpGetNumCpus)
//! let fence = CompletionFence::new();
//! let sum = Rc::new(RefCell::new(0u32));
//! for i in 1..=5u32 {
//!     let s = sum.clone();
//!     // 0xcd4a518c = a recovered AnimCpu*Job Jobtype hash; pass 0 for an ad-hoc closure.
//!     pool.fork(Priority::High, 0xcd4a518c, &fence, move || *s.borrow_mut() += i).unwrap();
//! }
//! pool.submit(Priority::Low, Job::from_fn(|| {})).unwrap();  // fire-and-forget, no fence
//!
//! assert_eq!(fence.count(), 0);   // nothing has run yet
//! pool.run_all();                 // the fork/join point: drain both rings
//! assert_eq!(fence.count(), 5);   // the join sees all 5 forked jobs complete
//! assert_eq!(*sum.borrow(), 15);
//! ```

pub mod job;
pub mod ring;

pub use job::{CompletionFence, Job, Priority, JOB_ELEM_SIZE, REGISTERED_JOBTYPES};
pub use ring::{JobRing, RING_CAP_INIT, RING_CAP_WORKER_DRAIN, RING_CAP_WORKER_ITER};

/// The recovered CPU-count query `pimpGetNumCpus` (`FUN_008767b0`): the exe does
/// `GetProcessAffinityMask` then popcounts the mask to size the worker pool. The portable analog of
/// "usable CPUs for this process" is [`std::thread::available_parallelism`]; we fall back to 1 when it
/// is unavailable (a single-CPU pool is still a valid, if serial, Pimp pool).
pub fn pimp_num_cpus() -> usize {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1)
}

/// The per-CPU **worker pool** (§1) — the parallel-work spine. Holds the two priority rings the
/// worker drains (`+0x04` high-pri / `+0x08` low-pri) and the recovered worker count. In the exe each
/// CPU owns its own pair of rings drained by its own `FUN_00876400` thread; the *observable* contract
/// — jobs submitted at a priority are drained high-before-low, FIFO within a priority — is the same
/// whether one thread or `num_workers` drain it, so the single-threaded model owns one pair and runs
/// the identical drain policy.
pub struct WorkerPool {
    /// `+0x04` high-priority queue — drained **fully** each worker iteration. Sized to the worker
    /// per-iteration ring capacity [`RING_CAP_WORKER_ITER`] (`0x1000`).
    high: JobRing,
    /// `+0x08` low-priority queue — **one** entry serviced per worker iteration. Sized to the worker
    /// drain-ring capacity [`RING_CAP_WORKER_DRAIN`] (`0x400`).
    low: JobRing,
    /// Worker count the exe would spawn (`pimpGetNumCpus` popcount). Recorded for fidelity; the
    /// deterministic drain runs them as one linearization (see the crate docs).
    num_workers: usize,
}

impl Default for WorkerPool {
    fn default() -> Self {
        WorkerPool::new()
    }
}

impl WorkerPool {
    /// A pool sized to [`pimp_num_cpus`] — the faithful default (one worker per usable CPU).
    pub fn new() -> Self {
        WorkerPool::with_workers(pimp_num_cpus())
    }

    /// A pool with an explicit worker count (>= 1). The ring capacities are fixed to the recovered
    /// values regardless of worker count (they are per-ring, not per-worker, in the exe).
    pub fn with_workers(num_workers: usize) -> Self {
        WorkerPool {
            high: JobRing::new(RING_CAP_WORKER_ITER),
            low: JobRing::new(RING_CAP_WORKER_DRAIN),
            num_workers: num_workers.max(1),
        }
    }

    /// Worker count (`pimpGetNumCpus`).
    pub fn num_workers(&self) -> usize {
        self.num_workers
    }

    /// Submit a fully-built [`Job`] at a [`Priority`]. Returns `Err(job)` (handing the work back) if
    /// that priority's ring is full — the exe's in-CS producer would spin-wait for a slot; single
    /// threaded we surface it as backpressure.
    pub fn submit(&mut self, priority: Priority, job: Job) -> Result<(), Job> {
        match priority {
            Priority::High => self.high.push(job),
            Priority::Low => self.low.push(job),
        }
    }

    /// Fork a closure onto the pool under a registered Jobtype hash, sharing a [`CompletionFence`] so a
    /// later `fence.count()` joins the fork. Convenience over [`submit`](Self::submit) for the common
    /// "fork N, join on a fence" pattern (§2/§3). `jobtype` may be `0` for an ad-hoc closure.
    pub fn fork(
        &mut self,
        priority: Priority,
        jobtype: u32,
        fence: &CompletionFence,
        work: impl FnOnce() + 'static,
    ) -> Result<(), Job> {
        self.submit(priority, Job::new(jobtype, Some(fence.clone()), work))
    }

    /// Number of jobs currently queued across both rings.
    pub fn pending(&self) -> usize {
        self.high.len() + self.low.len()
    }

    pub fn is_idle(&self) -> bool {
        self.high.is_empty() && self.low.is_empty()
    }

    /// Run **one worker iteration** exactly as `FUN_00876400` (§1): drain the high-pri queue **fully**,
    /// then service **one** low-pri entry (if any). Returns the number of jobs run. This is the atom of
    /// the pool's drain policy — call it repeatedly (as the worker's infinite loop does) to progress.
    pub fn run_worker_iteration(&mut self) -> usize {
        let mut ran = 0;
        while let Some(job) = self.high.pop() {
            job.run();
            ran += 1;
        }
        if let Some(job) = self.low.pop() {
            job.run();
            ran += 1;
        }
        ran
    }

    /// Drain **everything** deterministically — repeat [`run_worker_iteration`](Self::run_worker_iteration)
    /// until both rings are empty. This is the fork/join point: submit during the tick, `run_all` at the
    /// join, then read fences back complete. Ordering is a fixed linearization (all high FIFO, then the
    /// low queue FIFO one-per-iteration) — deterministic where the exe's N workers would interleave.
    ///
    /// (A true parallel pool would fan the same rings out to [`num_workers`](Self::num_workers) threads
    /// here; the fence semantics are identical, and the default stays deterministic on purpose.)
    pub fn run_all(&mut self) -> usize {
        let mut total = 0;
        while !self.is_idle() {
            total += self.run_worker_iteration();
        }
        total
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// The scaffold link the crate shipped with still holds (core is reachable).
    #[test]
    fn scaffold_links() {
        let _ = mercs2_core::Time::new(60.0);
    }

    /// The pool sizes to at least one worker, and the recovered Jobtype hashes are present.
    #[test]
    fn pool_sizes_and_jobtypes_recovered() {
        let pool = WorkerPool::new();
        assert!(pool.num_workers() >= 1, "pimpGetNumCpus is >= 1");
        assert_eq!(REGISTERED_JOBTYPES.len(), 7);
        assert!(REGISTERED_JOBTYPES.contains(&0xcd4a518c)); // AnimCpu*Job candidate
        assert_eq!(JOB_ELEM_SIZE, 0x60);
    }

    /// High-priority jobs are drained strictly before low-priority ones (the §1 asymmetric policy),
    /// FIFO within each priority — regardless of submit order.
    #[test]
    fn high_drains_before_low_fifo() {
        let order = Rc::new(RefCell::new(Vec::new()));
        let mut pool = WorkerPool::with_workers(4);
        // Interleave submits: low, high, low, high.
        for (pri, id) in [
            (Priority::Low, 10),
            (Priority::High, 1),
            (Priority::Low, 11),
            (Priority::High, 2),
        ] {
            let o = order.clone();
            pool.submit(pri, Job::new(0, None, move || o.borrow_mut().push(id))).unwrap();
        }
        let ran = pool.run_all();
        assert_eq!(ran, 4);
        // Highs first (FIFO 1,2), then lows (FIFO 10,11).
        assert_eq!(*order.borrow(), vec![1, 2, 10, 11]);
        assert!(pool.is_idle());
    }

    /// One worker iteration drains ALL high-pri but only ONE low-pri (§1). Faithful, not just full-drain.
    #[test]
    fn worker_iteration_policy_is_asymmetric() {
        let order = Rc::new(RefCell::new(Vec::new()));
        let mut pool = WorkerPool::with_workers(1);
        for id in [1, 2, 3] {
            let o = order.clone();
            pool.submit(Priority::High, Job::from_fn(move || o.borrow_mut().push(id))).unwrap();
        }
        for id in [10, 11] {
            let o = order.clone();
            pool.submit(Priority::Low, Job::from_fn(move || o.borrow_mut().push(id))).unwrap();
        }
        let ran = pool.run_worker_iteration();
        assert_eq!(ran, 4, "3 high + exactly 1 low in one iteration");
        assert_eq!(*order.borrow(), vec![1, 2, 3, 10]);
        assert_eq!(pool.pending(), 1, "one low-pri job remains for the next iteration");
        pool.run_worker_iteration();
        assert_eq!(*order.borrow(), vec![1, 2, 3, 10, 11]);
    }

    /// Fork/join: N jobs sharing one fence drive it to N; a fire-and-forget job leaves its (absent)
    /// fence untouched. This is the completion-counter contract (`InterlockedExchangeAdd`, §2/§3).
    #[test]
    fn fork_join_fence_counts_completions() {
        let mut pool = WorkerPool::with_workers(4);
        let fence = CompletionFence::new();
        let sum = Rc::new(RefCell::new(0u32));
        for i in 1..=5u32 {
            let s = sum.clone();
            pool.fork(Priority::High, 0xcd4a518c, &fence, move || *s.borrow_mut() += i).unwrap();
        }
        // A fire-and-forget job (no fence) alongside the forked batch.
        pool.submit(Priority::Low, Job::from_fn(|| {})).unwrap();
        assert_eq!(fence.count(), 0, "nothing completed before draining");
        pool.run_all();
        assert_eq!(fence.count(), 5, "the join sees all 5 forked jobs complete");
        assert_eq!(*sum.borrow(), 15);
    }

    /// The pool idles cleanly when empty (no work submitted) — the data-driven "idle until fed" shape.
    #[test]
    fn empty_pool_is_idle() {
        let mut pool = WorkerPool::new();
        assert!(pool.is_idle());
        assert_eq!(pool.run_all(), 0);
    }
}
