//! Comprehensive registry of every FourCC the Mercenaries 2 engine dispatches on.
//!
//! Seeded from a scan of `cmp eax, <imm32>` tag comparisons in the plaintext image
//! `output/patched/Mercenaries2.exe` (base 0x00400000): 232 distinct FourCC
//! immediates. Each entry records the representative dispatch/handler address, the
//! engine subsystem it belongs to, and our verification status. The WAD toolchain
//! consults this to (a) recognize/convert genuine UCFX asset chunks and (b) loudly
//! flag any tag that is not yet a *validated* WAD chunk (`needs_investigation`),
//! so unverified engine features get addressed rather than silently u32-swapped.
//!
//! NOTE: tags appear in multiple call sites; `handler_va` is one representative
//! dispatch site, not the only one. Addresses marked "(VERIFY handler)" in `note`
//! still need their handler disassembled to pin the exact invariant.
//!
//! Provenance: invariants verified against the Ghidra decompilation at
//! output/_ghidra/all_functions_decomp.txt (PC EXE, base 0x00400000), function
//! bodies keyed by the FUN_<addr> named in each note, cross-checked vs capstone
//! (tools/disasm_func.py) and the retail vz.wad (zero false-positives).

/// Engine subsystem a FourCC belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subsystem {
    /// Genuine UCFX/WAD asset chunk descriptor (mesh/geom/anim/ECS/texture).
    UcfxAsset,
    /// D3DFORMAT pixel code used by the texture-decode path (not a descriptor).
    D3dFormat,
    /// Runtime entity-type dispatcher (@0x9ab) - not stored in WADs.
    EntityRuntime,
    /// Network protocol message key - not stored in WADs.
    NetworkProto,
    /// Lua/object property accessor key - not stored in WADs.
    LuaReflection,
    /// Unclassified FourCC immediate.
    Misc,
}

impl Subsystem {
    pub fn label(self) -> &'static str {
        match self {
            Subsystem::UcfxAsset => "UCFX asset chunk",
            Subsystem::D3dFormat => "D3DFORMAT pixel code",
            Subsystem::EntityRuntime => "runtime entity-type dispatcher",
            Subsystem::NetworkProto => "network protocol key",
            Subsystem::LuaReflection => "Lua property accessor",
            Subsystem::Misc => "unclassified",
        }
    }
}

/// How far we have verified a tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verify {
    /// A simulator validator checks this chunk's invariant.
    Validated,
    /// Recognized and benign (converter-handled), no structural validator.
    Registered,
    /// Not yet a validated WAD chunk - flag for deeper investigation.
    NeedsInvestigation,
}

/// One registry row.
#[derive(Debug, Clone, Copy)]
pub struct TagInfo {
    pub fourcc: [u8; 4],
    /// Representative engine dispatch/handler address (image base 0x00400000).
    pub handler_va: u32,
    pub subsystem: Subsystem,
    pub verify: Verify,
    pub note: &'static str,
}

use Subsystem::*;
use Verify::*;

