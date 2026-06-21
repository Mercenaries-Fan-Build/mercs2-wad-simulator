# Mercenaries 2 FourCC tag registry

Auto-generated from `crates/mercs2_formats/src/tag_registry.rs` — keep in sync.
Scanned from `cmp eax,<imm32>` sites in `output/patched/Mercenaries2.exe` (base 0x00400000).

- **Validated** — a simulator validator checks this chunk's invariant.
- **Registered** — recognized & benign (converter-handled), no structural validator.
- **NeedsInvestigation** — not yet validated; simulator/converter flags it.

Totals: 232 tags — UcfxAsset=69, D3dFormat=7, EntityRuntime=30, NetworkProto=100, LuaReflection=17, Misc=9
Status: Validated=40, Registered=23, NeedsInvestigation=169


## UCFX asset chunk (UcfxAsset)

| Tag | VA | Status | Notes |
|-----|-----|--------|-------|
| `AREA` | 0x0047830a | Validated | container walk @0x4a4ab0; reads 4-byte 'info' header per child; no fixed array |
| `ATRB` | 0x00492b1c | Validated | effect attribute @0x492b1c: reads a 4-byte inner hash then sub-dispatches to per-attribute readers. Validated: body >= 4 |
| `BINN` | 0x0059d008 | Validated |  |
| `BNDS` | 0x004a86dc | Validated |  |
| `BODY` | 0x00750aa5 | Validated |  |
| `COLR` | 0x004930e5 | Validated | colour palette @0x4930e5: stores a fixed 0xC8 (200-byte) record into the effect palette heap. Validated: body >= 0xC8 |
| `COMP` | 0x006549ef | Validated |  |
| `DAMG` | 0x0045f558 | Validated | ECS damage ref array @0x45f558: count×4 u32 refs (count from INFO field, overflow-guarded). Validated: body % 4 == 0 |
| `DEBR` | 0x0045f9a8 | Validated | ECS debris ref array @0x45f9a8: count×4 u32 refs (overflow-guarded). Validated: body % 4 == 0 |
| `DEPS` | 0x0059d0d3 | Validated |  |
| `DICT` | 0x00491386 | Validated |  |
| `EMTR` | 0x00492402 | Validated | emitter @0x492402: reads a u16 count then count×4 alloc (overflow-guarded). Validated: body >= 2 |
| `FRCE` | 0x00491c93 | Validated | force @0x491c93: reads a 4-byte inner hash then sub-dispatches per force type. Validated: body >= 4 |
| `GEOM` | 0x0048ccbd | Validated |  |
| `IBUF` | 0x00478311 | Validated |  |
| `INFO` | 0x0045dc2b | Validated |  |
| `INST` | 0x004a4e51 | Validated | renderable consumer @0x4a4c40: count×0x18 (24B) records, count @esi+0x28 (renderable INFO); alloc overflow-guarded. Validated: body % 0x18 == 0 |
| `KEYS` | 0x004640a8 | Validated | keyframe list @0x4640a8: u32 count header then count×8 keyframe records. Validated: (body-4) % 8 == 0, body >= 4 |
| `MTRL` | 0x004a528d | Validated | tex count@106 -> fixed 10-slot array @+0xAC; >10 overruns (AV 0x84DD5B); parser FUN_00858790 |
| `NODE` | 0x004cf48b | Validated | scene node @0x4cf48b: reads u32 hash + u32 child-count (8B header), then count×0x14 child array (overflow-guarded). Validated: body >= 8 |
| `PART` | 0x0045f8e3 | Validated | ECS particle ref array @0x45f8e3: count×4 u32 refs (overflow-guarded). Validated: body % 4 == 0 |
| `PHY2` | 0x004a845f | Validated | Havok 5.5 collision packfile @0x4a845f: u32 header prefix + embedded packfile (magic SEARCHED, palindromic 57E0E057 10C0C010) + trailing wrapper. Validated by recalculation (havok::validate_phy2): locate packfile, verify length + Havok version + __classnames__ it needs to convert; magic-less PHY2 is valid legacy form |
| `POFF` | 0x004a9cf2 | Validated | effect consumer @0x4a9cf2: reads a fixed 0xC (Vec3) offset into @esi+0x30. Validated: body >= 0xC |
| `PRMG` | 0x0047817e | Validated |  |
| `PRMT` | 0x004783a5 | Validated |  |
| `PTCH` | 0x004a4cbe | Validated | renderable consumer @0x4a4c40: count×0x38 (56B) records, count @esi+0x20 (renderable INFO). (0x1d0 belongs to SCRB.) Validated: body % 0x38 == 0 |
| `PTMS` | 0x004a4e78 | Validated | renderable consumer @0x4a4c40: count×0x08 (8B) records, count @esi+0x30. Validated: body % 8 == 0 |
| `PTYP` | 0x00491ba9 | Validated | particle consumer @0x491ba9: reads a single flags byte (bit0→+0x205, bit1→+0x206). Validated: body >= 1 |
| `SOUN` | 0x0045f76d | Validated | ECS sound ref array @0x45f76d: count×4 u32 refs (overflow-guarded). Validated: body % 4 == 0 |
| `STRM` | 0x004782fd | Validated |  |
| `TEXT` | 0x00492fab | Validated | effect text/texture ref @0x492fab: reads a leading u32 (id/count) then variable data. Validated: body >= 4 |
| `TRCK` | 0x0068e7c3 | Validated | anim track @0x68e7c3: 12-byte inline header (3×u32) then count×4 parallel arrays (overflow-guarded). Validated: body >= 12 |
| `TREE` | 0x0045f629 | Validated | ECS tree/hierarchy array @0x45f629: count×0x34 records (overflow-guarded). Validated: body % 0x34 == 0 |
| `TRFM` | 0x0048cd09 | Validated | transform @0x48cd09: unrolled read of 16×4-byte floats = one 4x4 matrix. Validated: body >= 64 |
| `VALU` | 0x0067c9d7 | Validated | anim VALU @0x67c9d7: (count+1)×width value blob, u32 elements (overflow-guarded). Validated: body % 4 == 0 |
| `data` | 0x004a47d6 | Validated |  |
| `decl` | 0x004a47e2 | Validated |  |
| `flgs` | 0x00654f16 | Validated |  |
| `info` | 0x004a47ea | Validated |  |
| `schm` | 0x00654b6e | Validated |  |
| `AINF` | 0x0068c7de | Registered |  |
| `CEXE` | 0x004cf3d9 | Registered |  |
| `CHDR` | 0x004cf3bb | Registered |  |
| `DATA` | 0x0045f187 | Registered | ECS entity data @0x45f187: delegates body parse to template builder 0x631c90; no self-contained body invariant. Recognized/benign (distinct from lowercase data) |
| `EMIT` | 0x00492703 | Registered | emitter timing @0x492703: delegates body parse to sub-reader 0x48cc30; no self-contained body invariant. Recognized/benign |
| `INDX` | 0x004719f3 | Registered |  |
| `ITEM` | 0x0067c315 | Registered |  |
| `MANM` | 0x0067a844 | Registered | anim-name @0x67a844: allocates a fixed 0x34 in-memory struct; body read is smaller (16 bytes in retail) — no confirmed body invariant. Recognized/benign |
| `MESH` | 0x00471923 | Registered | mesh dispatcher @0x471900: allocates a FIXED 0x10-byte renderable per descriptor, indexed by a u16; no body read, no count-driven array. Engine-safe; no self-contained body invariant |
| `NAME` | 0x00750a8f | Registered |  |
| `SCRB` | 0x004a4cac | Registered |  |
| `SINF` | 0x0067c30e | Registered |  |
| `SKIN` | 0x0047192a | Registered |  |
| `STAT` | 0x004cf5cf | Registered |  |
| `STRS` | 0x004640a0 | Registered |  |
| `SWIT` | 0x004cf5d7 | Registered |  |
| `TINY` | 0x00471a01 | Registered | low-LOD mesh @0x471a01: allocates a FIXED 0x18-byte renderable per descriptor, index-driven (like MESH); no body read. Engine-safe, no body invariant |
| `TRNS` | 0x0068c7e5 | Registered |  |
| `TYPE` | 0x0067c8f9 | Registered | anim TYPE @0x67c8f9: count×2 u16 read where count is caller-passed (external, not in body) — no self-contained body invariant. Recognized/benign (separate from Stance TYPE converter arm) |
| `UNIQ` | 0x006549fb | Registered |  |
| `enum` | 0x006549dd | Registered |  |
| `flgt` | 0x00654f22 | Registered |  |
| `sequ` | 0x0067bfaa | Registered |  |
| `ASTO` | 0x0067c7de | NeedsInvestigation | anim struct array count x0xc (VERIFY handler) |
| `BSHI` | 0x00478318 | NeedsInvestigation | blendshape index; Mesh_ConsumeChunk @0x478318; converter swaps u16 |
| `BSHP` | 0x0047839e | NeedsInvestigation | blendshape data; handler ~0x4a4770; count x0x18 alloc (VERIFY handler) |
| `DECL` | 0x0045dc20 | NeedsInvestigation |  |
| `MINF` | 0x0068e61f | NeedsInvestigation | mesh/anim info; parallel count x4 arrays (VERIFY handler) |
| `trns` | 0x0067e4d5 | NeedsInvestigation |  |

