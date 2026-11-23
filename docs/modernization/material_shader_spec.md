# Material / Shader Spec — Textured, Lit Characters

**Status:** Reimplementation spec (research-derived; not engine code)
**Date:** 2026-06-30
**Consumers:** the native 64-bit Rust/`wgpu` reimplementation ([`docs/modernization/00_charter.md`](00_charter.md)),
Phase 1 ("first triangle from real data") → character rendering.

**Goal:** move character models from untextured point clouds to **tangent-space
normal-mapped, diffuse-lit** meshes, faithful enough to read as the real characters.

## Provenance (what is proven vs inferred)

- **MTRL byte layout** — decompile-verified from `Mtrl_Parse` = `FUN_00858790`
  (`output/_ghidra/all_functions_decomp.txt:641924`), corroborated by
  [`tools/ucfx_mesh_codec.py::parse_mtrl`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/ucfx_mesh_codec.py) and retail-oracle scans (~59k materials).
  Cited byte offsets are **into the material runtime struct** (`param_1`) and,
  separately, **into the on-disk MTRL chunk body**.
- **Vertex decl (DECL64) + tangent** — [`tools/wad_simulator/crates/mercs2_formats/src/model_inject.rs`](../../crates/mercs2_formats/src/model_inject.rs)
  (`DECL64`, lines 37–46; `synth_tangents`, `encode_strm`), plus memory
  `dec3n-tangent-layout-bug` (NORMAL=HEND3N 11-11-10, TANGENT=DEC3N 10-10-10-2).
