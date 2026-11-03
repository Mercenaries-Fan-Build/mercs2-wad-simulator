# Bink 1 (`BIKi`) audio — clean-room specification

Scope: everything a decoder needs to turn the audio tracks of the Mercenaries 2 PC movies into PCM,
plus which tracks the game asks for. The container and video are specified separately; only the
container fields the audio path reads are cited here.

## Provenance and rules

| Input | Identity |
|---|---|
| `binkw32.dll` (install folder) | sha256 `8e669e9e5c920123a1350f6fa4f6e58b507165b93fd88c546ccaaffae68da829`, image base `0x18000000` |
| Ghidra export of that DLL | `output/_ghidra_exports/binkw32.functions.jsonl` |
| Disassembly | radare2 6.2.0 on the DLL file (every VA below was read in r2 unless marked "decomp") |
| `EA.bik` | sha256 `2ccbaf86bc1d86055ea92d02ca686f5cd9dd5f7e309d1361ab050cdc7ad20f06` |
| `Pandemic.bik` | sha256 `0574408d620315a65bee21369780877a66f0ade51edb299b6cf8e8fbcc97347d` |
| `01_AOA_C.bik` | sha256 `77854bd5efc5eeaa5f7405137c1d6914de88394f1c92ce8eeb6d14e26f2d8cb3` |
| Game exe (track selection) | pc-v10 memory dump `game-files/canonical/dumps/mercs2_unpacked.exe`, sha256 `8c179a99…c7ba` (matches `images.tsv`), plus `output/_ghidra_exports/pc-v10.functions.jsonl` |
| Black-box comparison | ffmpeg 9.0.1 (Homebrew), used only as an executable |

Every fact comes from the DLL, the exe, or the file bytes. No third-party Bink decoder source was
read. Labels: **PROVEN** = read or measured this run, with citation; **INFERRED** = reasoned from
proven facts, not checked; **SPECULATIVE** = not used. "Black-box PROVEN" means: a scratch decoder
written from this document reproduces ffmpeg's output (§9).

The audio codec is compiled into `binkw32.dll`. The DLL's only audio-related imports are WINMM
`waveOut*` and delay-loaded `mss32.dll` `AIL_*` sample-output functions (r2 `ii`); neither decodes
Bink audio. No other install file is involved. **PROVEN**

All integers in the file are little-endian.

---

## 1. Container fields the audio path reads (owned by the container spec)

`_BinkOpen@8` (`0x1800d670`) reads the first `0x2c` bytes into a 11-dword header, then reads three
per-track tables and the frame-offset table, in that order (I/O-callback path, decomp lines with
`puVar2[0xa9]`, `[0xaa]`, `[0xab]`, `[0x53]`; the in-memory path at the same function confirms the
offsets: tables at `param_1 + 0xb` dwords, `+ n`, `+ 2n`, `+ 3n`). **PROVEN**

| Offset | Field | Audio use |
|---|---|---|
| `0x00` | magic `BIKf`/`BIKg`/`BIKh`/`BIKi` | all shipped files: `BIKi` |
| `0x08` | frame count | |
| `0x10` | `header[4]`: the frame-offset table has `header[4] + 1` entries; BinkOpen rejects the file if `header[4]` < frame count or > 1000000 | equals the frame count in every shipped file |
| `0x1c`, `0x20` | fps numerator, denominator | output-rate rescale only (§3.3) |
| `0x28` | `n` = number of audio tracks (BinkOpen rejects `n > 0x100`) | |
| `0x2c` | `max_buffer[n]` (u32 each) | |
| `0x2c + 4n` | `flags[n]` (u32 each) | §2 |
| `0x2c + 8n` | `track_id[n]` (u32 each) | §3 |
| `0x2c + 12n` | frame offsets, `header[4] + 1` u32 entries; bit 0 is masked off before use (`& 0xfffffffe`, `0x1800d670` in-memory path) | frame `i` = bytes `[off[i] & ~1, off[i+1] & ~1)` |

### 1.1 Per-frame packet: audio first

`_BinkGetTrackData@8` (`0x180105f0`, decomp) and the playback decoder `0x1800f1a0` walk the frame the
same way. The frame begins with one record per audio track, in track-table order, followed by the
video data. **PROVEN**

```
for t in 0..n:
    len  : u32                      // bytes that follow for this track; 0 = no audio this frame
    if len != 0:
        out_bytes : u32             // total PCM bytes this record decodes to (16-bit: 2 bytes/sample)
        blocks    : len - 4 bytes   // back-to-back coded blocks (§5)
    // next record starts at (address of len) + 4 + len
```

