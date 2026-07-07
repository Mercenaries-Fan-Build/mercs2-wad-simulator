# mercs2_formats — deferred improvements

Non-blocking follow-ups spotted while implementing but intentionally NOT done, so the crate stays a
faithful reimplementation and nothing is gold-plated. Anything that would change on-disk fidelity is
flagged `[faithful-blocker: yes]`; convenience/cleanup/perf is `[faithful-blocker: no]`.

## Save (`save.rs` / `save_write.rs`)

- **From-scratch profile assembly (no template).** `save_write::write_profile`
  currently re-stamps fields over an existing parsed `Profile`'s raw buffer, so it
  needs a real `.profile` as a template to supply the still-unexplained *constant*
  bytes (byte `@0xAC`, the two `@0x462..0x467` pre-zlib u16s, and the padding after
  the deflate stream). A `Profile::new_blank()` that builds all 13,404 bytes with no
  template would let the engine mint a brand-new save from pure `SaveState`. Not a
  blocker for round-tripping or editing existing saves. `[faithful-blocker: no]`
- **`@0x462..0x467` (2×u16) + byte `@0xAC` meaning.** Preserved verbatim by the
  writer today (byte-exact), so no fidelity loss — but their purpose is unread.
  Recover via a live save capture (break `saveProfile 0x7BC628`). `[faithful-blocker: no]`
- **Deflate encoder parity.** `set_lua_payload` uses flate2's default zlib encoder,
  which produces a *valid* stream the engine inflates correctly but whose bytes need
  not match the engine's own deflate output. The `ProfileHash` covers the compressed
  bytes, so each write is self-consistent; only bit-identical re-compression of an
  *edited* payload would require matching the engine's exact zlib parameters. Byte-exact
  round-trip of an *unmodified* save is unaffected (it reuses the original stream).
  `[faithful-blocker: no]`
- **`SaveState` → Lua-source serializer.** The read side decodes the inflated
  `return { … }` into a structured `SaveState`; the inverse (re-emit `SaveState` as
  Lua text for `set_lua_payload`) is not yet implemented. Needed only to *synthesize*
  save content in-engine; editing header fields of an existing save does not need it.
  `[faithful-blocker: no]`

## Read (`save.rs`)

- **Undecoded Lua sub-tables.** Economy/faction/support catalogs,
  `_tRequirementsObtained`, `tLockedGates`, per-vehicle unlock tables are present in
  the inflated Lua but not decoded into `SaveState`. `[faithful-blocker: no]`

## Schema (`schema.rs`)

- **`parse_comp_groups` allocates a `Vec<u8>` per child body.** It copies each
  `info`/`schm`/`data` body out of the container into an owned `Vec<u8>` (via
  `CompGroup { info, schm, data }`). A borrowing variant returning `&[u8]` slices into
  the container would avoid per-group allocations on the hot world-load path. Left as-is
  because the copies keep the API simple and self-contained for the current callers
  (tests + future loader), and correctness is unaffected. `[faithful-blocker: no]`