- **Texture formats** — `texsize.rs::dxt_format` (DXT1 = 8 B/block, DXT5 = 16 B/block).
- **Shader semantics (numeric)** — the actual pixel/vertex ALU is Xbox VMX128 micro-code
  (`.sho`/`.updb`, [`docs/mercs2-pdb-analysis/rendering-shaders.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/mercs2-pdb-analysis/rendering-shaders.md)) and does **not**
  decode; the character surface model below is **reconstructed** from the MTRL inputs
  the engine parses (diffuse+spec+normal slots, spec-power/IOR/fresnel preamble) and the
  `PgSkin*`/normal-map shader family names. Treat the exact BRDF constants as tunable.

---

## 1. MTRL chunk → texture-slot binding

### 1a. On-disk MTRL chunk body (what the mesh reader must parse)

A container's `MTRL` chunk holds a **packed array of material records** (one per
sub-mesh material; PRMT `material_index` selects which). Each record:

```
offset  size          field
 0      104 (26×u32)   PREAMBLE — color/emissive/specular float params (see 1c)
104     u16            flags        (material-type bits; low 16)
106     u16            tex_count    (1..=10; number of texture-hash slots)
108     tex_count×u32  texture asset hashes  (slot order: diffuse, specular, normal, …)
...     rest           trailing float props (specular_power @ prop[16], spec_intensity @ prop[17])
```

- **Inter-record stride = `116 + tex_count*4`** for >99.5% of records
  (`104` preamble + `4` flag/count word + `tex_count*4` hashes + `8` trailing).
  A rare tail carries extra trailing floats — recover by scanning forward to the next
  plausible `[u16 flags | u16 count]` word (count high-byte 0, low-byte 1..10).
- **`tex_count` is the reliable record-boundary signature** (BE-safe: high byte 0,
  low byte 1..10). `flags` is NOT bounded < 0x200 — real materials reach ~0x418.
- **CAUTION (converter, not renderer):** a blanket u32 byteswap transposes the
  `[flags|count]` u16 pair → engine reads `count=128` and walks param-floats as hashes.
  The Rust converter's `convert_mtrl` array-walker fixes this (memory
  `mtrl-flags-count-transposition`). The reimpl reads **already-LE** data, so this is
  only a concern if reading Xbox-endian assets directly.

### 1b. Runtime material struct (how `Mtrl_Parse` binds it — the semantics)

`FUN_00858790(material* p1, stream* p2)` (`__stdcall`, `ret 8`) deserializes the record:

- Reads the 104-byte preamble as float vectors (color/emissive/specular; see 1c).
- `flags` (u16) → `FUN_0084ee70` (pure bit-permute, 0 stream bytes) → `p1+0xA0` (`mov word ptr
  [esi+0xa0], ax` at `0x858b97`, **PROVEN**). What the bits do at draw time is §1d.
- `tex_count` (u16) → `p1+0xA2`.
- Loop `i in 0..tex_count`, **hard-capped at 10** (`if 9 < i goto done`):
  - raw hash → `p1 + 0x144 + i*4` (parallel raw-hash array).
  - a **12-byte lazy-handle record** for slot `i` = `{ asset_hash @ +0xA4+12i, type_hash
    0xF011157A @ +0xA8+12i, flags @ +0xAC+12i }` (the loop's `pbVar14` starts at `+0xAC` and writes
    the hash and type at `-8`/`-4`; **PROVEN**). `0xF011157A` is `TEXTURE_TYPE_HASH` — each slot is a
    deferred **texture** asset reference resolved by the streaming system. A hash of 0 gets no handle.
  - unused slots (i..10) are zero-filled.
- If the runtime `flags & 0x200` (on-disk `0x100`; `FUN_0084ee70` moves bits `0x780` up one): writes
  the texture asset hash `[[0x00dfc2f8]+0x2cf0]` and type `0xF011157A` into `p1+0xA4`/`+0xA8`, **slot
  0's** handle record, after the texture loop (**PROVEN**). `FUN_008242b0(0x100)` there only finds
  and releases the handle a previous bind left. The five retail materials with on-disk `0x100` are all
  `PgDiffRefractNormFP` (**PROVEN**) and leave slot 0 empty; their shader samples the "scene behind" at
  `s0`, so this is the refraction source (**INFERRED**; the global is the colour-buffer copy per the
  render code map).
- **Binding at draw time** (`FUN_00858320`, **PROVEN**): for `i in 0..tex_count`, `FUN_00484280(i, tex)`
  at `0x858390` sets texture **stage `i`** (device `SetTexture`, vtable `+0x104`) from the resident
  runtime entry at `p1+0x11C+4i` (writer not yet found), or unbinds stage `i` when that entry is empty
  or not resident. No default texture is bound, and stages at or past `tex_count` are left as they
  were. Stage 13 (`reflectionMap`) is bound per draw call from key `0x5E84EA6D`, stage 14 from
  `0x16085A8D`, and stage 15 is the shadow atlas (`FUN_00855420`, stream opcode `0x12`). So a slot's
  meaning is the sampler the material's pixel shader declares at that stage: over every retail model
  material, slot 0 is `diffuseMap`, slot 1 `specularMap` or `emissiveMap`, slot 2 `normalMap`
  (`mercs2_engine/tests/material_shaders.rs`).

**Slot semantics (index → role):** slot 0 = **diffuse (albedo)**, slot 1 = **specular /
gloss**, slot 2 = **normal map**, slots 3+ = extra maps (detail/env/mask). This ordering
is the project's working convention (`material_probe.py`, [`tools/ucfx_mesh_codec.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/ucfx_mesh_codec.py)
docstring). Characters are typically `tex_count = 3` (diffuse+spec+normal). For a
`PgDiffEmis*` material slot 1 is the **emissive** map: `PgDiffEmisFP` (`shader3.bin` record 65)
samples `emissiveMap` at `s1` and adds `emissiveMap.rgb · c96` after the lit diffuse, untouched by
shadow or N·L (**PROVEN** from its disassembly). The extra runtime `flags&0x200` texture is the
refraction source (above). **For the minimal renderer, bind slots 0–2.**

### 1c. Preamble float params (offsets into the runtime struct, `p1`)

`Mtrl_Parse` reads the 26 words in order; on-disk word `k` is the u32/f32 at byte `4k` (**PROVEN**,
each read and store address in the `Mtrl_Parse` disassembly, 2026-10-28):

| On-disk word | Runtime off | Transform |
|---|---|---|
| 0 | `+0x64` | raw |
| 1, 2, 3 | `+0x00`, `+0x04`, `+0x08` | raw; `+0x0C` = 1.0 |
| 4, 5, 6 | `+0x10`, `+0x14`, `+0x18` | raw; `+0x1C` = 1.0 |
| 7, 8, 9 | `+0x20`, `+0x24`, `+0x28` | raw |
| 10, 11, 12 | `+0x30`, `+0x34`, `+0x38` | raw; `+0x3C` = 1.0 |
| 13 | `+0x5C` | raw |
| 14 | `+0x58` | `max(w, 9.99e-5)` (`DAT_00b977f8` = `0x38D1B717`) |
| 15 | `+0x50` | raw |
| 16 | `+0x54` | raw **IOR**; also `+0x2C` = `pow((IOR−1)/(IOR+1), 2.0)` = Fresnel F0 (`call 0x466010` → `__libm_sse2_pow`) |
| 17..20 | `+0x40..+0x4C` | raw |
| 21..25 | `+0x68..+0x78` | raw |