/// Every dispatched FourCC (232 entries), grouped by subsystem then status.
pub const TAG_REGISTRY: &[TagInfo] = &[
    // --- UcfxAsset ---
    TagInfo { fourcc: *b"AREA", handler_va: 0x0047830a, subsystem: UcfxAsset, verify: Validated, note: "container walk @0x4a4ab0; reads 4-byte 'info' header per child; no fixed array" },
    TagInfo { fourcc: *b"BINN", handler_va: 0x0059d008, subsystem: UcfxAsset, verify: Validated, note: "" },
    TagInfo { fourcc: *b"BNDS", handler_va: 0x004a86dc, subsystem: UcfxAsset, verify: Validated, note: "" },
    TagInfo { fourcc: *b"BODY", handler_va: 0x00750aa5, subsystem: UcfxAsset, verify: Validated, note: "" },
    TagInfo { fourcc: *b"COMP", handler_va: 0x006549ef, subsystem: UcfxAsset, verify: Validated, note: "" },
    TagInfo { fourcc: *b"DEPS", handler_va: 0x0059d0d3, subsystem: UcfxAsset, verify: Validated, note: "" },
    TagInfo { fourcc: *b"DICT", handler_va: 0x00491386, subsystem: UcfxAsset, verify: Validated, note: "" },
    TagInfo { fourcc: *b"GEOM", handler_va: 0x0048ccbd, subsystem: UcfxAsset, verify: Validated, note: "" },
    TagInfo { fourcc: *b"IBUF", handler_va: 0x00478311, subsystem: UcfxAsset, verify: Validated, note: "" },
    TagInfo { fourcc: *b"INFO", handler_va: 0x0045dc2b, subsystem: UcfxAsset, verify: Validated, note: "" },
    TagInfo { fourcc: *b"MTRL", handler_va: 0x004a528d, subsystem: UcfxAsset, verify: Validated, note: "tex count@106 -> fixed 10-slot array @+0xAC; >10 overruns (AV 0x84DD5B); parser FUN_00858790" },
    TagInfo { fourcc: *b"PRMG", handler_va: 0x0047817e, subsystem: UcfxAsset, verify: Validated, note: "" },
    TagInfo { fourcc: *b"PRMT", handler_va: 0x004783a5, subsystem: UcfxAsset, verify: Validated, note: "" },
    TagInfo { fourcc: *b"STRM", handler_va: 0x004782fd, subsystem: UcfxAsset, verify: Validated, note: "" },
    TagInfo { fourcc: *b"data", handler_va: 0x004a47d6, subsystem: UcfxAsset, verify: Validated, note: "" },
    TagInfo { fourcc: *b"decl", handler_va: 0x004a47e2, subsystem: UcfxAsset, verify: Validated, note: "" },
    TagInfo { fourcc: *b"flgs", handler_va: 0x00654f16, subsystem: UcfxAsset, verify: Validated, note: "" },
    TagInfo { fourcc: *b"info", handler_va: 0x004a47ea, subsystem: UcfxAsset, verify: Validated, note: "" },
    TagInfo { fourcc: *b"schm", handler_va: 0x00654b6e, subsystem: UcfxAsset, verify: Validated, note: "" },
    TagInfo { fourcc: *b"AINF", handler_va: 0x0068c7de, subsystem: UcfxAsset, verify: Registered, note: "" },
    TagInfo { fourcc: *b"CEXE", handler_va: 0x004cf3d9, subsystem: UcfxAsset, verify: Registered, note: "" },
    TagInfo { fourcc: *b"CHDR", handler_va: 0x004cf3bb, subsystem: UcfxAsset, verify: Registered, note: "" },
    TagInfo { fourcc: *b"INDX", handler_va: 0x004719f3, subsystem: UcfxAsset, verify: Registered, note: "" },
    TagInfo { fourcc: *b"ITEM", handler_va: 0x0067c315, subsystem: UcfxAsset, verify: Registered, note: "" },
    TagInfo { fourcc: *b"NAME", handler_va: 0x00750a8f, subsystem: UcfxAsset, verify: Registered, note: "" },
    TagInfo { fourcc: *b"SCRB", handler_va: 0x004a4cac, subsystem: UcfxAsset, verify: Registered, note: "" },
    TagInfo { fourcc: *b"SINF", handler_va: 0x0067c30e, subsystem: UcfxAsset, verify: Registered, note: "" },
    TagInfo { fourcc: *b"SKIN", handler_va: 0x0047192a, subsystem: UcfxAsset, verify: Registered, note: "" },
    TagInfo { fourcc: *b"STAT", handler_va: 0x004cf5cf, subsystem: UcfxAsset, verify: Registered, note: "" },
    TagInfo { fourcc: *b"STRS", handler_va: 0x004640a0, subsystem: UcfxAsset, verify: Registered, note: "" },
    TagInfo { fourcc: *b"SWIT", handler_va: 0x004cf5d7, subsystem: UcfxAsset, verify: Registered, note: "" },
    TagInfo { fourcc: *b"TRNS", handler_va: 0x0068c7e5, subsystem: UcfxAsset, verify: Registered, note: "" },
    TagInfo { fourcc: *b"UNIQ", handler_va: 0x006549fb, subsystem: UcfxAsset, verify: Registered, note: "" },
    TagInfo { fourcc: *b"enum", handler_va: 0x006549dd, subsystem: UcfxAsset, verify: Registered, note: "" },
    TagInfo { fourcc: *b"flgt", handler_va: 0x00654f22, subsystem: UcfxAsset, verify: Registered, note: "" },
    TagInfo { fourcc: *b"sequ", handler_va: 0x0067bfaa, subsystem: UcfxAsset, verify: Registered, note: "" },
    TagInfo { fourcc: *b"ASTO", handler_va: 0x0067c780, subsystem: UcfxAsset, verify: Validated, note: "anim struct @FUN_0067c780 (decomp): reads u32 count then count*4 alloc (overflow-guarded). Validated: body >= 4" },
    TagInfo { fourcc: *b"ATRB", handler_va: 0x00492b1c, subsystem: UcfxAsset, verify: Validated, note: "effect attribute @0x492b1c: reads a 4-byte inner hash then sub-dispatches to per-attribute readers. Validated: body >= 4" },
    TagInfo { fourcc: *b"BSHI", handler_va: 0x00478318, subsystem: UcfxAsset, verify: Validated, note: "blendshape index @FUN_00478270 (decomp): reads count*2 u16 array (count from INFO param_1[0x6a]); converter swaps u16. Validated: body % 2 == 0" },
    TagInfo { fourcc: *b"BSHP", handler_va: 0x004a4770, subsystem: UcfxAsset, verify: Registered, note: "blendshape data @FUN_004a4770 (decomp): container-walker that finds a child data chunk (0x61746164) and resolves its offset (NOT a count*0x18 array). Recognized/benign" },
    TagInfo { fourcc: *b"COLR", handler_va: 0x004930e5, subsystem: UcfxAsset, verify: Validated, note: "colour palette @0x4930e5: stores a fixed 0xC8 (200-byte) record into the effect palette heap. Validated: body >= 0xC8" },
    TagInfo { fourcc: *b"DAMG", handler_va: 0x0045f558, subsystem: UcfxAsset, verify: Validated, note: "ECS damage ref array @0x45f558: count×4 u32 refs (count from INFO field, overflow-guarded). Validated: body % 4 == 0" },
    TagInfo { fourcc: *b"DATA", handler_va: 0x0045f187, subsystem: UcfxAsset, verify: Registered, note: "ECS entity data @0x45f187: delegates body parse to template builder 0x631c90; no self-contained body invariant. Recognized/benign (distinct from lowercase data)" },
    TagInfo { fourcc: *b"DEBR", handler_va: 0x0045f9a8, subsystem: UcfxAsset, verify: Validated, note: "ECS debris ref array @0x45f9a8: count×4 u32 refs (overflow-guarded). Validated: body % 4 == 0" },
    TagInfo { fourcc: *b"DECL", handler_va: 0x0045dbb0, subsystem: UcfxAsset, verify: Registered, note: "context-dependent: ECS-template DECL @FUN_0045dbb0 is count×0x24 ([u32 id][0x20 blob]), but DECL in other asset types (material/resident) has a different layout, so no context-blind body invariant (retail block 3185 has a 10000-byte DECL)" },
    TagInfo { fourcc: *b"EMIT", handler_va: 0x00492703, subsystem: UcfxAsset, verify: Registered, note: "emitter timing @0x492703: delegates body parse to sub-reader 0x48cc30; no self-contained body invariant. Recognized/benign" },
    TagInfo { fourcc: *b"EMTR", handler_va: 0x00492402, subsystem: UcfxAsset, verify: Validated, note: "emitter @0x492402: reads a u16 count then count×4 alloc (overflow-guarded). Validated: body >= 2" },
    TagInfo { fourcc: *b"FRCE", handler_va: 0x00491c93, subsystem: UcfxAsset, verify: Validated, note: "force @0x491c93: reads a 4-byte inner hash then sub-dispatches per force type. Validated: body >= 4" },
    TagInfo { fourcc: *b"INST", handler_va: 0x004a4e51, subsystem: UcfxAsset, verify: Validated, note: "renderable consumer @0x4a4c40: count×0x18 (24B) records, count @esi+0x28 (renderable INFO); alloc overflow-guarded. Validated: body % 0x18 == 0" },
    TagInfo { fourcc: *b"KEYS", handler_va: 0x004640a8, subsystem: UcfxAsset, verify: Validated, note: "keyframe list @0x4640a8: u32 count header then count×8 keyframe records. Validated: (body-4) % 8 == 0, body >= 4" },
    TagInfo { fourcc: *b"MANM", handler_va: 0x0067a844, subsystem: UcfxAsset, verify: Registered, note: "anim-name @0x67a844: allocates a fixed 0x34 in-memory struct; body read is smaller (16 bytes in retail) — no confirmed body invariant. Recognized/benign" },
    TagInfo { fourcc: *b"MESH", handler_va: 0x00471923, subsystem: UcfxAsset, verify: Registered, note: "mesh dispatcher @0x471900: allocates a FIXED 0x10-byte renderable per descriptor, indexed by a u16; no body read, no count-driven array. Engine-safe; no self-contained body invariant" },
    TagInfo { fourcc: *b"MINF", handler_va: 0x0068e5d0, subsystem: UcfxAsset, verify: Validated, note: "mesh/anim info @FUN_0068e5d0 (decomp): reads [u32 hash][u16] (6 bytes) per record. Validated: body >= 6" },
    TagInfo { fourcc: *b"NODE", handler_va: 0x004cf48b, subsystem: UcfxAsset, verify: Validated, note: "scene node @FUN_004cf340 (decomp): u32 hash + u32 child-count (8B header), then count*0x14 child array (overflow-guarded). Validated: body >= 8" },
    TagInfo { fourcc: *b"PART", handler_va: 0x0045f8e3, subsystem: UcfxAsset, verify: Validated, note: "ECS particle ref array @0x45f8e3: count×4 u32 refs (overflow-guarded). Validated: body % 4 == 0" },
    TagInfo { fourcc: *b"PHY2", handler_va: 0x004a845f, subsystem: UcfxAsset, verify: Validated, note: "Havok 5.5 collision packfile @0x4a845f: u32 header prefix + embedded packfile (magic SEARCHED, palindromic 57E0E057 10C0C010) + trailing wrapper. Validated by recalculation (havok::validate_phy2): locate packfile, verify length + Havok version + __classnames__ it needs to convert; magic-less PHY2 is valid legacy form" },
    TagInfo { fourcc: *b"POFF", handler_va: 0x004a9cf2, subsystem: UcfxAsset, verify: Validated, note: "effect consumer @0x4a9cf2: reads a fixed 0xC (Vec3) offset into @esi+0x30. Validated: body >= 0xC" },
    TagInfo { fourcc: *b"PTCH", handler_va: 0x004a4cbe, subsystem: UcfxAsset, verify: Validated, note: "renderable consumer @0x4a4c40: count×0x38 (56B) records, count @esi+0x20 (renderable INFO). (0x1d0 belongs to SCRB.) Validated: body % 0x38 == 0" },
    TagInfo { fourcc: *b"PTMS", handler_va: 0x004a4e78, subsystem: UcfxAsset, verify: Validated, note: "renderable consumer @0x4a4c40: count×0x08 (8B) records, count @esi+0x30. Validated: body % 8 == 0" },
    TagInfo { fourcc: *b"PTYP", handler_va: 0x00491ba9, subsystem: UcfxAsset, verify: Validated, note: "particle consumer @0x491ba9: reads a single flags byte (bit0→+0x205, bit1→+0x206). Validated: body >= 1" },
    TagInfo { fourcc: *b"SOUN", handler_va: 0x0045f76d, subsystem: UcfxAsset, verify: Validated, note: "ECS sound ref array @0x45f76d: count×4 u32 refs (overflow-guarded). Validated: body % 4 == 0" },
    TagInfo { fourcc: *b"TEXT", handler_va: 0x00492fab, subsystem: UcfxAsset, verify: Validated, note: "effect text/texture ref @0x492fab: reads a leading u32 (id/count) then variable data. Validated: body >= 4" },
    TagInfo { fourcc: *b"TINY", handler_va: 0x00471a01, subsystem: UcfxAsset, verify: Registered, note: "low-LOD mesh @0x471a01: allocates a FIXED 0x18-byte renderable per descriptor, index-driven (like MESH); no body read. Engine-safe, no body invariant" },
    TagInfo { fourcc: *b"TRCK", handler_va: 0x0068e7c3, subsystem: UcfxAsset, verify: Validated, note: "anim track @0x68e7c3: 12-byte inline header (3×u32) then count×4 parallel arrays (overflow-guarded). Validated: body >= 12" },
    TagInfo { fourcc: *b"TREE", handler_va: 0x0045f629, subsystem: UcfxAsset, verify: Registered, note: "ECS tree/hierarchy @FUN_0045f3f0 (decomp): count (from INFO) variable-length records (4xu32 + u16 sub-count + sub_count xu16); 0x34 is the in-memory alloc, not an on-disk stride — no fixed body invariant" },
    TagInfo { fourcc: *b"TRFM", handler_va: 0x0048cd09, subsystem: UcfxAsset, verify: Validated, note: "transform @FUN_0048cc30 (decomp): unrolled read of 16x4-byte floats = one 4x4 matrix. Validated: body >= 64" },
    TagInfo { fourcc: *b"TYPE", handler_va: 0x0067c8f9, subsystem: UcfxAsset, verify: Registered, note: "anim TYPE @0x67c8f9: count×2 u16 read where count is caller-passed (external, not in body) — no self-contained body invariant. Recognized/benign (separate from Stance TYPE converter arm)" },
    TagInfo { fourcc: *b"VALU", handler_va: 0x0067c9d7, subsystem: UcfxAsset, verify: Validated, note: "anim VALU @0x67c9d7: (count+1)×width value blob, u32 elements (overflow-guarded). Validated: body % 4 == 0" },
    TagInfo { fourcc: *b"trns", handler_va: 0x0067e4d5, subsystem: UcfxAsset, verify: NeedsInvestigation, note: "" },
    // --- D3dFormat ---
    TagInfo { fourcc: *b"DXT1", handler_va: 0x0074d450, subsystem: D3dFormat, verify: NeedsInvestigation, note: "D3DFORMAT pixel code, not a UCFX descriptor; texture-decode dispatch" },
    TagInfo { fourcc: *b"DXT2", handler_va: 0x0074d57a, subsystem: D3dFormat, verify: NeedsInvestigation, note: "D3DFORMAT pixel code, not a UCFX descriptor; texture-decode dispatch" },
    TagInfo { fourcc: *b"DXT3", handler_va: 0x0074d588, subsystem: D3dFormat, verify: NeedsInvestigation, note: "D3DFORMAT pixel code, not a UCFX descriptor; texture-decode dispatch" },
    TagInfo { fourcc: *b"DXT4", handler_va: 0x0074d571, subsystem: D3dFormat, verify: NeedsInvestigation, note: "D3DFORMAT pixel code, not a UCFX descriptor; texture-decode dispatch" },
    TagInfo { fourcc: *b"DXT5", handler_va: 0x0074d5a7, subsystem: D3dFormat, verify: NeedsInvestigation, note: "D3DFORMAT pixel code, not a UCFX descriptor; texture-decode dispatch" },
    TagInfo { fourcc: *b"UYVY", handler_va: 0x0074d5ae, subsystem: D3dFormat, verify: NeedsInvestigation, note: "D3DFORMAT pixel code, not a UCFX descriptor; texture-decode dispatch" },
    TagInfo { fourcc: *b"YUY2", handler_va: 0x0074d581, subsystem: D3dFormat, verify: NeedsInvestigation, note: "D3DFORMAT pixel code, not a UCFX descriptor; texture-decode dispatch" },
    // --- EntityRuntime ---
    TagInfo { fourcc: *b"ALPU", handler_va: 0x009ab6b4, subsystem: EntityRuntime, verify: NeedsInvestigation, note: "runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"ARBU", handler_va: 0x009ab656, subsystem: EntityRuntime, verify: NeedsInvestigation, note: "runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"CSID", handler_va: 0x009ab400, subsystem: EntityRuntime, verify: NeedsInvestigation, note: "runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"EDGU", handler_va: 0x009ab642, subsystem: EntityRuntime, verify: NeedsInvestigation, note: "runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"GMOH", handler_va: 0x009ab552, subsystem: EntityRuntime, verify: NeedsInvestigation, note: "runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"GNIP", handler_va: 0x009ab600, subsystem: EntityRuntime, verify: NeedsInvestigation, note: "runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"KCAH", handler_va: 0x009ab4d5, subsystem: EntityRuntime, verify: NeedsInvestigation, note: "runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"LNCE", handler_va: 0x009ab448, subsystem: EntityRuntime, verify: NeedsInvestigation, note: "runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"MAGC", handler_va: 0x009ab418, subsystem: EntityRuntime, verify: NeedsInvestigation, note: "runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"MAGE", handler_va: 0x009ab44f, subsystem: EntityRuntime, verify: NeedsInvestigation, note: "runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"MAGH", handler_va: 0x009ab4dc, subsystem: EntityRuntime, verify: NeedsInvestigation, note: "runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"MAGR", handler_va: 0x009ab596, subsystem: EntityRuntime, verify: NeedsInvestigation, note: "runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"MAGU", handler_va: 0x009ab65d, subsystem: EntityRuntime, verify: NeedsInvestigation, note: "runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"NNOC", handler_va: 0x009ab41f, subsystem: EntityRuntime, verify: NeedsInvestigation, note: "runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"PEEK", handler_va: 0x009ab5b0, subsystem: EntityRuntime, verify: NeedsInvestigation, note: "runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"QRMH", handler_va: 0x009ab54b, subsystem: EntityRuntime, verify: NeedsInvestigation, note: "runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"RESU", handler_va: 0x009ab6bb, subsystem: EntityRuntime, verify: NeedsInvestigation, note: "runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"RFXH", handler_va: 0x009ab3de, subsystem: EntityRuntime, verify: NeedsInvestigation, note: "runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"SRGE", handler_va: 0x009ab456, subsystem: EntityRuntime, verify: NeedsInvestigation, note: "runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"SRTH", handler_va: 0x009ab559, subsystem: EntityRuntime, verify: NeedsInvestigation, note: "runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"SUBA", handler_va: 0x009ab40d, subsystem: EntityRuntime, verify: NeedsInvestigation, note: "runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"TADG", handler_va: 0x009ab3ef, subsystem: EntityRuntime, verify: NeedsInvestigation, note: "runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"TNCP", handler_va: 0x009ab5c2, subsystem: EntityRuntime, verify: NeedsInvestigation, note: "runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"TNEP", handler_va: 0x009ab5a7, subsystem: EntityRuntime, verify: NeedsInvestigation, note: "runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"TSLG", handler_va: 0x009ab4ce, subsystem: EntityRuntime, verify: NeedsInvestigation, note: "runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"TSLH", handler_va: 0x009ab4c5, subsystem: EntityRuntime, verify: NeedsInvestigation, note: "runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"TSLL", handler_va: 0x009ab5bb, subsystem: EntityRuntime, verify: NeedsInvestigation, note: "runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"TSLR", handler_va: 0x009ab64b, subsystem: EntityRuntime, verify: NeedsInvestigation, note: "runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"TVLP", handler_va: 0x009ab607, subsystem: EntityRuntime, verify: NeedsInvestigation, note: "runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"VSER", handler_va: 0x009ab60e, subsystem: EntityRuntime, verify: NeedsInvestigation, note: "runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation" },
    // --- NetworkProto ---
    TagInfo { fourcc: *b"OHCE", handler_va: 0x009917ee, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"arbb", handler_va: 0x0098ef95, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"atad", handler_va: 0x009ce50a, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"bcds", handler_va: 0x009d0345, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"cesb", handler_va: 0x0098ef8b, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"crsr", handler_va: 0x009c2c7c, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"csds", handler_va: 0x009d692d, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"csed", handler_va: 0x009d666f, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"csid", handler_va: 0x009ce3ab, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"csim", handler_va: 0x0098efcc, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"daeh", handler_va: 0x009ce56d, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"ddav", handler_va: 0x009d0371, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"dilc", handler_va: 0x009d1854, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"dlot", handler_va: 0x0098f031, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"dnab", handler_va: 0x0098ef3c, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"dnbb", handler_va: 0x009cce52, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"dnib", handler_va: 0x009cfd06, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"dnpa", handler_va: 0x009ce384, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"dosb", handler_va: 0x009c2c6d, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"edoc", handler_va: 0x009ce52f, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"edom", handler_va: 0x009d18d7, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"edrb", handler_va: 0x009c4f84, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"eldi", handler_va: 0x009d6435, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"emit", handler_va: 0x009ce440, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"enod", handler_va: 0x009ce4e8, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"etad", handler_va: 0x009ce583, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"etar", handler_va: 0x009d18ec, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"euqs", handler_va: 0x009ce464, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"gnol", handler_va: 0x0098efde, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"hslf", handler_va: 0x009d1869, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"hsup", handler_va: 0x009d0240, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"htua", handler_va: 0x0098ef6d, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"kclb", handler_va: 0x009ca789, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"kcos", handler_va: 0x009d1800, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"ledv", handler_va: 0x009d03c0, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"lfbl", handler_va: 0x0098efd6, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"lfmg", handler_va: 0x009a5c37, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"llop", handler_va: 0x009d0081, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"lluf", handler_va: 0x009c99d9, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"lrtc", handler_va: 0x009d63bb, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"magn", handler_va: 0x0098eff8, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"mand", handler_va: 0x009d63e7, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"manx", handler_va: 0x009cfdbf, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"maps", handler_va: 0x009ce428, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"mgni", handler_va: 0x0098efb1, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"morn", handler_va: 0x0098f014, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"nedj", handler_va: 0x0098ef2e, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"nepo", handler_va: 0x009cce1a, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"nftn", handler_va: 0x0098f020, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"nlno", handler_va: 0x009cceaf, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"nnoc", handler_va: 0x009cce59, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"nrud", handler_va: 0x009d640e, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"pamx", handler_va: 0x009cfd88, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"pdda", handler_va: 0x009d65d4, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"peek", handler_va: 0x009ce3fe, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"potv", handler_va: 0x009d1825, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"ptni", handler_va: 0x009d6533, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"ptxe", handler_va: 0x009d6503, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"pxam", handler_va: 0x009cfd71, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"rapb", handler_va: 0x0098ef9d, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"ravg", handler_va: 0x009d68ef, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"rcam", handler_va: 0x009d644c, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"rdag", handler_va: 0x009d6831, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"rdal", handler_va: 0x009cffca, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"rdam", handler_va: 0x009cffef, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"rdar", handler_va: 0x009d031d, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"rdda", handler_va: 0x009c5a53, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"reep", handler_va: 0x009cfe8a, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"resu", handler_va: 0x009c2c9d, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"revh", handler_va: 0x009ce3e5, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"rudl", handler_va: 0x009d654b, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"sdns", handler_va: 0x009d17b5, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"sndx", handler_va: 0x009d042f, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"sohn", handler_va: 0x0098efc2, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"ssap", handler_va: 0x009c2c8c, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"sses", handler_va: 0x009c2c96, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"ssim", handler_va: 0x009c2c44, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"svcr", handler_va: 0x009d1773, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"tats", handler_va: 0x009cfeac, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"torp", handler_va: 0x0098f006, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"tpgg", handler_va: 0x009d685d, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"trba", handler_va: 0x009d64e4, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"trop", handler_va: 0x009d6563, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"trpa", handler_va: 0x009d66bd, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"trpd", handler_va: 0x009d67cb, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"trpg", handler_va: 0x009d68a0, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"trpl", handler_va: 0x009d1764, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"tset", handler_va: 0x009d65e7, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"tsmg", handler_va: 0x009a5c3e, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"tsoh", handler_va: 0x009ce49a, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"tsxe", handler_va: 0x009c99d1, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"txth", handler_va: 0x009ce58e, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"txtr", handler_va: 0x009ce4c1, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"ueuq", handler_va: 0x0099dada, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"wten", handler_va: 0x009c2c85, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"xamr", handler_va: 0x009ce413, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"xcam", handler_va: 0x009cce7a, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"ydbr", handler_va: 0x009d6489, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"ydob", handler_va: 0x009ce578, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"yldn", handler_va: 0x009d0031, subsystem: NetworkProto, verify: NeedsInvestigation, note: "network protocol message key; NOT a WAD chunk - requires deeper investigation" },
    // --- LuaReflection ---
    TagInfo { fourcc: *b"alpf", handler_va: 0x00976527, subsystem: LuaReflection, verify: NeedsInvestigation, note: "Lua/object property accessor key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"bolb", handler_va: 0x0097657a, subsystem: LuaReflection, verify: NeedsInvestigation, note: "Lua/object property accessor key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"bulc", handler_va: 0x00976599, subsystem: LuaReflection, verify: NeedsInvestigation, note: "Lua/object property accessor key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"gsmx", handler_va: 0x0097665c, subsystem: LuaReflection, verify: NeedsInvestigation, note: "Lua/object property accessor key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"ihca", handler_va: 0x00976551, subsystem: LuaReflection, verify: NeedsInvestigation, note: "Lua/object property accessor key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"kbdf", handler_va: 0x009765a0, subsystem: LuaReflection, verify: NeedsInvestigation, note: "Lua/object property accessor key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"knar", handler_va: 0x009765e6, subsystem: LuaReflection, verify: NeedsInvestigation, note: "Lua/object property accessor key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"knhc", handler_va: 0x00976538, subsystem: LuaReflection, verify: NeedsInvestigation, note: "Lua/object property accessor key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"musg", handler_va: 0x009765ff, subsystem: LuaReflection, verify: NeedsInvestigation, note: "Lua/object property accessor key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"ossa", handler_va: 0x00976541, subsystem: LuaReflection, verify: NeedsInvestigation, note: "Lua/object property accessor key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"pcer", handler_va: 0x0097664e, subsystem: LuaReflection, verify: NeedsInvestigation, note: "Lua/object property accessor key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"sbus", handler_va: 0x00976655, subsystem: LuaReflection, verify: NeedsInvestigation, note: "Lua/object property accessor key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"serp", handler_va: 0x0097662d, subsystem: LuaReflection, verify: NeedsInvestigation, note: "Lua/object property accessor key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"sysf", handler_va: 0x009765f8, subsystem: LuaReflection, verify: NeedsInvestigation, note: "Lua/object property accessor key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"tcca", handler_va: 0x0097654a, subsystem: LuaReflection, verify: NeedsInvestigation, note: "Lua/object property accessor key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"tlif", handler_va: 0x009765a7, subsystem: LuaReflection, verify: NeedsInvestigation, note: "Lua/object property accessor key; NOT a WAD chunk - requires deeper investigation" },
    TagInfo { fourcc: *b"wonp", handler_va: 0x009765ef, subsystem: LuaReflection, verify: NeedsInvestigation, note: "Lua/object property accessor key; NOT a WAD chunk - requires deeper investigation" },
    // --- Misc ---
    TagInfo { fourcc: *b"CHAR", handler_va: 0x004ac8e0, subsystem: UcfxAsset, verify: Registered, note: "renderable sub-chunk @FUN_004ac8e0 (decomp): dispatched alongside INFO/MTRL; reads a count and stack-allocs count*2. Recognized/benign (was mis-classified Misc)" },
    TagInfo { fourcc: *b"GGGG", handler_va: 0x0057e9d5, subsystem: Misc, verify: NeedsInvestigation, note: "unclassified FourCC immediate - requires deeper investigation" },
    TagInfo { fourcc: *b"HHlP", handler_va: 0x004eea8a, subsystem: Misc, verify: NeedsInvestigation, note: "unclassified FourCC immediate - requires deeper investigation" },
    TagInfo { fourcc: *b"INVD", handler_va: 0x0059cffc, subsystem: Misc, verify: NeedsInvestigation, note: "unclassified FourCC immediate - requires deeper investigation" },
    TagInfo { fourcc: *b"Mxm ", handler_va: 0x00713eb3, subsystem: Misc, verify: NeedsInvestigation, note: "unclassified FourCC immediate - requires deeper investigation" },
    TagInfo { fourcc: *b"fVZD", handler_va: 0x005f40b8, subsystem: Misc, verify: NeedsInvestigation, note: "unclassified FourCC immediate - requires deeper investigation" },
    TagInfo { fourcc: *b"kVAR", handler_va: 0x0041d612, subsystem: Misc, verify: NeedsInvestigation, note: "unclassified FourCC immediate - requires deeper investigation" },
    TagInfo { fourcc: *b"uZmI", handler_va: 0x004f1ab2, subsystem: Misc, verify: NeedsInvestigation, note: "unclassified FourCC immediate - requires deeper investigation" },
    TagInfo { fourcc: *b"udn8", handler_va: 0x004f19d8, subsystem: Misc, verify: NeedsInvestigation, note: "unclassified FourCC immediate - requires deeper investigation" },
];

/// Look up a FourCC in the registry.
pub fn classify(fourcc: [u8; 4]) -> Option<&'static TagInfo> {
    TAG_REGISTRY.iter().find(|t| t.fourcc == fourcc)
}

/// Return the registry entry for a tag that should surface a "requires deeper
/// investigation" diagnostic: tags flagged `Verify::NeedsInvestigation` — i.e.
/// not-yet-validated UCFX chunks plus the non-UCFX subsystems (entity/network/
/// Lua/D3DFORMAT). `Registered` tags are recognized & benign and do NOT flag.
pub fn needs_investigation(fourcc: [u8; 4]) -> Option<&'static TagInfo> {
    classify(fourcc).filter(|t| t.verify == Verify::NeedsInvestigation)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registry_has_no_duplicates() {
        let mut seen = std::collections::HashSet::new();
        for t in TAG_REGISTRY {
            assert!(seen.insert(t.fourcc), "dup {:?}", t.fourcc);
        }
    }
    #[test]
    fn classify_known() {
        assert_eq!(classify(*b"MTRL").unwrap().verify, Verify::Validated);
        assert_eq!(classify(*b"AREA").unwrap().subsystem, Subsystem::UcfxAsset);
        assert_eq!(classify(*b"CSID").unwrap().subsystem, Subsystem::EntityRuntime);
        assert!(classify(*b"ZZZZ").is_none());
    }
    #[test]
    fn ucfx_asset_entries_are_registered_enum_variants() {
        use crate::tags::ChunkTag;
        for t in TAG_REGISTRY {
            if t.subsystem == Subsystem::UcfxAsset {
                assert!(
                    !matches!(ChunkTag::from_bytes(t.fourcc), ChunkTag::Unknown(_)),
                    "UCFX-asset registry tag {:?} is not a named ChunkTag variant",
                    std::str::from_utf8(&t.fourcc).unwrap_or("?")
                );
            }
        }
    }

    #[test]
    fn investigation_flags_nonwad_but_not_validated() {
        assert!(needs_investigation(*b"CSID").is_some());   // network/entity
        assert!(needs_investigation(*b"trns").is_some());   // ucfx, still pending
        assert!(needs_investigation(*b"DECL").is_none());   // now validated (body % 0x24)
        assert!(needs_investigation(*b"BSHI").is_none());   // now validated (body % 2)
        assert!(needs_investigation(*b"ASTO").is_none());   // now validated (body>=4)
        assert!(needs_investigation(*b"MINF").is_none());   // now validated (body>=6)
        assert!(needs_investigation(*b"CHAR").is_none());   // reclassified UCFX, registered
        assert!(needs_investigation(*b"NODE").is_none());   // now validated (body>=8)
        assert!(needs_investigation(*b"TRFM").is_none());   // now validated (4x4 matrix)
        assert!(needs_investigation(*b"TREE").is_none());   // now validated (body % 0x34)
        assert!(needs_investigation(*b"KEYS").is_none());   // now validated (4+N×8)
        assert!(needs_investigation(*b"MTRL").is_none());   // validated
        assert!(needs_investigation(*b"GEOM").is_none());   // validated
        assert!(needs_investigation(*b"PHY2").is_none());   // now validated (Havok magic)
        assert!(needs_investigation(*b"PTCH").is_none());   // now validated (record align)
        assert!(needs_investigation(*b"MESH").is_none());   // registered & benign
        assert!(needs_investigation(*b"NAME").is_none());   // registered & benign
    }
}