Bounds the DLL checks, and stops on (it returns 0 / skips, no error): `p + 4 > frame_end`, or
`p + 4 + len > frame_end` (`0x180105f0`). A conforming decoder must treat either as a hard error.

Measured on EA.bik frame 0, track 0: `len = 0x3604` (13828), `out_bytes = 0x23a00` (145920) at file
offset `0x2b8`; ffmpeg reports the same 13828 as the packet size. **PROVEN**

---

## 2. Track flags

From `_BinkOpen@8` (`0x1800d670`, decomp lines 374–437) and `_BinkOpenTrack@8` (`0x18010470`):

| Bits | Meaning | Citation |
|---|---|---|
| `flags & 0xffff` | sample rate in Hz | `uVar16 & 0xffff` passed as rate to `0x180136e0` |
| bit 28 (`0x10000000`) | track has audio, transform = **DCT path** | |
| bit 31 (`0x80000000`) | track has audio, transform = **RDFT path**; wins over bit 28 | `if ((int)flags < 0 \|\| !(flags & 0x10000000)) dct = 0 else dct = 1` |
| neither 28 nor 31 | track is not opened (`(flags & 0x90000000) != 0` gate) | |
| bit 29 (`0x20000000`) | channels = 2, else 1: `((flags >> 29) & 1) + 1` | |
| bit 30 (`0x40000000`) | output 16-bit, else 8-bit: `((flags >> 27) & 8) + 8` | |

All **PROVEN** as code reads. ffmpeg (black box) names bit-28 streams `binkaudio_dct`.

`max_buffer[t]`: BinkOpen sizes the sound buffer from it (`((max_buffer*5)>>2 + 0xff) & ~0xff`).
On the three measured tracks it equals the largest per-frame `out_bytes`, which is frame 0's
(EA t0 145920, Pandemic t0 138240, 01_AOA_C t3 34560). **PROVEN** for those tracks; the general
meaning is **INFERRED**.

### 2.1 Shipped PC data (all 45 `Data\Movies\*.bik`, read this run)

- Every audio track has bit 28 set, bit 31 clear, bit 30 set: **DCT path, 16-bit, everywhere**.
  No shipped track uses the RDFT path or 8-bit output.
- Rates: `0x5622` 22050 Hz (story movies), `0xac44` 44100 Hz (Pandemic), `0xbb80` 48000 Hz
  (EA, attract).
- Track sets: 35 story movies have 8 tracks, ids `0, 2, 3, 2001, 2003, 2004, 2005, 2006`
  (ids 0 and 3 stereo, the rest mono). EA and Pandemic: 4 tracks, ids `0 (st), 1, 2, 3 (st)`.
  `attract.bik`: 1 track, id 0, 48 kHz stereo. 7 files (shell menus, `01_VIK_01`, `title_*`) have
  no audio.
- Correction to `docs/reverse_engineer/disc_media_inventory.md` §2.1 (which read a disc image):
  the **PC** `01_AOA_C.bik` has **8** tracks at **22050 Hz** and is 1024×576, not 9 tracks at
  48 kHz; the PC `EA.bik` header says 15/1 fps, not 30/1.

---

## 3. Which tracks play

### 3.1 The game's request — PROVEN except where marked

- `FUN_00709d70` (pc-v10) writes the language track id to `record + 0x158`: default `0x7d1` (2001),
  case 1 `0x7d5`, case 2 `0x7d6`, case 3 `0x7d4`, case 4 `0x7d3`, switching on `DAT_00cf255c`
  (filled by `FUN_00826a90` when `-1`), then calls `_BinkSetSoundTrack(4, record + 0x14c)`
  (`0x00709e1b`).
- The four dwords at `record + 0x14c` in the pc-v10 dump are `0, 2, 3, 0x7d1` for all four movie
  records (`0x017c6640`, `0x017c67ac`, `0x017c6918`, `0x017c6a84`). The code that stores `0, 2, 3`
  was not located (see Open).
- After the open, the exe calls `_BinkSetVolume(handle, id, vol)` with ids `ebx`, `2`, `3` and
  `[record + 0x158]` (`0x00709f0b`–`0x00709f33`, pc-v10 dump). `ebx` is the register the handle
  is compared against for the "FileNotFound" branch at `0x00709ec1`, so it is 0 (**INFERRED**).
  `_BinkSetVolume` (`0x180102a0`) finds the opened track by **id** via `0x18010260`. So the game
  addresses the ids {0, 2, 3, language}.