## D3DFORMAT pixel code (D3dFormat)

| Tag | VA | Status | Notes |
|-----|-----|--------|-------|
| `DXT1` | 0x0074d450 | NeedsInvestigation | D3DFORMAT pixel code, not a UCFX descriptor; texture-decode dispatch |
| `DXT2` | 0x0074d57a | NeedsInvestigation | D3DFORMAT pixel code, not a UCFX descriptor; texture-decode dispatch |
| `DXT3` | 0x0074d588 | NeedsInvestigation | D3DFORMAT pixel code, not a UCFX descriptor; texture-decode dispatch |
| `DXT4` | 0x0074d571 | NeedsInvestigation | D3DFORMAT pixel code, not a UCFX descriptor; texture-decode dispatch |
| `DXT5` | 0x0074d5a7 | NeedsInvestigation | D3DFORMAT pixel code, not a UCFX descriptor; texture-decode dispatch |
| `UYVY` | 0x0074d5ae | NeedsInvestigation | D3DFORMAT pixel code, not a UCFX descriptor; texture-decode dispatch |
| `YUY2` | 0x0074d581 | NeedsInvestigation | D3DFORMAT pixel code, not a UCFX descriptor; texture-decode dispatch |

## Runtime entity-type dispatcher (EntityRuntime)