### 1d. Flags → render state (PROVEN from `FUN_00858320` / `FUN_00855420`)

`FUN_0084ee70` (`mercs2_formats::texture::runtime_material_flags`) turns on-disk `flags & 7` into a
one-hot (1..4 → 1, 2, 4, 8; else 0) and moves the other bits (`0x008→0x40`, `0x010→0x80`,
`0x020→0x10`, `0x040→0x20`, `0x080→0x100`, `0x100→0x200`, `0x200→0x400`, `0x400→0x800`,
`0x800→0x4000`; `0x1000+` dropped). At draw time, with `F` the runtime word:

| Runtime bit | On-disk bit | Effect |
|---|---|---|
| nibble `0x1/0x2/0x4/0x8` | `flags & 7` = 1..4 | blend mode. BLENDOP = `[0xce8d48][nibble]` = ADD, ADD, SUBTRACT, REVSUBTRACT for 1, 2, 4, 8 (nibble 0 reads as 1). Nibble `0x1` forces SRCBLEND = SRCALPHA, DESTBLEND = INVSRCALPHA |
| `0x10`, `0x20` | `0x020`, `0x040` | SRC = `[0xce8d28][(F>>4)&3]` = ONE, SRCALPHA, DESTALPHA, ONE; DEST = `[0xce8d38][…]` = ONE, ONE, ONE, INVSRCALPHA (both set: BLENDOP 1, SRC 2, DEST 6 literal) |
| `0x40` | `0x008` | ALPHATESTENABLE; ALPHAFUNC 5 (GREATER), ALPHAREF `0x4D` (77) |
| `0x80` | `0x010` | CULLMODE = NONE (two-sided); otherwise a value read at draw time from `*(PTR_PTR_00dfc2fc+0x3fd0)` (unresolved) |
| `0x200` | `0x100` | refraction source texture bound at `+0xA4` (§1b) |

Render-state numbers are D3D9's: SRCBLEND 19, DESTBLEND 20, BLENDOP 171, ALPHATESTENABLE 15,
ALPHAFUNC 25, ALPHAREF 24, CULLMODE 22, set through `FUN_00484310` (device vtable `+0xE4`). Retail
model census (`mercs2_probe mtrl_census`): 15,586 materials mode 0, 503 mode 1 (alpha blend), 72
mode 2 (additive ONE/ONE, mostly `PgDiffEmis*`), none 3 or 4. No mode 1 or 2 material carries
on-disk `0x020`/`0x040`, so retail blending is SRCALPHA/INVSRCALPHA ADD or ONE/ONE ADD only. Tables
read from the v1.0 image: `DAT_00ce8d28` = 2, 5, 7, 2; `DAT_00ce8d38` = 2, 2, 2, 6; `DAT_00ce8d48` =
1, 1, 1, 1, 2, 1, 1, 1, 3 (**PROVEN**; `mercs2_formats::texture::material_blend_state`).

The default `CULLMODE` at `*(PTR_00dfc2fc) + 0x3fd0` reads 2 (`D3DCULL_CW`) in the v1.0 capture
`updated-chris-exports/20261021_chris-jacobs-flying-winch.dmp`. No instruction writing it was found:
the 58 functions that load `PTR_00dfc2fc` only read it, and `FUN_00858320`, `FUN_00858530` and
`FUN_00858600` are its only readers. Since the default applies to every one-sided material, opaque
ones included, it is the cull that keeps a model's outward faces.

ZENABLE is `!(F & 0x2000)` (`FUN_00484310(7, …)`); no on-disk bit maps to runtime `0x2000`, so every
material depth-tests.

### 1e. `materialData` (pixel constants c93..c98)

The pixel CTAB declares `materialData` = `{diffuse, specular, ambient, emissive, subsurface,
constant0}` at c93..c98. The draw uploads it with `SetPixelShaderConstantF` (`0x855f4e`, start and
count from the shader record `+0xbc`/`+0xc0`). In terms of on-disk words (**PROVEN**):