### 3.2 How binkw32 turns the request into tracks — PROVEN

- `_BinkSetSoundTrack@8` (`0x1800d300`): stores up to 32 ids (count clamped to `0x20`) in
  `DAT_1802e5c0[]`, count in `DAT_1802e5b8`.
- `_BinkOpen@8`: if the open flags lack `0x4000`, the request is reset to the single id 0 at entry
  (`if ((param_2 & 0x4000) == 0) { count = 1; ids[0] = 0; }`). After the open the request is
  reset to `{0}` again.
- For each requested id in request order, the **first** track whose `track_id` equals it is
  appended to the open list (`bink + 0x11c`, count `bink + 0x114`). A requested id with no
  matching track adds nothing (silently).
- Each listed track is then opened on the sound system if its flags pass §2 and
  `((max_buffer*5)>>2 + 0xff) & ~0xff <= (channels * bits * rate) >> 2`; a track that fails is
  removed from the list.

The flags the game passes to `_BinkOpen` are not readable statically: the call is at `0x02490c06`
inside SecuROM-relocated code, arguments pushed through VM helper `0x1aaff10`. Whether `0x4000` is
set is therefore **open**; the SetVolume calls on ids 2, 3 and language make it **INFERRED** that
it is.

Result if the 4-id request is honoured (**INFERRED**, from the above):

| File | Tracks opened (index → id) |
|---|---|
| EA.bik, Pandemic.bik | 0→0 (stereo), 2→2 (mono), 3→3 (stereo); id 0x7d1 absent |
| story movies (English) | 0→0 (st), 1→2, 2→3 (st), 3→2001 |
| story movies, language case 1/2/3/4 | language track 0x7d5→idx 6, 0x7d6→idx 7, 0x7d4→idx 5, 0x7d3→idx 4 |
| attract.bik | 0→0 |

Each opened track is decoded independently (its own decoder state, §4) and handed to the sound
system as its own voice; mixing is the sound system's job, not the codec's.

### 3.3 Output rate — PROVEN (not part of decoding)

BinkOpen gives the sound system `rate' = round(rate * fps_den_file * play_num /
(fps_num_file * play_den))` when a playback frame rate is set (`BinkSetFrameRate` + open flag
`0x1000`), else `rate`; clamped to `0x3e800`. Decoding itself always uses the header rate.

---

## 4. Decoder setup — `FUN_180136e0` (fastcall: ECX, EDX = rate, stack: channels, dct)

Called from BinkOpen (`0x1800e37d`) and `_BinkOpenTrack` (`0x180104cf`) with
`(channels, flags & 0xffff, channels, dct)`. All **PROVEN** (asm `0x180136e0`–`0x180138e8`).

```
base = 2048            if rate >= 44100
     = 1024            if 22050 <= rate < 44100
     =  512            if rate < 22050
total = base * channels                      // int16 samples per block before overlap removal
if dct: N = base;  ch_loop = channels; r = rate
else:   N = total; ch_loop = 1;        r = rate * channels
half   = N >> 1
hr     = (r + 1) >> 1
nb     = smallest i in 0..24 with hr <= FREQ[i]; 25 if none
band[i] = max(1, (FREQ[i] * half) / hr)      for i < nb   (u32 multiply, u32 divide)
band[nb] = half
scale  = (f32) (2.0 / sqrt((float)N))        // fild, fsqrt, fdivr [0x180212a0]=2.0f, fstp dword
block_bytes   = total * 2                    // state +0x08
overlap_bytes = block_bytes >> 4             // state +0x0c  (= total/8 bytes = total/16 int16)
first_block   = 1                            // state +0x14
```

Decoder state layout (offsets in bytes, for reference): `+0x00 N`, `+0x04 scale`, `+0x08
block_bytes`, `+0x0c overlap_bytes`, `+0x10 ch_loop`, `+0x14 first_block`, `+0x18 nb`, `+0x1c`
int16 output buffer, `+0x20` overlap buffer (at `+0xa0`), `+0x24` FFT work ints (first dword
zeroed = "tables not built"), `+0x28` FFT float tables, `+0x2c dct flag`, `+0x34 band[0..nb]`.

`FREQ` = 25 u32 at `0x1802e018` (file offset `0x22618`), decimal:

```
0 100 200 300 400 510 630 770 920 1080 1270 1480 1720 2000 2320 2700
3150 3700 4400 5300 6400 7700 9500 12000 15500
```

