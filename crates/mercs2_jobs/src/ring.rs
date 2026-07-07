//! The **job ring** — the CS-guarded bounded FIFO the workers drain (code map §3).
//!
//! `FUN_0084af70` (`PimpQueueInit`, the `"pimpQueue"` seed) builds **three** rings of 96-byte elements
//! via `Pool_Alloc`. The 360's VMX lock-free "a64" queue **degraded to a `CRITICAL_SECTION`-guarded
//! ring** on x86 — the only Interlocked op left in the system is the completion fence (see
//! [`crate::job::CompletionFence`]). The recovered per-ring struct (bases `DAT_00ff45e8` /
//! `DAT_00ff4618` / `DAT_00ff4650`):
//!
//! | off | field |
//! |---|---|
//! | +0x00 | `HANDLE` mutex — `CreateMutexA(…, "pimpQueue")` (created but the CS does the guarding — legacy/diagnostic, *confirm-live*) |
//! | +0x04 | `u32` elem_size = 0x60 |
//! | +0x08 | `u32` capacity |
//! | +0x0c | `void*` ring buffer |
//! | +0x10 | `u32` **head\|count** — low16 = consume idx, high16 = live count |
//! | +0x14 | `u32` **tail** — low16 = produce idx, high16 = reservation ctr |
//! | +0x18 | `CRITICAL_SECTION` (24 B) |
//!
//! **Enqueue** (`FUN_004c00e0` / `FUN_0084b290`): `EnterCS`, `slot=(head.lo+tail.lo) mod cap`, memcpy
//! (split on wrap), then an in-CS spin-commit `do{}while(published!=reserved)` that recovers **FIFO
//! ordering among concurrent producers**, advance, `LeaveCS`. **Dequeue** (the worker `FUN_00876400`):
//! `TryEnterCS` → read count (`head>>16`) → advance consume idx (wrap at cap) → decrement count →
//! `LeaveCS`.
//!
//! **What collapses in the single-threaded reimpl:** the render loop is single-threaded, so the
//! `CRITICAL_SECTION` / `TryEnterCS` and the multi-producer reservation spin (`+0x14` high16 reservation
//! ctr, the `published!=reserved` loop) have no work to do — exclusive `&mut self` *is* the critical
//! section, and there is only ever one producer in flight, so the spin is a no-op. We therefore keep
//! the **observable** contract — a bounded FIFO of 96-byte-stride job slots — and preserve the packed
//! `head`/`count`/`tail` fields so the layout is legible, while noting the guard's exe origin.

use crate::job::{Job, JOB_ELEM_SIZE};

/// `PimpQueueInit` seed-ring capacity — `0x10` (§3, first of the three rings).
pub const RING_CAP_INIT: usize = 0x10;
/// Worker per-iteration ring capacity — `0x1000` (§3). Used here for the **high-pri** queue.
pub const RING_CAP_WORKER_ITER: usize = 0x1000;
/// Worker drain-ring capacity — `0x400` (§3). Used here for the **low-pri** queue.
pub const RING_CAP_WORKER_DRAIN: usize = 0x400;

/// A bounded, critical-section-guarded FIFO ring of [`Job`]s (each a `JOB_ELEM_SIZE`-byte element in
/// the exe). Bounded exactly to its `capacity`; a push into a full ring is **rejected** (the job is
/// handed back so no work is silently lost — the exe's in-CS producer would spin-wait for a slot,
/// which single-threaded we surface as backpressure rather than blocking).
pub struct JobRing {
    cap: usize,
    /// The ring buffer (`+0x0c void*`). `Job` is not `Copy` (it owns a boxed closure), so slots are
    /// `Option<Job>` rather than a raw byte array — the FIFO semantics are identical.
    slots: Vec<Option<Job>>,
    /// `+0x10` low16 — consume index.
    head: usize,
    /// `+0x10` high16 — live count.
    count: usize,
    /// `+0x14` low16 — produce index.
    tail: usize,
    /// Pushes rejected because the ring was full (the exe would spin-wait in-CS; we count instead).
    rejected: u64,
}