| Register | Source |
|---|---|
| c93 diffuse | (w1, w2, w3, 1) |
| c94 specular | (w4, w5, w6, 1) |
| c95 ambient | (w7, w8, w9, F0) |
| c96 emissive | (w10, w11, w12, 1) |
| c97 subsurface | (w17, w18, w19, w20) |
| c98 constant0 | (w15, IOR = w16, max(w14, 9.99e-5), w13) |

When the draw entry's u16 at `+0x3a` is 1 these go up raw. Otherwise c93..c97 (rgb, and w) are
multiplied by a per-instance colour (`p0..p3`), c96 gets a per-instance offset (`q0..q3`), and
c98.w becomes `p3 · w13`. With runtime bit `0x2` set, c93..c97 are also scaled by `p3 · w13`. The
instance entries (`_DAT_0116a778 + idx·0x20`) are written by `FUN_00854b38`; their meaning is not
yet decoded. The pixel shaders use c93.rgb as the diffuse tint, c95 as ambient, c96 as the emissive
colour, c98.w as the alpha multiplier (`oC0.w = diffuse.a · c98.w` in `PgDiffFP`/`PgDiffEmisFP`, the
constant itself in `PgDiffRefractNormFP`) and c98.y (the IOR) to scale the refraction offset.

**Output alpha by family (PROVEN from the programs).** Over every retail model material's pixel shader
(16,161 materials, `tests/material_shaders.rs`), the final `oC0.w` write is one of five shapes, with
`d` = diffuse texel alpha (`texld …, s0`), `k` = c98.w, `v` = COLOR0 alpha:

| Shape | Shaders | Materials (of them blended) |
|---|---|---|
| `k` | `PgDiffRefract*` | 5 (0) |
| `d · k` | `PgDiff*`, `PgDiffEmis*`, `PgFast*`, rim variants | 6,001 (206) |
| `d · k · v` | non-specular `*AmbOcc*` | 1,148 (43) |
| `saturate(max(d · k, spec))` | `*Spec*` | 6,654 (301) |
| `saturate(max(d · k · v, spec))` | `*Spec*AmbOcc*` | 2,353 (25) |

`spec` is `dp3(specular, (0.299, 0.587, 0.114))`: the luminance of the direct specular term
(light × N·H^c98.z × specularMap × c94), taken before the reflection, vertex-colour and fog terms are
added (read in `PgDiffSpecMetalNormFP` and `PgDiffSpecNormAmbOccFP`). So blended glass turns opaque
where a highlight is brighter than its alpha. `PgMeshVP` passes the vertex COLOR to COLOR0 unchanged
(`mov o1, v1`), so `v` is the mesh's vertex alpha. `OutputAlpha::of_program`
(`mercs2_formats::shader_registrations`) classifies a program by tracing its `oC0.w` write.

---

## 2. Character surface shader model