(raw: `00000000 00000064 000000c8 0000012c 00000190 000001fe 00000276 00000302 00000398 00000438
000004f6 000005c8 000006b8 000007d0 00000910 00000a8c 00000c4e 00000e74 00001130 000014b4 00001900
00001e14 0000251c 00002ee0 00003c8c`; the next dword `0x1802e07c` is 0 and is never indexed.)

Worked values (computed from the above; the 48000, 44100 and 22050-mono rows were also printed by
the scratch decoder that matched ffmpeg, §9):

| rate, ch | N | nb | band[0..nb] | scale | overlap int16 | emitted int16 / block |
|---|---|---|---|---|---|---|
| 48000, 2 | 2048 | 25 | 1 4 8 12 17 21 26 32 39 46 54 63 73 85 98 115 134 157 187 226 273 328 405 512 661 1024 | 0.0441941731 | 256 | 3840 (1920 frames) |
| 44100, 2 | 2048 | 25 | 1 4 9 13 18 23 29 35 42 50 58 68 79 92 107 125 146 171 204 246 297 357 441 557 719 1024 | 0.0441941731 | 256 | 3840 (1920 frames) |
| 22050, 1 | 1024 | 23 | 1 4 9 13 18 23 29 35 42 50 58 68 79 92 107 125 146 171 204 246 297 357 441 512 | 0.0625 | 64 | 960 |
| 22050, 2 | 1024 | 23 | same as 22050 mono | 0.0625 | 128 | 1920 (960 frames) |

---

## 5. Block bitstream — `FUN_180132a0` + `FUN_18012f60`

### 5.1 Bit reader — PROVEN

Input is consumed as little-endian **u32 words**, bits **LSB first**. State: `buf` (u32), `avail`
(valid bits in `buf`), word pointer. Reading `n` bits (1 ≤ n ≤ 29):

```
if avail < n:
    w = next u32 word                    // load only when needed
    value = ((w << avail) | buf) & ((1 << n) - 1)
    buf   = w >> (n - avail)
    avail = avail + 32 - n
else:
    value = buf & ((1 << n) - 1)
    buf >>= n; avail -= n
```

The DLL inlines this with `n` = 29 (`0x1801332c`), 8 (`0x18013432`), 1 and 4 (`0x18012ff7`,
`0x1801301f`, `0x18013088`), `width` (`0x180131cf`), 1 for the sign (`0x18013229`). The sign-bit
load at `0x18013229` has no end-of-input check; every other load checks `ptr >= end` and abandons
the block. A conforming decoder hard-fails on any read past `end`.

Each block starts a **fresh** reader at a word-aligned address (`0x180132a0` takes a start pointer
and resets `buf`/`avail`). A block consumes **exactly the words it loaded**:
`consumed_bytes = (ptr_after - ptr_start)` (`0x180136bd`–`0x180136cc`), i.e. 4 × ⌈bits/32⌉. The
next block in the record starts there (`0x18013920` returns `start + consumed`). Measured: on EA t0,
Pandemic t0 and 01_AOA_C t3, every record's blocks end exactly at the record end. **PROVEN**

### 5.2 Block layout

```
if dct: skip 2 bits                      // 0x180132db: first word loaded, buf = w >> 2, avail = 30
                                         // (if no input at all: return 0 consumed, nothing decoded)
for ch in 0..ch_loop:                    // channels share one reader, back to back
    coeff[0] = read_float29()
    coeff[1] = read_float29()
    for i in 0..nb: q_index[i] = read(8); quant[i] = QUANT[min(q_index[i], 95)]
    read_coefficients(coeff[2..N])       // §5.4
    transform(coeff[0..N])               // §6, in place, per channel
pcm_out()                                // §7
```

### 5.3 `read_float29` — PROVEN (`0x18013353`–`0x18013391`)

```
v    = read(29)
mant = (v >> 5) & 0x77fffff              // bits 5..27 (23 bits); the mask also clears bit 23 = sign
f    = (float)(int)mant * EXP[v & 31]    // fild, fmul dword [0x1802e4a8 + 4*(v&31)], fstp dword
if v & 0x10000000: f = -f                // bit 28 = sign
```

`EXP` = 32 dwords at `0x1802e4a8` (file `0x22aa8`). Entries 0–23 are `2^(e-23)`
(`0x34000000, 0x34800000, …, 0x3f800000`, each `+0x00800000`). Entries 24–31 are whatever the DLL
has there: `0xffffffff 0x00000000 0x00000000 0x00000001 0x00000002 0x00000003 0x00000004
0x00000005` (as floats: NaN, 0, 0, denormals). Shipped data never uses them: the largest exponent
seen was 15 / 17 / 16 on EA t0 / Pandemic t0 / 01_AOA_C t3. A conforming decoder should reject
`e >= 24` rather than reproduce NaN. The product is exact (`mant < 2^23`, power-of-two factor).