| Tag | VA | Status | Notes |
|-----|-----|--------|-------|
| `ALPU` | 0x009ab6b4 | NeedsInvestigation | runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation |
| `ARBU` | 0x009ab656 | NeedsInvestigation | runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation |
| `CSID` | 0x009ab400 | NeedsInvestigation | runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation |
| `EDGU` | 0x009ab642 | NeedsInvestigation | runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation |
| `GMOH` | 0x009ab552 | NeedsInvestigation | runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation |
| `GNIP` | 0x009ab600 | NeedsInvestigation | runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation |
| `KCAH` | 0x009ab4d5 | NeedsInvestigation | runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation |
| `LNCE` | 0x009ab448 | NeedsInvestigation | runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation |
| `MAGC` | 0x009ab418 | NeedsInvestigation | runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation |
| `MAGE` | 0x009ab44f | NeedsInvestigation | runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation |
| `MAGH` | 0x009ab4dc | NeedsInvestigation | runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation |
| `MAGR` | 0x009ab596 | NeedsInvestigation | runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation |
| `MAGU` | 0x009ab65d | NeedsInvestigation | runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation |
| `NNOC` | 0x009ab41f | NeedsInvestigation | runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation |
| `PEEK` | 0x009ab5b0 | NeedsInvestigation | runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation |
| `QRMH` | 0x009ab54b | NeedsInvestigation | runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation |
| `RESU` | 0x009ab6bb | NeedsInvestigation | runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation |
| `RFXH` | 0x009ab3de | NeedsInvestigation | runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation |
| `SRGE` | 0x009ab456 | NeedsInvestigation | runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation |
| `SRTH` | 0x009ab559 | NeedsInvestigation | runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation |
| `SUBA` | 0x009ab40d | NeedsInvestigation | runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation |
| `TADG` | 0x009ab3ef | NeedsInvestigation | runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation |
| `TNCP` | 0x009ab5c2 | NeedsInvestigation | runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation |
| `TNEP` | 0x009ab5a7 | NeedsInvestigation | runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation |
| `TSLG` | 0x009ab4ce | NeedsInvestigation | runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation |
| `TSLH` | 0x009ab4c5 | NeedsInvestigation | runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation |
| `TSLL` | 0x009ab5bb | NeedsInvestigation | runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation |
| `TSLR` | 0x009ab64b | NeedsInvestigation | runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation |
| `TVLP` | 0x009ab607 | NeedsInvestigation | runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation |
| `VSER` | 0x009ab60e | NeedsInvestigation | runtime entity-type dispatcher @0x9ab; NOT a WAD chunk - requires deeper investigation |