The character shader family is **`PgSkin*` / normal-mapped** (skinned VP + normal-map
FP; [`docs/mercs2-pdb-analysis/rendering-shaders.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/mercs2-pdb-analysis/rendering-shaders.md)). Reconstructed surface model:

```
N_tan   = 2*sample(normal_map, uv).rgb - 1        // tangent-space normal (DXT5 or DXT1)
TBN     = [ T | B | N ]   (per-vertex, world space; B = cross(N,T)*tangent.w)
N_world = normalize( TBN * N_tan )
albedo  = sample(diffuse, uv).rgb  * material_color.rgb
spec_ms = sample(specular, uv)     // rgb = spec color/mask, optionally a = gloss

L       = normalize(-light_dir)    // single key directional light
NdotL   = max(dot(N_world, L), 0)
diffuse = albedo * NdotL * light_color + albedo * ambient

// Blinn-Phong spec with material spec-power (gloss) and Fresnel F0
H       = normalize(L + V)
spec    = spec_ms.rgb * spec_intensity * pow(max(dot(N_world,H),0), spec_power)
F       = F0 + (1-F0)*pow(1-max(dot(N_world,V),0), 5)   // Schlick fresnel/rim
color   = diffuse + spec*light_color + F*rim_tint       // (rim term optional)
```

Notes / fidelity:
- **Tangent-space normal mapping is the load-bearing effect** — getting the TBN right
  (from the DEC3N tangent) is what makes the character read as lit/detailed rather than
  flat (memory `dec3n-tangent-layout-bug`: wrong tangent → ~125° error → "mangled").
- The real engine adds shadow-buffer shadows, the refraction source (runtime `flags&0x200`), rim,
  and HDR tonemap. **All optional for faithful-enough.** Start with diffuse+normal+one
  directional light; add spec + fresnel next; env/shadows last.
- Vertex COLOR (bgra8) is usually white for characters — multiply in if present.

---

## 3. Minimal `wgpu` implementation spec

### 3a. Vertex attributes the mesh reader must extract (DECL64, stride 40)

Decode from the stride-40 STRM body (`model_inject.rs` DECL64). All half-floats are LE
IEEE-754 f16. **Decode NORMAL/TANGENT already-LE from PC assets; do NOT re-apply the
DEC3N bit-unpack** — that unpack (11-11-10 for normal, 10-10-10-2 for tangent) is an
**Xbox→PC converter** step; converted PC assets store NORMAL/TANGENT as f16x4 in the
stream. The reader just reads f16x4.

| Attr | Byte off | On-disk type | Decode → wgpu attribute |
|---|---|---|---|
| POSITION     | +0  | f16×4 (x,y,z,w=1) | `Float32x3` (drop w) |
| TEXCOORD0    | +8  | f16×2 (u,v)       | `Float32x2` |
| COLOR        | +12 | bgra8 (0xFFFFFFFF) | `Unorm8x4` (or skip) |
| BLENDINDICES | +16 | u8×4              | `Uint8x4` (skinning; ignore for rigid) |
| BLENDWEIGHT  | +20 | u8×4n             | `Unorm8x4` (skinning) |
| NORMAL       | +24 | f16×4 (nx,ny,nz,1) | `Float32x3` (drop w) |
| TANGENT      | +32 | f16×4 (tx,ty,tz,sign) | `Float32x4` (keep w = handedness sign) |

The renderer's minimal vertex is **`{ pos:vec3, uv:vec2, normal:vec3, tangent:vec4 }`**
(add joints/weights when skinning lands). Bitangent = `cross(normal, tangent.xyz) *
tangent.w`. If a source mesh lacks tangents, synthesize them (Lengyel + Gram-Schmidt,
`model_inject.rs::synth_tangents`).

Indices: STRM/IBUF is a **triangle STRIP** (u16); convert to a list or draw as
`PrimitiveTopology::TriangleStrip` (winding: odd triangles reversed — see
`strip_to_tris`).

### 3b. Bind-group layout

```
Group 0 — frame/camera uniform (view_proj, camera_pos)         // std, not detailed here
Group 1 — light uniform:
  @binding(0) uniform Light { dir:vec3, _p0:f32,
                              color:vec3, _p1:f32,
                              ambient:vec3, _p2:f32 }           // 48 B, std140-aligned
Group 2 — material (per draw / per sub-mesh):
  @binding(0) uniform Mtrl  { base_color:vec4, spec_intensity:f32,
                              spec_power:f32, f0:f32, _pad:f32 }
  @binding(1) texture_2d<f32> t_diffuse   // slot 0, DXT1/DXT5 (Bc1/Bc3 in wgpu)
  @binding(2) texture_2d<f32> t_specular  // slot 1
  @binding(3) texture_2d<f32> t_normal    // slot 2
  @binding(4) sampler s_linear            // repeat, linear+mip
```

Texture upload: DXT1 → `TextureFormat::Bc1RgbaUnorm(Srgb)`, DXT5 →
`Bc3RgbaUnorm(Srgb)`. Diffuse = sRGB; **normal + specular = linear** (`Bc*Unorm`, not
Srgb). Upload the full mip chain (`texsize::dxt_mip_count`, down to 4×4). If a slot hash
is 0/unresolved, bind a 1×1 default (white diffuse, `(0.5,0.5,1)` normal, black spec).

### 3c. WGSL (minimal, normal-mapped diffuse + optional spec/fresnel)

```wgsl
struct Camera { view_proj: mat4x4<f32>, cam_pos: vec3<f32> };
struct Light  { dir: vec3<f32>, _p0: f32, color: vec3<f32>, _p1: f32,
                ambient: vec3<f32>, _p2: f32 };
struct Mtrl   { base_color: vec4<f32>, spec_intensity: f32, spec_power: f32,
                f0: f32, _pad: f32 };

@group(0) @binding(0) var<uniform> cam: Camera;
@group(1) @binding(0) var<uniform> light: Light;
@group(2) @binding(0) var<uniform> mtrl: Mtrl;
@group(2) @binding(1) var t_diffuse:  texture_2d<f32>;
@group(2) @binding(2) var t_specular: texture_2d<f32>;
@group(2) @binding(3) var t_normal:   texture_2d<f32>;
@group(2) @binding(4) var s_linear:   sampler;

struct VsIn  { @location(0) pos: vec3<f32>, @location(1) uv: vec2<f32>,
               @location(2) normal: vec3<f32>, @location(3) tangent: vec4<f32> };
struct VsOut { @builtin(position) clip: vec4<f32>,
               @location(0) uv: vec2<f32>, @location(1) wpos: vec3<f32>,
               @location(2) n: vec3<f32>, @location(3) t: vec3<f32>, @location(4) b: vec3<f32> };

@vertex
fn vs(in: VsIn) -> VsOut {
    // NOTE: for skinned characters, blend pos/normal/tangent by bone matrices here first.
    var o: VsOut;
    let wpos = in.pos;                       // model==world for the minimal path
    o.clip = cam.view_proj * vec4<f32>(wpos, 1.0);
    o.wpos = wpos;
    o.uv   = in.uv;
    o.n    = normalize(in.normal);
    o.t    = normalize(in.tangent.xyz);
    o.b    = cross(o.n, o.t) * in.tangent.w; // handedness from tangent.w
    return o;
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let n_tan = textureSample(t_normal, s_linear, in.uv).xyz * 2.0 - 1.0;
    let tbn   = mat3x3<f32>(normalize(in.t), normalize(in.b), normalize(in.n));
    let N     = normalize(tbn * n_tan);

    let albedo = textureSample(t_diffuse, s_linear, in.uv).rgb * mtrl.base_color.rgb;
    let spec_s = textureSample(t_specular, s_linear, in.uv).rgb;

    let L = normalize(-light.dir);
    let V = normalize(cam.cam_pos - in.wpos);
    let H = normalize(L + V);
    let ndl = max(dot(N, L), 0.0);

    let diffuse = albedo * (ndl * light.color + light.ambient);
    let spec    = spec_s * (mtrl.spec_intensity *
                            pow(max(dot(N, H), 0.0), max(mtrl.spec_power, 1.0)));
    let fres    = mtrl.f0 + (1.0 - mtrl.f0) * pow(1.0 - max(dot(N, V), 0.0), 5.0);
    let color   = diffuse + spec * light.color + fres * spec_s * light.color;
    return vec4<f32>(color, mtrl.base_color.a);
}
```

Uniform mapping from the parsed material: `base_color` ← preamble diffuse vector (default
white); `spec_intensity` ← MTRL prop[17]; `spec_power` ← MTRL prop[16] (clamp ≥1);
`f0` ← `(IOR-1)/(IOR+1)` (or a constant ~0.04 if IOR unavailable).

### 3d. Bring-up order (each an oracle-gated step)

1. Diffuse only (slot 0), flat lighting → confirms UV + texture upload + strip winding.
2. Add directional `NdotL` using vertex normal → confirms normal decode.
3. Add normal map + TBN (slot 2) → confirms tangent decode (the `dec3n` correctness gate).
4. Add spec + fresnel (slot 1 + preamble) → material completeness.
5. Later: skinning (BLENDINDICES/WEIGHT + bone matrices), shadows, refraction (runtime `flags&0x200`), HDR.

---

## Cross-references

- [`tools/wad_simulator/crates/mercs2_formats/src/model_inject.rs`](../../crates/mercs2_formats/src/model_inject.rs) — DECL64, f16, strip, `synth_tangents`, `encode_strm`.
- [`tools/wad_simulator/crates/mercs2_formats/src/texsize.rs`](../../crates/mercs2_formats/src/texsize.rs) — DXT1/DXT5 block sizes + mip counts.
- `output/_ghidra/all_functions_decomp.txt:641924` — `Mtrl_Parse` `FUN_00858790`.
- [`tools/ucfx_mesh_codec.py::parse_mtrl`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/ucfx_mesh_codec.py), [`tools/material_probe.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/material_probe.py) — MTRL parse + slot naming.
- [`docs/mercs2-pdb-analysis/rendering-shaders.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/mercs2-pdb-analysis/rendering-shaders.md) — `PgSkin*`/normal-map shader family, pass structure.
- Memory: `dec3n-tangent-layout-bug`, `mtrl-flags-count-transposition`,
  `fun-858790-mtrl-parse-stdcall`, `xbox-pc-vertex-decl-mapping`,
  `worldload-0x84dd5b-texhandle-corruption`.