### 5.4 Band quantisers — PROVEN (`0x18013420`–`0x180135ea`)

`nb` reads of 8 bits; values above 95 are clamped to 95; `QUANT` = 96 floats at `0x1802e328`
(file `0x22928`), raw u32 bit patterns in index order:

```
3f800000 3f95253b 3fadc8b9 3fca7e34 3febf1cf 4009760e 40202b68 403aa11e
405975d6 407d626c 40939f23 40ac022f 40c86c93 40e988af 41080e85 411e887a
4138b8fb 41573d0f 417acbaf 41921d06 41aa404a 41c6605b 41e725de 4206aaa8
421ce9d4 4236d5d5 42550a18 42783bb8 42909edc 42a882fd 42c4597f 42e4c94a
43054a6e 431b4f6a 4334f79e 4352dce1 4375b274 438f2499 43a6ca3d 43c257ef
43e272e4 4403edce 4419b931 44331e4a 4450b55c 44732fd3 448dae34 44a515fe
44c05b9f 44e0229b 450294bd 4518271f 453149cc 454e937a 4570b3c2 458c3ba2
45a36634 45be6480 45ddd85f 46013f33 46169929 462f7a18 464c772b 466e3e31
468accd9 46a1bad4 46bc7285 46db9420 46ffda4c 47150f44 472daf20 474a6061
476bcf0e 478961cf 47a013d1 47ba85a1 47d955cf 47fd3d1a 48138965 482be8d9
48484f0e 4869664a 4887fa7b 489e7121 48b89dc6 48d71d5c 48faa6bf 49120781
492a2736 49464323 496703d3 498696d3 499cd2b7 49b6bae7 49d4eab7 49f81728
```

(QUANT[0] = 1.0, QUANT[95] ≈ 2032357.0.) Use the bit patterns; do not regenerate the table from a
formula. Largest index seen in shipped data: 94 (EA t0, Pandemic t0), 69 (01_AOA_C t3).

### 5.5 Coefficients — `FUN_18012f60` — PROVEN (asm `0x18012f60`–`0x18013294`)

Band `k` covers coefficient indices `[2*band[k], 2*band[k+1])` and uses `quant[k]`. Since
`band[0] = 1`, coefficient 2 is the first index of band 0.

```
RL = [2,3,4,5,6,8,9,10,11,12,13,14,15,16,32,64]   // 16 bytes at 0x1802e314 (file 0x22914)
i = 2; k = 0; q = 0.0
while i < N:
    if read(1) == 0: end = i + 8
    else:            end = i + 8 * RL[read(4)]
    end = min(end, N)
    width = read(4)
    if width == 0:
        coeff[i..end] = 0.0                          // memset, no further bits
        i = end
        while 2*band[k] < end: q = quant[k]; k += 1  // strictly less
    else:
        while i < end:
            if i == 2*band[k]: q = quant[k]; k += 1  // equality, at most one band per index
            v = read(width)
            if v == 0: coeff[i] = 0.0                // no sign bit
            else:
                coeff[i] = (read(1) ? -q : q) * (float)(int)v   // fimul, fstp dword: one f32 rounding
            i += 1
```

Notes, all PROVEN from the asm:
- The equality test advances at most one band per coefficient. With the shipped tables two band
  edges never coincide (FREQ steps ≥ 100 Hz give ≥ 4 coefficients per band at every rate in §4),
  so this never skips a band; a decoder may assert `band[k] < band[k+1]`.
- The negation is applied to `q` before the multiply (`fld st(0)` / `fld st(1)` choose between
  `-q` and `q`), which is bit-identical to negating the product.
- A sign bit is read only for non-zero `v`.
- Largest `width` seen in shipped data: 5 (EA t0, Pandemic t0), 6 (01_AOA_C t3).

---

## 6. Transform

Input: `coeff[0..N)` per channel (f32). Output: `x[0..N)` time samples (f32), in place.

### 6.1 DCT path (bit 28) — `FUN_18019d40` — the path every shipped track uses

**Closed form — black-box PROVEN** (§9):

```
x[k] = Σ_{j=0}^{N-1} coeff[j] · cos(π · j · (k + ½) / N),    k = 0..N-1
```

(no halving of `coeff[0]`, no normalisation; the `scale` of §4 is applied in §7.)

