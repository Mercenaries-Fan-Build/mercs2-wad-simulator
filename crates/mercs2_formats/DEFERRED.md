# Deferred (mercs2_formats)

Improvements/optimizations spotted while implementing but intentionally NOT done, so the
task stays a faithful reimplementation and nothing is gold-plated. Each is tagged with whether it
blocks faithful behaviour.

## schema.rs `parse_comp_groups` allocates a `Vec<u8>` per child body

`parse_comp_groups` copies each `info`/`schm`/`data` body out of the container into an owned
`Vec<u8>` (via `CompGroup { info, schm, data }`). A borrowing variant returning `&[u8]` slices into
the container would avoid per-group allocations on the hot world-load path. Left as-is because the
copies keep the API simple and self-contained for the current callers (tests + future loader), and
correctness is unaffected.

- `[faithful-blocker: no]`