## Network protocol key (NetworkProto)

| Tag | VA | Status | Notes |
|-----|-----|--------|-------|
| `OHCE` | 0x009917ee | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `arbb` | 0x0098ef95 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `atad` | 0x009ce50a | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `bcds` | 0x009d0345 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `cesb` | 0x0098ef8b | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `crsr` | 0x009c2c7c | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `csds` | 0x009d692d | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `csed` | 0x009d666f | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `csid` | 0x009ce3ab | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `csim` | 0x0098efcc | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `daeh` | 0x009ce56d | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `ddav` | 0x009d0371 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `dilc` | 0x009d1854 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `dlot` | 0x0098f031 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `dnab` | 0x0098ef3c | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `dnbb` | 0x009cce52 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `dnib` | 0x009cfd06 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `dnpa` | 0x009ce384 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `dosb` | 0x009c2c6d | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `edoc` | 0x009ce52f | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `edom` | 0x009d18d7 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `edrb` | 0x009c4f84 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `eldi` | 0x009d6435 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `emit` | 0x009ce440 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `enod` | 0x009ce4e8 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `etad` | 0x009ce583 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `etar` | 0x009d18ec | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `euqs` | 0x009ce464 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `gnol` | 0x0098efde | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `hslf` | 0x009d1869 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `hsup` | 0x009d0240 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `htua` | 0x0098ef6d | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `kclb` | 0x009ca789 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `kcos` | 0x009d1800 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `ledv` | 0x009d03c0 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `lfbl` | 0x0098efd6 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `lfmg` | 0x009a5c37 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `llop` | 0x009d0081 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `lluf` | 0x009c99d9 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `lrtc` | 0x009d63bb | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `magn` | 0x0098eff8 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `mand` | 0x009d63e7 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `manx` | 0x009cfdbf | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `maps` | 0x009ce428 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `mgni` | 0x0098efb1 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `morn` | 0x0098f014 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `nedj` | 0x0098ef2e | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `nepo` | 0x009cce1a | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `nftn` | 0x0098f020 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `nlno` | 0x009cceaf | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `nnoc` | 0x009cce59 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `nrud` | 0x009d640e | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `pamx` | 0x009cfd88 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `pdda` | 0x009d65d4 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `peek` | 0x009ce3fe | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `potv` | 0x009d1825 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `ptni` | 0x009d6533 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `ptxe` | 0x009d6503 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `pxam` | 0x009cfd71 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `rapb` | 0x0098ef9d | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `ravg` | 0x009d68ef | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `rcam` | 0x009d644c | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `rdag` | 0x009d6831 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `rdal` | 0x009cffca | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `rdam` | 0x009cffef | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `rdar` | 0x009d031d | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `rdda` | 0x009c5a53 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `reep` | 0x009cfe8a | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `resu` | 0x009c2c9d | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `revh` | 0x009ce3e5 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `rudl` | 0x009d654b | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `sdns` | 0x009d17b5 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `sndx` | 0x009d042f | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `sohn` | 0x0098efc2 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `ssap` | 0x009c2c8c | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `sses` | 0x009c2c96 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `ssim` | 0x009c2c44 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `svcr` | 0x009d1773 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `tats` | 0x009cfeac | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `torp` | 0x0098f006 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `tpgg` | 0x009d685d | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `trba` | 0x009d64e4 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `trop` | 0x009d6563 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `trpa` | 0x009d66bd | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `trpd` | 0x009d67cb | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `trpg` | 0x009d68a0 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `trpl` | 0x009d1764 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `tset` | 0x009d65e7 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `tsmg` | 0x009a5c3e | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `tsoh` | 0x009ce49a | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `tsxe` | 0x009c99d1 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `txth` | 0x009ce58e | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `txtr` | 0x009ce4c1 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `ueuq` | 0x0099dada | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `wten` | 0x009c2c85 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `xamr` | 0x009ce413 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `xcam` | 0x009cce7a | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `ydbr` | 0x009d6489 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `ydob` | 0x009ce578 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |
| `yldn` | 0x009d0031 | NeedsInvestigation | network protocol message key; NOT a WAD chunk - requires deeper investigation |