The DLL computes it as this chain (each step read in asm; formulas below are PROVEN unless marked):

1. Table setup, cached per decoder in the `+0x24` int area (`ip[0] = nw`, `ip[1] = nc`):
   `nw = N >> 2`, built by `FUN_18019e50` if `N > 4*ip[0]`; `nc = N`, built by `FUN_18019f60` at
   `w + nw` if `N > ip[1]` (`0x18019d51`–`0x18019da6`).
   - `FUN_18019f60` (nc): `nch = nc >> 1`, `δ = (π/4)/nch` with `π/4` = f32 `0x3f490fdb` at
     `0x18021288`; `c[0] = cos(δ·nch)`, `c[nch] = 0.5f·c[0]`, for `j = 1..nch-1`:
     `c[j] = cos(jδ)·0.5`, `c[nc-j] = sin(jδ)·0.5` (the 0.5 here is the f64 `0.5` at
     `0x18021280`).
   - `FUN_18019e50` (nw): `nwh = nw >> 1`, `δ = (π/4)/nwh`; `w[0] = 1.0`, `w[1] = cos(δ·nwh)`; if
     `nwh >= 4`: `w[2] = 0.5/cos(2δ)`, `w[3] = 0.5/cos(6δ)` (6.0 at `0x18021290`); for
     `j = 4, 8, … < nwh`: `w[j] = cos(jδ)`, `w[j+1] = sin(jδ)`, `w[j+2] = cos(3jδ)`,
     `w[j+3] = sin(3jδ)` (3.0 at `0x1802128c`); then sub-tables: `nw0 = 0`; while `nwh > 2`:
     `nw1 = nw0 + nwh; nwh >>= 1; w[nw1] = 1.0; w[nw1+1] = w[1]`; if `nwh >= 4`:
     `w[nw1+2] = 0.5/w[nw0+4]`, `w[nw1+3] = 0.5/w[nw0+6]`, and for `j = 4, 8, … < nwh`:
     `w[nw1+j+m] = w[nw0+2j+m]` for `m = 0..3`; `nw0 = nw1`.
2. Pre-rotation `FUN_1801e360(n=N, a, nc, c)`: `m = N >> 1`, `ks = nc / N` (= 1), `kk = 0`; for
   `j = 1..m-1`: `kk += ks; k = N - j; wkr = c[kk] - c[nc-kk]; wki = c[kk] + c[nc-kk];
   t = wki·a[j] - wkr·a[k]; a[j] = wkr·a[j] + wki·a[k]; a[k] = t`. Then `a[m] *= c[0]`.
3. Complex stage `FUN_18019fd0(N, ip+2, nw)` (n > 4; `N = 4` takes a 2-point butterfly): an
   in-place complex DFT of the `N/2` values `z_j = a[2j] + i·a[2j+1]`, output in natural order:
   `Z_k = Σ_j z_j · e^{+2πi·jk/(N/2)}`. **PROVEN by consistency**: with steps 2, 4, 5 transcribed
   from the asm, only the `+` exponent reproduces the closed form (max error 1.9e-11 at N = 2048,
   vs 98.9 with `−`; scratch test `chain`). The size-8 leaf `FUN_1801e110` has the `+` sign
   (out[1] = (z0 − z2) + i(z1 − z3)). Its internal radix/ordering (callees `0x1801ace0`,
   `0x1801bd70`, `0x1801be10`, `0x1801beb0`, `0x1801cfc0`, `0x1801a250`, `0x1801d030`,
   `0x1801aa90`, `0x1801dce0`, `0x1801e110`) was **not transcribed** (Open).
4. Post-rotation `FUN_1801e1f0(n=N, nc, c)` (only when `N > 4`): `m = N >> 1`, `ks = 2·nc / m`,
   `kk = 0`; for `j = 2, 4, … < m`: `k = N - j; kk += ks; wkr = 0.5 - c[nc-kk]; wki = c[kk];
   xr = a[j] - a[k]; xi = a[j+1] + a[k+1]; yr = wkr·xr - wki·xi; yi = wkr·xi + wki·xr;
   a[j] -= yr; a[j+1] -= yi; a[k] += yr; a[k+1] -= yi`. (0.5 = f32 at `0x1802127c`.)
5. Butterfly (`0x18019e0c`–`0x18019e40`): `t = a[0] - a[1]; a[0] = a[1] + a[0];` for
   `j = 2, 4, … < N`: `a[j-1] = a[j] - a[j+1]; a[j] = a[j+1] + a[j];` finally `a[N-1] = t`.