impl JobRing {
    /// A ring of `capacity` 96-byte-stride slots (one of [`RING_CAP_INIT`] / [`RING_CAP_WORKER_ITER`] /
    /// [`RING_CAP_WORKER_DRAIN`]). `capacity` must be non-zero.
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "job ring capacity must be non-zero");
        let mut slots = Vec::with_capacity(capacity);
        slots.resize_with(capacity, || None);
        JobRing { cap: capacity, slots, head: 0, count: 0, tail: 0, rejected: 0 }
    }

    /// Element stride in the exe (`+0x04 elem_size` = `0x60`). Constant across all three rings.
    pub const fn elem_size(&self) -> usize {
        JOB_ELEM_SIZE
    }

    /// Enqueue at the produce index (`tail`), advancing with wraparound. Rejects (returns `Err(job)`)
    /// when the ring is full so the caller keeps its work. Mirrors `EnterCS → write slot → advance →
    /// LeaveCS`; the multi-producer reservation spin collapses single-threaded (module docs).
    pub fn push(&mut self, job: Job) -> Result<(), Job> {
        if self.count == self.cap {
            self.rejected += 1;
            return Err(job);
        }
        self.slots[self.tail] = Some(job);
        self.tail = (self.tail + 1) % self.cap;
        self.count += 1;
        Ok(())
    }

    /// Dequeue from the consume index (`head`), advancing with wraparound. Returns `None` when empty
    /// (`count == 0`). Mirrors the worker's `TryEnterCS → read count → advance consume idx → dec count`.
    pub fn pop(&mut self) -> Option<Job> {
        if self.count == 0 {
            return None;
        }
        let job = self.slots[self.head].take();
        self.head = (self.head + 1) % self.cap;
        self.count -= 1;
        job
    }

    /// Live count (`head>>16`).
    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn is_full(&self) -> bool {
        self.count == self.cap
    }

    /// Slot capacity (`+0x08`).
    pub fn capacity(&self) -> usize {
        self.cap
    }

    /// Pushes rejected because the ring was full since construction.
    pub fn rejected(&self) -> u64 {
        self.rejected
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;
    use std::cell::RefCell;

    #[test]
    fn caps_are_the_recovered_values() {
        assert_eq!(RING_CAP_INIT, 0x10);
        assert_eq!(RING_CAP_WORKER_ITER, 0x1000);
        assert_eq!(RING_CAP_WORKER_DRAIN, 0x400);
        assert_eq!(JobRing::new(4).elem_size(), 0x60);
    }

    /// Push/pop is FIFO and wraps around the ring buffer correctly.
    #[test]
    fn bounded_fifo_wraps() {
        let order = Rc::new(RefCell::new(Vec::new()));
        let mut ring = JobRing::new(2);
        // Fill, drain one, refill past the wrap boundary.
        for i in 0..2u32 {
            let o = order.clone();
            ring.push(Job::from_fn(move || o.borrow_mut().push(i))).unwrap();
        }
        assert!(ring.is_full());
        ring.pop().unwrap().run(); // runs job 0, frees a slot at head
        let o = order.clone();
        ring.push(Job::from_fn(move || o.borrow_mut().push(2))).unwrap(); // wraps to slot 0
        ring.pop().unwrap().run(); // job 1
        ring.pop().unwrap().run(); // job 2
        assert!(ring.is_empty());
        assert_eq!(*order.borrow(), vec![0, 1, 2]);
    }

    /// A full ring rejects and hands the job back (no silent loss); the reject is counted.
    #[test]
    fn full_ring_rejects_and_returns_job() {
        let mut ring = JobRing::new(1);
        ring.push(Job::from_fn(|| {})).unwrap();
        let back = ring.push(Job::from_fn(|| {}));
        assert!(back.is_err(), "push into full ring must be rejected");
        assert_eq!(ring.rejected(), 1);
        drop(back); // the returned job is ours to keep/retry
    }
}