## Lua property accessor (LuaReflection)

| Tag | VA | Status | Notes |
|-----|-----|--------|-------|
| `alpf` | 0x00976527 | NeedsInvestigation | Lua/object property accessor key; NOT a WAD chunk - requires deeper investigation |
| `bolb` | 0x0097657a | NeedsInvestigation | Lua/object property accessor key; NOT a WAD chunk - requires deeper investigation |
| `bulc` | 0x00976599 | NeedsInvestigation | Lua/object property accessor key; NOT a WAD chunk - requires deeper investigation |
| `gsmx` | 0x0097665c | NeedsInvestigation | Lua/object property accessor key; NOT a WAD chunk - requires deeper investigation |
| `ihca` | 0x00976551 | NeedsInvestigation | Lua/object property accessor key; NOT a WAD chunk - requires deeper investigation |
| `kbdf` | 0x009765a0 | NeedsInvestigation | Lua/object property accessor key; NOT a WAD chunk - requires deeper investigation |
| `knar` | 0x009765e6 | NeedsInvestigation | Lua/object property accessor key; NOT a WAD chunk - requires deeper investigation |
| `knhc` | 0x00976538 | NeedsInvestigation | Lua/object property accessor key; NOT a WAD chunk - requires deeper investigation |
| `musg` | 0x009765ff | NeedsInvestigation | Lua/object property accessor key; NOT a WAD chunk - requires deeper investigation |
| `ossa` | 0x00976541 | NeedsInvestigation | Lua/object property accessor key; NOT a WAD chunk - requires deeper investigation |
| `pcer` | 0x0097664e | NeedsInvestigation | Lua/object property accessor key; NOT a WAD chunk - requires deeper investigation |
| `sbus` | 0x00976655 | NeedsInvestigation | Lua/object property accessor key; NOT a WAD chunk - requires deeper investigation |
| `serp` | 0x0097662d | NeedsInvestigation | Lua/object property accessor key; NOT a WAD chunk - requires deeper investigation |
| `sysf` | 0x009765f8 | NeedsInvestigation | Lua/object property accessor key; NOT a WAD chunk - requires deeper investigation |
| `tcca` | 0x0097654a | NeedsInvestigation | Lua/object property accessor key; NOT a WAD chunk - requires deeper investigation |
| `tlif` | 0x009765a7 | NeedsInvestigation | Lua/object property accessor key; NOT a WAD chunk - requires deeper investigation |
| `wonp` | 0x009765ef | NeedsInvestigation | Lua/object property accessor key; NOT a WAD chunk - requires deeper investigation |

## Unclassified (Misc)

| Tag | VA | Status | Notes |
|-----|-----|--------|-------|
| `CHAR` | 0x004ac973 | NeedsInvestigation | unclassified FourCC immediate - requires deeper investigation |
| `GGGG` | 0x0057e9d5 | NeedsInvestigation | unclassified FourCC immediate - requires deeper investigation |
| `HHlP` | 0x004eea8a | NeedsInvestigation | unclassified FourCC immediate - requires deeper investigation |
| `INVD` | 0x0059cffc | NeedsInvestigation | unclassified FourCC immediate - requires deeper investigation |
| `Mxm ` | 0x00713eb3 | NeedsInvestigation | unclassified FourCC immediate - requires deeper investigation |
| `fVZD` | 0x005f40b8 | NeedsInvestigation | unclassified FourCC immediate - requires deeper investigation |
| `kVAR` | 0x0041d612 | NeedsInvestigation | unclassified FourCC immediate - requires deeper investigation |
| `uZmI` | 0x004f1ab2 | NeedsInvestigation | unclassified FourCC immediate - requires deeper investigation |
| `udn8` | 0x004f19d8 | NeedsInvestigation | unclassified FourCC immediate - requires deeper investigation |