All arithmetic is x87 on f32 memory operands, results stored as f32 (`fstp dword`).

### 6.2 RDFT path (bit 31) — `FUN_18019c60` — **not used by any shipped file**

Step list read from the asm (**PROVEN** as code): tables `nw = N >> 2` (`FUN_18019e50`), `nc =
N >> 2` (`FUN_18019f60`, note: not `N`); `a[1] = (a[0] - a[1]) · 0.5f; a[0] = a[0] - a[1]`; if
`N > 4`: `FUN_1801e2a0(n=N, nc, c)`: same loop shape as step 4 but `yr = wkr·xr + wki·xi;
yi = wkr·xi - wki·xr` (then the same four updates), followed by complex stage `FUN_1801a120`; if
`N == 4` only the complex stage.

Closed form **INFERRED** (scratch test `rchain`: with the transcribed steps, a complex stage of
exponent `−2πi·jk/(N/2)` gives max error 6.4e-12 at N = 1024 against it; the size-8 leaf
`FUN_1801e180` has the `−` sign; not verified against any decoded output because no shipped track
uses this path):

```
x[k] = ½(coeff[0] + coeff[1]·(−1)^k) + Σ_{j=1}^{N/2−1} [coeff[2j]·cos(2πjk/N) + coeff[2j+1]·sin(2πjk/N)]
```

In this path `N = base·channels`, `ch_loop = 1`, and the time output is converted by the mono
routine (§7), so a stereo track's interleaving would come out of the transform itself
(**INFERRED**).

---

## 7. PCM conversion, interleave, overlap

### 7.1 Float → int16 — PROVEN (`FUN_18012db0`, `FUN_18012e80`)

Called after all channels are transformed (`0x18013681`). For each sample: `fld x; fmul scale;
fistp dword` → 32-bit integer `r`; then `r > 0x7fff → 0x7fff`, `r < -0x8000 → 0x8000`, else low
16 bits.

- `ch_loop == 1` (`FUN_18012db0`): writes `N` int16 in order (processed two at a time).
- `ch_loop == 2` (`FUN_18012e80`): writes `N` frames `L = x0[i], R = x1[i]` interleaved, where
  `x1` is the second channel's buffer (`x0 + N`).

`fistp` rounds with the thread's x87 rounding control. `binkw32.dll` contains no `fldcw`,
`fnstcw`, `fstcw` or `ldmxcsr` in any executable section (r2 linear sweep), so the mode is the
host thread's: round-to-nearest-even unless the game changed it (**INFERRED**; see Open). The
verification decoder used round-half-even and matched (§9).

### 7.2 Overlap — PROVEN (`FUN_18013920`, asm `0x18013956`–`0x180139bd`)

Let `T = total` int16 just produced (§4) and `M = overlap_bytes / 2 = T / 16`.

```
if first_block: first_block = 0                     // no crossfade on the decoder's first block
else:
    for i in 0..M:                                  // i indexes interleaved int16, not frames
        s = prev[i] * (M - i) + out[i] * i          // 32-bit signed (movsx, imul, add)
        out[i] = low16( (u32)s / M )                // `div edi`: UNSIGNED division
prev[0..M] = out[T-M .. T]                          // memmove, after the crossfade
emit out[0 .. T-M]                                  // T - M int16 = block_bytes - overlap_bytes
```

