# mercs2_jobs

A faithful reimplementation of Mercenaries 2's in-house **"Pimp"** job system (scoreboard row 15) — the per-CPU worker pool and the CS-guarded job ring that were the engine's parallel-work spine.

## What it is

A small, dependency-light library with three moving parts:

- **`WorkerPool`** — the per-CPU worker pool. Owns two priority rings (high / low) and the recovered
  worker count (`pimp_num_cpus()`, the portable stand-in for the exe's `GetProcessAffinityMask`
  popcount). Its drain policy is the engine's *asymmetric* one: the high-priority queue is drained
  **fully**, then **exactly one** low-priority entry is serviced, per worker iteration.
- **`JobRing`** — a bounded FIFO of 96-byte-stride job slots. Pushing into a full ring **rejects and
  hands the job back** (`Err(job)`) rather than dropping it, and counts the rejection.
- **`Job` + `CompletionFence`** — one work item (Jobtype hash + optional fence + bound
  `handler(arg)` closure) and the fork/join done-counter. `Job::run()` performs the engine's exact
  two-step: call the handler, then increment the fence.

The reimpl render loop is single-threaded, so the crate models the **job-graph / ordering
semantics** rather than spawning OS threads. `WorkerPool::run_all()` is a deterministic
linearization (all high FIFO, then the low queue one-per-iteration);
`WorkerPool::run_worker_iteration()` reproduces the exact per-worker drain policy. Exclusive
`&mut` access stands in for the critical section, and with one producer in flight the exe's
multi-producer reservation spin is a no-op. Where the exe's N workers would interleave
nondeterministically, the `CompletionFence` is how a submitter recovers an ordering guarantee — so
it is modeled first-class.

## Where it comes from

Pandemic's Pimp multi-CPU job library (Xbox source path `d:\mainline\mercs2\pimp\`) was the
scoreboard's "true novel system" — **100% string-only on the Xbox build** (zero decomp hits). The PC
build keeps the real architecture, and that is what was recovered:

- Code map: `docs/reverse_engineer/pimp_job_system_code_map.md` (companion to the Keystone PC
  code-map recovery: master tick order, event bus, Pimp jobs).
- Xbox oracle: `docs/mercs2-pdb-analysis/jobs-threading.md`.

Everything in the crate traces to a section of that code map:

| thing | exe origin |
|---|---|
| worker loop / drain policy | `FUN_00876400` (§1) |
| CPU count | `pimpGetNumCpus` `FUN_008767b0` (§1) |
| ring construction, 3 rings of 96-B elements | `PimpQueueInit` `FUN_0084af70`, the `"pimpQueue"` seed (§3) |
| enqueue | `FUN_004c00e0` / `FUN_0084b290` (§3) |
| job dispatch + `InterlockedExchangeAdd(fence, 1)` | §2, "Job dispatch — the hot line" |
| `REGISTERED_JOBTYPES` (7 hashes) | §2 `hash → handler`, incl. the `0x724xxx` **AnimCpu\*Job** trio |

The 360's lock-free VMX "a64" queue **degraded to a `CRITICAL_SECTION`-guarded ring** on x86; the
completion fence is the *only* Interlocked op left in the whole system.

## Usage

```rust
use mercs2_jobs::{CompletionFence, Job, Priority, WorkerPool};
use std::cell::RefCell;
use std::rc::Rc;

// One worker per usable CPU (pimpGetNumCpus), or WorkerPool::with_workers(n).
let mut pool = WorkerPool::new();

// Fork N jobs onto a shared fence under an authentic Jobtype hash (0xcd4a518c = an
// AnimCpu*Job candidate; pass 0 for an ad-hoc closure).
let fence = CompletionFence::new();
let sum = Rc::new(RefCell::new(0u32));
for i in 1..=5u32 {
    let s = sum.clone();
    pool.fork(Priority::High, 0xcd4a518c, &fence, move || *s.borrow_mut() += i).unwrap();
}

// A fire-and-forget low-pri job alongside the batch (no fence).
pool.submit(Priority::Low, Job::from_fn(|| {})).unwrap();

assert_eq!(fence.count(), 0);        // nothing has run yet
pool.run_all();                       // the fork/join point: drain both rings
assert_eq!(fence.count(), 5);        // the join sees all 5 forked jobs complete
assert_eq!(*sum.borrow(), 15);
assert!(pool.is_idle());
```

To reproduce the engine's per-worker drain policy exactly (high fully, then *one* low), call
`pool.run_worker_iteration()` in a loop instead of `run_all()`.

## Modules

| module | owns |
|---|---|
| `job` | `Job` (Jobtype hash + optional fence + bound handler closure), `CompletionFence`, `Priority`, `JOB_ELEM_SIZE` (`0x60`), `REGISTERED_JOBTYPES` (the 7 recovered hashes). |
| `ring` | `JobRing` — the bounded, CS-guarded FIFO, plus the recovered capacities `RING_CAP_INIT` (`0x10`), `RING_CAP_WORKER_ITER` (`0x1000`), `RING_CAP_WORKER_DRAIN` (`0x400`). |

The crate root owns `WorkerPool` and `pimp_num_cpus()`.

## Notes / gotchas

- **The crate is not wired into the engine.** It provides the mechanism; submitting jobs from the
  frame is the integration layer's job. In the exe the workers run *continuously, parallel to
  `RunFrame`* — the sim submits while it ticks (e.g. the streaming LOD-budget notify `FUN_0084ae70`
  fired inside the master update `FUN_004c14f0`). Single-threaded, that maps to: **submit during the
  tick → `run_all()` at the fork/join point → the fence reads back complete.**
- **The exe's handler/arg split is collapsed.** The engine keeps the handler (in a per-CPU handler
  table, keyed by Jobtype index) separate from its `+0x08` context pointer, then calls
  `handler(arg)`. In a single address space that indirection buys nothing, so handler and arg are
  bound into one `Box<dyn FnOnce()>` at enqueue time. The Jobtype hash is retained on the job for
  identity/routing.
- **The Jobtype handlers are not reimplemented.** `REGISTERED_JOBTYPES` records the hashes only —
  the handlers are engine `FUN_` bodies that belong to the animation/streaming crates. A caller can
  register its own work under the authentic hash.
- **Deliberately not built** (the code map marks these unrecovered / *confirm-live*): the worker
  `CreateThread` spawn site and per-CPU control-block alloc loop live in the SecuROM-relocated
  `pimpInit` island — the crate models the pool *shape* the spawn produces, not the relocated spawn
  code. The per-CPU RootTimer profiler node struct and the PC profiler-zone table global +
  `SyncCPUGPU` fence have their name strings stripped, so no profiler tree is invented here.
- **Backpressure, not blocking.** A push into a full ring returns `Err(job)`. The exe's in-CS
  producer would spin-wait for a slot; the caller here must decide to retry or drop.