The unsigned divide is real (`xor edx,edx; div edi`). For every shipped configuration `M` is a
power of two (64, 128 or 256), and then `low16((2^32 + s) / M) = floor(s / M)` for negative `s`,
i.e. an arithmetic shift right; positive `s` truncates. `first_block` is set only by
`FUN_180136e0` (`0x180138da`; no other `mov [reg+0x14], 1` in the DLL's disassembly, PROVEN), so a
decoder state is never re-primed after its first block; that seeking keeps the same decoder state
is **INFERRED** (seek path not read).

---

## 8. Decoding a frame's record — PROVEN (`0x180105f0`, `0x1800f1a0`)

```
remaining = out_bytes
p = record + 8; end = record + 4 + len
while remaining != 0:
    produced_bytes, consumed = decode_block(p, end)  // §5–§7; produced = (T - M) * 2
    take = min(produced_bytes, remaining)            // the last block of the stream is truncated
    output out[0 .. take/2]
    remaining -= take; p += consumed
```

- Truncation happens only at the end of the stream in the measured tracks: Pandemic t0 frame 673
  (7680 produced, 6920 kept), 01_AOA_C t3 frame 3181 (1920 produced, 572 kept); EA t0 never.
  A decoder must keep the clamp; ffmpeg does not (it emits whole blocks, §9).
- A block is decoded even if its output is then truncated, and its tail still enters `prev`.
- 8-bit tracks (bit 30 clear, none shipped): `_BinkGetTrackData` emits one byte per sample,
  `(high byte of the int16) − 0x80` (mod 256), i.e. `(s >> 8) ^ 0x80`.
- Frame 0 carries a large pre-roll: EA t0 declares 145920 bytes (19 blocks), Pandemic t0 138240,
  01_AOA_C t3 34560.

---

## 9. Verification

A scratch decoder written from §1–§8 (C, double-precision naive O(N²) transform, round-half-even,
the §7.2 integer crossfade) was compared with ffmpeg used only as a black box. Scratch files:
`/private/tmp/claude-501/-Volumes-Projects-mercenaries-game/88b95516-0035-44fa-a901-747b4da2d496/scratchpad/bink_audio/`
(`bkatest.c`; not part of the repo).

**EA.bik track 0** (48000 Hz stereo, DCT): `ffmpeg -i EA.bik -map 0:a:0 -f f32le` and `-f s16le`.

| Prediction (from this spec) | ffmpeg | |
|---|---|---|
| 48000 Hz, 2 channels | 48000, stereo | match |
| 1920 frames (3840 int16) per block | `nb_samples=1920` on all 250 decoded frames | match |
| 250 blocks, 960000 int16 = 480000 frames = 10.000 s | 1,920,000 bytes s16le; duration 10.000000 | match |
| 140 non-empty records (`len != 0`) in 150 frames, counted by the scratch decoder | 140 packets, first `pkt_size=13828` | match |

Sample values: max |ours − ffmpeg_f32·32768| = 1.48; after rounding ffmpeg's float, 944023 of
960000 samples are identical and 15977 differ by 1, of which **15813 lie in the first 256 int16 of
a block** (the integer crossfade of §7.2 vs a float crossfade). Against ffmpeg's own s16 output:
max |Δ| = 1. Our output sha256 `09b7650965d1735993762cf02dfe116e6c0182fa49602a4e1eef1d2c5867c5a1`.

**Pandemic.bik track 0** (44100 Hz stereo): 276 blocks; 1059460 int16 vs ffmpeg 1059840 (ffmpeg
does not apply the §8 clamp; 380 extra). Over the common length: max |Δ| = 1.48 vs float, 35025
off-by-one, 34379 of them in crossfade regions.

**01_AOA_C.bik track 3** (id 2001, 22050 Hz mono): 2453 blocks; 2354206 vs ffmpeg 2354880 (674
extra, same reason). Max |Δ| = 1.47; 57515 off-by-one, 56724 in crossfade regions.

What this proves: the bitstream, tables, band layout, DCT closed form, scale, rounding, interleave,
overlap length and output length rules agree with an independent decoder to ±1 LSB, the residue
being explained by the crossfade arithmetic and float rounding. What it does not prove: bit-exact
agreement with `binkw32.dll` itself (never executed here).

---

## 10. Open

1. **Complex FFT operation order.** `FUN_18019fd0` (DCT path) and `FUN_1801a120` (RDFT path) and
   their callees were not transcribed. Their mathematical result is pinned down (§6.1 step 3), but
   bit-exact replication of binkw32's f32 rounding needs their exact operation order.
2. **x87 precision and rounding control at decode time.** The DLL never sets the FPU control word;
   the result depends on the calling thread (the game's main thread via `_BinkDoFrame`, or the
   DLL's async/sound path `0x1800f1a0`). D3D9 devices created without FPU-preserve change x87
   precision; whether this game does, and on which thread decoding runs, was not read.
3. **BinkOpen flags used by the game.** The call at `0x02490c06` is in SecuROM-relocated code with
   arguments pushed through `0x1aaff10`; whether flag `0x4000` (keep the `BinkSetSoundTrack` list)
   is passed was not read (§3.2). Without it only id 0 would be opened.
4. **Writer of the requested ids `0, 2, 3`** at `record + 0x14c..+0x154`: observed in the pc-v10
   memory dump, writer not located (no xref in the pc-v10 decomp export).
5. **RDFT path** (§6.2): untested against output; no shipped file uses it.
6. **Malformed input.** The DLL's behaviour on truncated blocks (stops reading, keeps stale
   coefficients or skips the block) is described but a conforming decoder should hard-fail; not
   exercised by shipped data.
7. **`header[4]` vs frame count** and the frame-offset low bit: cited only; the container spec owns
   their meaning.
