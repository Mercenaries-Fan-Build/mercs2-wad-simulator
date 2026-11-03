# Bink 1 (`BIKi`) video — clean-room decoding specification

Scope: everything a decoder needs to turn the video stream of the Mercenaries 2 PC movies
(`data/Movies/*.bik`, container signature `BIKi`) into Y/V/U(/A) planes, plus what the shipped
`binkw32.dll` does to hand those planes out. Audio decoding is in `SPEC-audio.md`; this document only
says where the audio bytes sit so they can be skipped.

Every fact below was read in this run from the shipped DLL (Ghidra decompile + r2 disassembly) or from
the bytes of the shipped `.bik` files, and is cited. No third-party Bink decoder source was read or
recalled. `ffmpeg`/`ffprobe` were used only as a black box (decoded frames, stream metadata) in §8.

Confidence labels: **PROVEN** (read in this run, citation given; where marked "hash-verified" the rule
also reproduced ffmpeg's decoded plane bit-exactly, §8), **INFERRED** (reasoned from proven facts,
not checked against output), **SPECULATIVE** (a guess; flagged as such and never relied on).

---

## 0. Sources and conventions

| Item | Value |
|---|---|
| DLL | `game-files/Mercenaries 2 World in Flames/binkw32.dll`, sha256 `8e669e9e5c920123a1350f6fa4f6e58b507165b93fd88c546ccaaffae68da829` (matches the `program_sha256` in the export), PE32 i386, compiled 2007-11-06, pdb `C:\devel\projects\bink\build\binkw32.pdb` |
| Decompile | `output/_ghidra_exports/binkw32.functions.jsonl` (320 functions, 0 decompile failures) |
| Disassembly | r2 on the DLL, image base `0x18000000` (same VAs as Ghidra) |
| Movies | `game-files/Mercenaries 2 World in Flames/data/Movies/` — `shell_mattias.bik` (600×720, 0 tracks), `title_logo.bik` (1280×720, 0 tracks), `EA.bik` (1280×720, 4 tracks) |

Sections of the DLL used (r2 `rabin2 -S`): `BINK` code `0x1800a000–0x18021000`, `BINK32` converters
`0x18008000`, read-only data `BINKBSS` `0x18029000–0x1802e000` (file offset `0x1d600`), read-write data
`BINKDATA` `0x1802e000` (file offset `0x22600`). **All constant tables of §4–§5 live in `BINKBSS`
(section is `r--`, so they are static, not built at run time): VA = file offset + `0x1800BA00`.**

Notation: all multi-byte integers little-endian. `u32[x]` = 32-bit LE word at byte offset x.
`>>` on signed values is arithmetic (the DLL uses `sar`). "Plane width/height" always means the
*decode* dimensions of §2.2 (multiples of 8), not the display size.

The call chain for one video frame (all **PROVEN**):

```
_BinkDoFrame@4            0x1800edf0   skips the audio records, then calls
  FUN_18017bb0            0x18017bb0   video packet: plane order, buffer flip (§1.5, §6)
    FUN_18015500          0x18015500   one plane: header, per-row bundles, block loop (§4, §5)
      FUN_18013b60        tree header reader                     (§4.2)
      FUN_18014200        16+1 colour trees                      (§4.2)
      FUN_18014260        block-type / sub-type bundle            (§4.4.1)
      FUN_180147f0        colour bundle (BIKi; FUN_18014520 for BIKf/g/h)  (§4.4.2)
      FUN_18014cd0        pattern bundle                          (§4.4.3)
      FUN_18014ef0        motion x / motion y bundles             (§4.4.4)
      FUN_18015130        intra-DC / inter-DC bundles             (§4.4.5)
      FUN_18014aa0        run bundle                              (§4.4.6)
      FUN_1801efa0        DCT coefficient reader                  (§5.6.1)
      FUN_1801e540        intra IDCT, 8×8 put                     (§5.6.3)
      FUN_1801e880        intra IDCT, 16×16 (scaled) put          (§5.2)
      FUN_1801ec20        inter IDCT, add to motion source        (§5.8)
      FUN_1801fdd0 / FUN_1801f8e0   residue apply / residue reader (§5.5)
```

---

## 1. Container

### 1.1 File header (44 bytes) — PROVEN

`_BinkOpen@8` (`0x1800d670`) reads exactly `0x2c` bytes into a local array (decomp line 106:
`(*local_260[0])(local_260,0,local_45c,0x2c)`), then copies selected fields into the BINK struct.

| Off | Type | Meaning | DLL use (BinkOpen decomp line) |
|---|---|---|---|
| 0 | u32 | signature. Accepted: `0x664b4942` `BIKf`, `0x674b4942` `BIKg`, `0x684b4942` `BIKh`, `0x694b4942` `BIKi` (bytes `42 49 4B 69` = "BIKi") | l.116; else "Not a Bink file." |
| 4 | u32 | file size − 8 | l.123–124 sanity checks only |
| 8 | u32 | number of frames (`BINK+0x08`); 0 → error "The file doesn't contain any compressed frames yet." | l.118, l.165 |
| 12 | u32 | largest frame packet size; must be ≤ field@4 | l.123 |
| 16 | u32 | number of frame-index entries minus 1 (the index holds field+1 words); must be ≥ field@8 and ≤ 1 000 000 | l.122, l.166, l.201 |
| 20 | u32 | width in pixels, ≤ `0x7fff` | l.125, l.132 |
| 24 | u32 | height in pixels, ≤ `0x7fff` | l.125, l.133 |
| 28 | u32 | frame-rate numerator, ≠ 0 (`BINK+0x14`) | l.125, l.168 |
| 32 | u32 | frame-rate denominator, ≠ 0 (`BINK+0x18`) | l.125, l.169 |
| 36 | u32 | video flags (`BINK+0x24`), see below | l.136–158 |
| 40 | u32 | number of audio tracks `n`, ≤ 256 | l.124 |

Further header check (l.124): `field@4 ≥ (n+1) * field@16 * 4`.

Video flag bits used by the DLL (`BinkOpen` l.136–164, `FUN_18017bb0`):

| Bit | Meaning | Citation |
|---|---|---|
| `0x00100000` | an **alpha plane** is present in every video packet (§1.5) | BinkOpen l.137–139 keeps open-flag `0x100000` only if this is set; `0x18017cd9` |
| `0x00020000` | **grayscale**: no chroma planes in the video packet | BinkOpen l.140 copies it into the run-time flags; `0x18017dfe` |
| `0x70000000` | display scaling mode (3 bits). `0x10000000`/`0x20000000`: reported height ×2; `0x30000000`: reported width ×2; `0x40000000`/`0x50000000`: both ×2. The decoder still decodes at field@20 × field@24 (`BINK+0x29c/+0x2a0`) | BinkOpen l.141–158; what the doubled modes look like on screen: **SPECULATIVE**, not needed for decoding |

The DLL also forces run-time flag `0x18000` for `BIKf`/`BIKg` and `0x8000` for `BIKh` (l.159–164);
`BIKi` gets neither. These change the colour bundle reader and the luma size prefix (§1.5, §4.4.2).
Everything below is the `BIKi` path.

Measured (PROVEN, `xxd` of the files):

| File | sig | @4 | @8 frames | @12 largest | @16 | W×H | fps | @36 flags | tracks |
|---|---|---|---|---|---|---|---|---|---|
| shell_mattias.bik (16 488 988 B) | BIKi | 0x00fb9a14 = size−8 | 1099 | 0x7db4 | 1099 | 600×720 | 10000000/333333 | 0 | 0 |
| title_logo.bik (358 352 B) | BIKi | 0x000577c8 = size−8 | 120 | 0xbd74 | 120 | 1280×720 | 10000000/333333 | 0 | 0 |
| EA.bik (15 950 880 B) | BIKi | 0x00f36418 = size−8 | 150 | 0x281c4 | 150 | 1280×720 | 15/1 | 0 | 4 |

ffprobe reports the same dimensions and `r_frame_rate` (10000000/333333, 15/1) and `codec_tag BIKi`
for all three (§8).

### 1.2 Audio track table — PROVEN

Immediately after the header: three arrays of `n` u32 each, then the frame index. BinkOpen reads them
in this order into `BINK+0x2a4`, `+0x2a8`, `+0x2ac`, `+0x14c` (l.225–228), which are what
`_BinkGetTrackMaxSize@8` (`0x18010430`, reads `+0x2a4`), `_BinkGetTrackType@8` (`0x18010410`, `+0x2a8`)
and `_BinkGetTrackID@8` (`0x18010450`, `+0x2ac`) return.

| Offset | Array | Meaning |
|---|---|---|
| `44` | `max_size[n]` | per-track maximum decoded buffer size |
| `44 + 4n` | `flags[n]` | low 16 bits sample rate; bit 29 `0x20000000` stereo (`channels = (f>>29&1)+1`, l.378); bit 30 `0x40000000` 16-bit (`(f>>0x1b&8)+8` bits, l.381); bit 28 `0x10000000` and bit 31 select the audio transform (l.374, l.431) — see SPEC-audio.md |
| `44 + 8n` | `track_id[n]` | id matched by `BinkSetSoundTrack` (l.353) |
| `44 + 12n` | frame index | §1.3 |

EA.bik (PROVEN, bytes at 0x2c..0x5b): max `0x23a00 0x11d00 0x11d00 0x23a00`, flags
`0x7000bb80 0x5000bb80 0x5000bb80 0x7000bb80` (48000 Hz; tracks 0 and 3 stereo), ids `0 1 2 3`.
ffprobe: 4 `binkaudio_dct` streams, 48000 Hz, channels 2,1,1,2 — agrees.

### 1.3 Frame index — PROVEN

`field@16 + 1` u32 words (BinkOpen l.201/l.228 allocate and read `count*4+4` bytes). Word `i` describes
frame `i` (0-based); `FUN_1800d1d0` (`0x1800d1d0`, l.34–38) computes, for frame `i`:

```
offset   = index[i]   & ~1          // absolute file offset of the frame packet
keyframe = index[i]   &  1
size     = (index[i+1] & ~1) - offset
```

`_BinkGetKeyFrame@12` (`0x1800f660`) tests the same bit 0. `FUN_1800d350` rejects the file ("This file
has bad frame size data.") unless the raw words are strictly increasing (`index[k+1] > index[k]`).

shell_mattias: `index[0] = 0x115d` (offset `0x115c` = 44 + 4·1100, keyframe), `index[1] = 0x871c`
(not a keyframe), `index[2] = 0xd0b0`. EA: `index[0] = 0x2b9` → offset `0x2b8` = 44 + 48 + 4·151.

### 1.4 Frame packet: audio records, then video — PROVEN

`_BinkDoFrame@4`, `0x1800ee78`–`0x1800f03a`: starting at the packet, for each track `t = 0 .. n-1`
(count `BINK+0xe0` = header@40):

```
len : u32                 // 0 = no audio for this track in this frame
data: len bytes           // if len != 0 and the track is opened: first u32 of data = decoded byte count
next record at (address of len) + 4 + len
```

(`mov edx,[ebx]; add ebx,4 … add ebx,[esp+0x4c]` at `0x1800ee88/0x1800f029`.) The video packet is
everything from the end of the last record to the end of the frame packet (`0x1800f117`–`0x1800f14c`:
`FUN_18017bb0(edx = video start, end = packet start + size)`). With `n = 0` the video packet starts at
the frame offset (shell_mattias, title_logo).

### 1.5 Video packet layout — PROVEN (hash-verified for Y/V/U)

`FUN_18017bb0` (`0x18017bb0`, disassembly `0x18017cd9`–`0x18017edf`), with `p` = video start:

1. **Alpha** (only if header flag `0x100000`): `u32 size` at `p`, alpha plane bitstream at `p+4`, next
   plane at `p + size` (the size counts its own 4 bytes: `mov eax,[edx]; add eax,edx` at `0x18017d51`).
   The size is skipped even when the alpha plane is not being decoded (run-time flag `0x100000` clear).
2. **Luma Y**: for `BIKi` (run-time flag `0x8000` clear) a `u32 size` (again counting itself), the Y
   bitstream at `+4`, the next plane at `(address of size) + size` (`0x18017dec`–`0x18017dfa`).
3. Unless grayscale (`0x20000`): **chroma plane A**, then **chroma plane B**. There is no size prefix;
   plane B starts at the address returned by the plane-A decode, which is the bit reader's word pointer
   (`mov eax,[ebp-0x1fc]` at `0x18017b50`): **the first 32-bit word after the last word from which any
   bit of plane A was consumed** = `startA + 4*ceil(bitsA/32)`.

Plane A is decoded into buffer slot `+0x28` and plane B into slot `+0x34` (§6). Plane A is the **V
(Cr)** plane and plane B the **U (Cb)** plane: hash-verified — the plane-A walk reproduces ffmpeg's
`yuv420p` V plane and plane B its U plane (§8); also consistent with the RGB converter's use of A as
the red-difference input (§7.2).

Measured on shell_mattias frame 0 (offset `0x115c`, size `0x75c0`): Y size word `0x70a8`, Y bits at
`0x1160`, plane A at `0x8204`, plane B at `0x8354`, and plane B's last word ends exactly at `0x871c`,
the start of frame 1 (§8). Frame 1: Y size `0x4738`, A at `0xce54`, B at `0xcec4`, ends at `0xd0b0`
= `index[2]`.

---

## 2. Video frame decode, top down

### 2.1 Order — PROVEN

Per frame (`FUN_18017bb0`): optional alpha plane, Y plane, plane A (V), plane B (U), each decoded by the
same function `FUN_18015500` with that plane's dimensions and buffers. Each plane is independent: its own
trees, bundles and bit reader, starting at its own byte address (§1.5).

### 2.2 Plane dimensions — PROVEN

`_BinkGetFrameBuffersInfo@8` (`0x1800d590`), with header width `w`, height `h` (`BINK+0x29c/+0x2a0`):

| Plane | Decode width | Decode height | Pitch of the DLL's own buffer (BinkOpen l.270–289) |
|---|---|---|---|
| Y, alpha | `(w + 7) & ~7` | `(h + 7) & ~7` | = decode width |
| V, U | `(((w + 1) >> 1) + 7) & ~7` | `(((h + 1) >> 1) + 7) & ~7` | `(decode width + 15) & ~15` |

Chroma is subsampled 2×2. Example: 600×720 → Y 600×720 (pitch 600), chroma 304×360 (pitch 304; ffmpeg
crops chroma to 300×360). 1280×720 → Y 1280×720, chroma 640×360.

### 2.3 Blocks and scan order — PROVEN

`FUN_18015500`: the plane is walked in **8-pixel rows of blocks, top to bottom; within a row, left to
right**. The outer loop steps `y += 8` while `y < height` (`0x18015e41`–`0x18015e5d`); the inner loop
steps `x` by 8 (or 16 for a scaled block) until the row is used up (`0x18015de6`–`0x18015e0d`). Every
block is 8×8, except **scaled** blocks (§5.2), which write 16×16. Bundles are refilled at the start of
each block row (§4.3).

---

## 3. Bitstream reader — PROVEN (hash-verified)

State: `{ buf: u32, ptr: *u32, count: u32 }` (`FUN_18015500` keeps it at `ebp-0x200/-0x1fc/-0x1f8`).
Initial: `buf = 0`, `count = 0`, `ptr = plane start` (`0x180155a3`–`0x180155c5`).

Bits are consumed **LSB-first from 32-bit little-endian words**. Reading `n` bits (`1 ≤ n ≤ 32`), as
written out in every reader (e.g. `FUN_18014260` at `0x18014289`–`0x180142e1`):

```
if count < n:                      // need the next word
    w     = *ptr++
    value = (buf | (w << count)) & ((1<<n)-1)
    buf   = w >> (n - count)
    count = count + 32 - n
else:
    value = buf & ((1<<n)-1)
    buf   = buf >> n
    count = count - n
```

This is exactly "bit position `P` advances by `n`; value = bits `P .. P+n-1` of the byte stream, bit 0 of
each byte first". There is no byte alignment anywhere inside a plane. Huffman lookups *peek* `maxlen`
bits (combining with `*ptr` without advancing when `count < maxlen`) and then consume only the code's
length (§4.2.3). The plane's end address is the reader's `ptr` (§1.5).

`get1()` below = read 1 bit; `getbits(n)` = read n bits.

---

## 4. Plane header and bundles

### 4.1 Plane header — PROVEN (hash-verified)

At the start of each plane, `FUN_18015500` reads 23 tree descriptors in this order
(`0x18015b04`–`0x18015bbe`):

| # | Tree for | Reader |
|---|---|---|
| 1 | block types | `FUN_18013b60` |
| 2 | scaled-block sub-types | `FUN_18013b60` |
| 3–18 | colour high nibble, context 0..15 (in that order) | `FUN_18014200` (16 × `FUN_18013b60`) |
| 19 | colour low nibble | `FUN_18014200` |
| 20 | pattern nibbles | `FUN_18013b60` |
| 21 | motion x | `FUN_18013b60` |
| 22 | motion y | `FUN_18013b60` |
| 23 | run lengths | `FUN_18013b60` |

`FUN_18014200` also sets the colour context `prev_hi = 0` (`*(param_3+0x180) = 0`).
After the trees comes the first block row (§4.3).

### 4.2 Trees (Huffman tables + symbol map)

A tree = one of 16 fixed code tables (chosen by 4 bits) + a 16-entry **symbol map** read from the stream.
Decoding returns `map[code_index]`.

#### 4.2.1 Tree descriptor — PROVEN (hash-verified: `t = 0`, the explicit list, and the shuffle with `d = 3`, which occurs in frame 0 plane B's colour-low tree; `d = 0, 1, 2` read from code only)

`FUN_18013b60` (`0x18013b60`):

```
t = getbits(4)
table  = TREE_BYTES + TREE_OFF[t]      // §4.2.2
maxlen = TREE_MAXLEN[t]
if t == 0:
    map[i] = i for i in 0..15          // no further bits
else if get1() == 1:                   // explicit list
    k = getbits(3)
    for i in 0..k: map[i] = getbits(4)            // k+1 symbols
    fill map[k+1..15] with the symbols NOT listed, in ascending order
else:                                  // shuffle
    d = getbits(2)
    for i in 0,2,4,..,14:              // 8 pairs
        if get1() == 0: P[i]=i,   P[i+1]=i+1
        else:           P[i]=i+1, P[i+1]=i
    if d == 0: map = P
    if d == 1: map[0..3]=merge(P[0..1],P[2..3]); map[4..7]=merge(P[4..5],P[6..7]);
               map[8..11]=merge(P[8..9],P[10..11]); map[12..15]=merge(P[12..13],P[14..15])
    if d >= 2: Q[0..3]=merge(P[0..1],P[2..3]); Q[4..7]=merge(P[4..5],P[6..7]);
               Q[8..11]=merge(P[8..9],P[10..11]); Q[12..15]=merge(P[12..13],P[14..15])
      if d == 2: map[0..7]=merge(Q[0..3],Q[4..7]); map[8..15]=merge(Q[8..11],Q[12..15])
      if d == 3: O1=merge(Q[0..3],Q[4..7]); O2=merge(Q[8..11],Q[12..15]); map=merge(O1,O2)
```

`merge(A, B)` (`FUN_18013ad0`, `0x18013ad0`), lists of equal length `m`:

```
out = []
loop:
    if get1() == 0: out.push(A.pop_front())  else: out.push(B.pop_front())
    if A empty: out += B; return out          // A is tested first
    if B empty: out += A; return out
```

The DLL peeks 3 / 7 / 15 bits for the `m = 2 / 4 / 8` merges and then advances by the number of
decisions actually made (the function's return value); this is identical to reading one bit per decision
as written above.

For the explicit list the fill loop is `do { if (unused & 1) map[++k] = sym; sym++; unused >>= 1 } while
unused != 0` with a `k == 15` stop (decomp lines near `LAB_180141db`): ascending order, PROVEN.

#### 4.2.2 Code tables — PROVEN (byte-exact)

`TREE_MAXLEN[16]` at VA `0x1802da84` (file `0x22084`):
`04 05 05 05 05 05 05 06 06 06 06 06 06 07 07 07`

`TREE_OFF[16]` (u32) at VA `0x1802df20` (file `0x22520`), byte offsets into `TREE_BYTES`:
`0x000 0x010 0x030 0x050 0x070 0x090 0x0b0 0x0d0 0x110 0x150 0x190 0x1d0 0x210 0x250 0x2d0 0x350`
(table `t` has `2^TREE_MAXLEN[t]` entries).

`TREE_BYTES` at VA `0x1802db40` (file `0x22140`), 976 bytes. Each byte: **high nibble = code length,
low nibble = code index** (index into the tree's symbol map). Dump (file offset; VA = offset +
`0x1800BA00`):

```
0022140  40 41 42 43 44 45 46 47 48 49 4a 4b 4c 4d 4e 4f   tree 0  (16)
0022150  10 41 10 52 10 53 10 54 10 55 10 56 10 57 10 58   tree 1  (32)
0022160  10 41 10 59 10 5a 10 5b 10 5c 10 5d 10 5e 10 5f
0022170  20 42 21 58 20 54 21 5c 20 43 21 5a 20 56 21 5e   tree 2  (32)
0022180  20 42 21 59 20 55 21 5d 20 43 21 5b 20 57 21 5f
0022190  20 43 31 58 20 45 32 5c 20 44 31 5a 20 56 32 5e   tree 3  (32)
00221a0  20 43 31 59 20 45 32 5d 20 44 31 5b 20 57 32 5f
00221b0  30 44 32 58 31 46 33 5c 30 45 32 5a 31 47 33 5e   tree 4  (32)
00221c0  30 44 32 59 31 46 33 5d 30 45 32 5b 31 47 33 5f
00221d0  30 46 42 4a 31 48 44 5c 30 47 43 4b 31 49 45 5e   tree 5  (32)
00221e0  30 46 42 4a 31 48 44 5d 30 47 43 4b 31 49 45 5f
00221f0  20 45 41 49 20 47 43 5c 20 46 42 5a 20 48 44 5e   tree 6  (32)
0022200  20 45 41 49 20 47 43 5d 20 46 42 5b 20 48 44 5f
0022210  10 31 10 53 10 32 10 68 10 31 10 55 10 32 10 6c   tree 7  (64)
0022220  10 31 10 54 10 32 10 6a 10 31 10 66 10 32 10 6e
0022230  10 31 10 53 10 32 10 69 10 31 10 55 10 32 10 6d
0022240  10 31 10 54 10 32 10 6b 10 31 10 67 10 32 10 6f
0022250  10 21 10 52 10 21 10 68 10 21 10 64 10 21 10 6c   tree 8  (64)
0022260  10 21 10 53 10 21 10 6a 10 21 10 66 10 21 10 6e
0022270  10 21 10 52 10 21 10 69 10 21 10 65 10 21 10 6d
0022280  10 21 10 53 10 21 10 6b 10 21 10 67 10 21 10 6f
0022290  10 31 10 54 10 42 10 68 10 31 10 56 10 43 10 6c   tree 9  (64)
00222a0  10 31 10 55 10 42 10 6a 10 31 10 57 10 43 10 6e
00222b0  10 31 10 54 10 42 10 69 10 31 10 56 10 43 10 6d
00222c0  10 31 10 55 10 42 10 6b 10 31 10 57 10 43 10 6f
00222d0  20 32 21 55 20 43 21 59 20 32 21 57 20 44 21 6c   tree 10 (64)
00222e0  20 32 21 56 20 43 21 6a 20 32 21 58 20 44 21 6e
00222f0  20 32 21 55 20 43 21 59 20 32 21 57 20 44 21 6d
0022300  20 32 21 56 20 43 21 6b 20 32 21 58 20 44 21 6f
0022310  10 41 10 55 10 43 10 59 10 42 10 57 10 44 10 6c   tree 11 (64)
0022320  10 41 10 56 10 43 10 6a 10 42 10 58 10 44 10 6e
0022330  10 41 10 55 10 43 10 59 10 42 10 57 10 44 10 6d
0022340  10 41 10 56 10 43 10 6b 10 42 10 58 10 44 10 6f
0022350  20 22 21 53 20 22 21 68 20 22 21 55 20 22 21 6c   tree 12 (64)
0022360  20 22 21 54 20 22 21 6a 20 22 21 66 20 22 21 6e
0022370  20 22 21 53 20 22 21 69 20 22 21 55 20 22 21 6d
0022380  20 22 21 54 20 22 21 6b 20 22 21 67 20 22 21 6f
0022390  10 31 10 33 10 32 10 64 10 31 10 33 10 32 10 78   tree 13 (128)
00223a0  10 31 10 33 10 32 10 66 10 31 10 33 10 32 10 7c
00223b0  10 31 10 33 10 32 10 65 10 31 10 33 10 32 10 7a
00223c0  10 31 10 33 10 32 10 67 10 31 10 33 10 32 10 7e
00223d0  10 31 10 33 10 32 10 64 10 31 10 33 10 32 10 79
00223e0  10 31 10 33 10 32 10 66 10 31 10 33 10 32 10 7d
00223f0  10 31 10 33 10 32 10 65 10 31 10 33 10 32 10 7b
0022400  10 31 10 33 10 32 10 67 10 31 10 33 10 32 10 7f
0022410  10 31 10 33 10 32 10 54 10 31 10 33 10 32 10 78   tree 14 (128)
0022420  10 31 10 33 10 32 10 65 10 31 10 33 10 32 10 7c
0022430  10 31 10 33 10 32 10 54 10 31 10 33 10 32 10 7a
0022440  10 31 10 33 10 32 10 76 10 31 10 33 10 32 10 7e
0022450  10 31 10 33 10 32 10 54 10 31 10 33 10 32 10 79
0022460  10 31 10 33 10 32 10 65 10 31 10 33 10 32 10 7d
0022470  10 31 10 33 10 32 10 54 10 31 10 33 10 32 10 7b
0022480  10 31 10 33 10 32 10 77 10 31 10 33 10 32 10 7f
0022490  20 32 21 34 20 33 21 65 20 32 21 34 20 33 21 69   tree 15 (128)
00224a0  20 32 21 34 20 33 21 67 20 32 21 34 20 33 21 7c
00224b0  20 32 21 34 20 33 21 66 20 32 21 34 20 33 21 7a
00224c0  20 32 21 34 20 33 21 68 20 32 21 34 20 33 21 7e
00224d0  20 32 21 34 20 33 21 65 20 32 21 34 20 33 21 69
00224e0  20 32 21 34 20 33 21 67 20 32 21 34 20 33 21 7d
00224f0  20 32 21 34 20 33 21 66 20 32 21 34 20 33 21 7b
0022500  20 32 21 34 20 33 21 68 20 32 21 34 20 33 21 7f
```

#### 4.2.3 Decoding one symbol — PROVEN (hash-verified)

(`FUN_18014260` `0x18014357`–`0x180143d9`, same pattern in every bundle reader):

```
v = peek(maxlen)                // next maxlen bits, LSB-first, NOT consumed
e = table[v]
consume(e >> 4)                 // code length
return map[e & 15]
```

### 4.3 Bundles: per-row refill rule — PROVEN (hash-verified)

A **bundle** is a buffer of pre-decoded values that blocks consume in order. Each plane has 9 bundles.
At the start of every block row, before the row's blocks, `FUN_18015500` calls the readers in this order
(`0x18015be0`–`0x18015cc4`):

| # | Bundle | Element | Length-field bits `L` | Max count | Reader |
|---|---|---|---|---|---|
| 1 | block types | u8 0..15 | `bitlen(bw + 511)` | `(bw + 0x20f) & ~15` | FUN_18014260 |
| 2 | sub-types | u8 0..15 | `bitlen((W>>4) + 511)` | `((W>>4) + 0x20f) & ~15` | FUN_18014260 |
| 3 | colours | u8 | `bitlen(bw*64 + 511)` | `(bw*64 + 0x20f) & ~15` | FUN_180147f0 |
| 4 | pattern | u8 | `bitlen(bw*8 + 511)` | `(bw*8 + 0x20f) & ~15` | FUN_18014cd0 |
| 5 | motion x | s8 | `bitlen(bw + 511)` | `(bw + 0x20f) & ~15` | FUN_18014ef0 |
| 6 | motion y | s8 | `bitlen(bw + 511)` | `(bw + 0x20f) & ~15` | FUN_18014ef0 |
| 7 | intra DC | s16 | `bitlen(bw + 511)` | `((bw*2 + 0x40f) & ~15) / 2` | FUN_18015130 |
| 8 | inter DC | s16 | `bitlen(bw + 511)` | same | FUN_18015130 |
| 9 | run lengths | u8 0..15 | `bitlen(bw*48 + 511)` | `(bw*48 + 0x20f) & ~15` | FUN_18014aa0 |

`W` = plane decode width, `bw = W >> 3`. Sources: the `bitlen` arguments and maxima are computed at
`0x180155e7`–`0x18015ab6` (Ghidra lines 175–346), motion via `FUN_180139f0` with `param_4 = 1`.

`bitlen(x)` = number of significant bits of `x` (x ≥ 511 always): the DLL computes 9 for `x < 0x200`,
10 below `0x400`, 11 below `0x800`, 12 below `0x1000`, 13 below `0x2000`, 14 below `0x4000`, else **15**
(`0x18015609`–`0x18015650`; the table branch `x ≤ 0x80` is unreachable). Examples: Y of width 600 → block
types 10 bits, colours 13, pattern 11, runs 13; chroma width 304 → 10, 12, 10, 12.

After the 9 readers, the motion-x read position is **forced to the motion-y read index**:
`x.cur = y.cur − y.buf + x.buf` (`0x18015c6c`–`0x18015c82`), every row.

Refill rule, identical in all nine readers (e.g. `0x18014268`–`0x180142f2`, `0x180144db`):

```
if bundle.cur != bundle.end: return            // still has data: no bits read this row
count = getbits(L)
if count == 0 or count > max:
    bundle.cur = bundle.buf + 4; bundle.end = bundle.buf    // "terminated"
    return                                       // cur != end forever: never re-read in this plane
bundle.cur = bundle.buf; bundle.end = bundle.buf + count
decode `count` elements (§4.4)
```

Consequences (PROVEN by code, observed in §8): a count of 0 ends that bundle **for the rest of the plane**
(keyframes write 0 for motion, inter-DC etc. at row 0). A bundle is refilled only when it was exactly used
up. In the block loop, a block whose type cannot be fetched because the block-type bundle is used up or
terminated is skipped (advance 8, nothing written; `0x18015cf4`–`0x18015d00`). Other bundles read past
their end fall back to reading their buffer at `+4` (e.g. `0x18016fe4`) — garbage that a valid stream
never reaches.

### 4.4 Bundle element readers

#### 4.4.1 Block types and sub-types — PROVEN (hash-verified)

`FUN_18014260` (`0x18014260`):

```
if get1() == 1:                         // fill
    v = getbits(4)                       // RAW value, not mapped through the tree
    all `count` elements = v
else:
    last = 0
    while elements < count:
        s = decode(tree)                 // §4.2.3, mapped value
        if s < 12: emit s; last = s
        else:      r = RUN_LEN[s]; if r > remaining: stop (malformed); emit `last` r times
```

`RUN_LEN[12..15]` at VA `0x1802da94` (= `0x1802da88 + 12`, file `0x22094`): `04 08 0c 20` → 4, 8, 12, 32.
(The aligned store loop writes 4 bytes at a time, `0x18014423`, so these must stay multiples of 4.)

#### 4.4.2 Colours (BIKi) — PROVEN (hash-verified)

`FUN_180147f0` (used when run-time flag `0x8000` is clear, `0x180155d1`–`0x180155dd`):

```
fill = get1()
n = fill ? 1 : count
repeat n times:
    hi = decode(colour_hi_tree[prev_hi])  // tree chosen by the previous colour's high nibble
    prev_hi = hi
    lo = decode(colour_lo_tree)
    emit (hi << 4) | lo                   // unsigned byte
if fill: all `count` elements = that one colour
```

`prev_hi` starts at 0 for each plane and carries across rows (saved to `param_3+0x180`). Note the order:
high nibble first, then low.

#### 4.4.3 Pattern — PROVEN (hash-verified)

`FUN_18014cd0`: no fill bit. Each element: `lo = decode(pattern_tree)`, then `hi = decode(pattern_tree)`,
emit `(hi << 4) | lo`.

#### 4.4.4 Motion x / motion y — PROVEN (hash-verified for the tree path; fill path read from code)

`FUN_18014ef0`:

```
if get1() == 1:                          // fill
    v = getbits(4)                        // raw
    if v != 0 and get1() == 1: v = -v
    all elements = v
else each element:
    v = decode(tree)                      // 0..15
    if v != 0 and get1() == 1: v = -v
    emit v                                // signed byte, -15..15
```

#### 4.4.5 Intra DC / inter DC — PROVEN (both hash-verified)

`FUN_18015130`; parameters set in `FUN_18015500`: intra `bits = 11, signed = 0` (`[ebp-0x100]=0xb,
[ebp-0xfc]=0`), inter `bits = 11, signed = 0x400` (`[ebp-0x1d0]=0xb, [ebp-0x1cc]=0x400`):

```
if not signed: v = getbits(11)                         // intra: 0..2047
else:          v = getbits(10); if v != 0 and get1(): v = -v
emit v
remaining = count - 1
while remaining > 0:
    g = min(8, remaining)
    w = getbits(4)
    repeat g times:
        if w != 0:
            d = getbits(w); if d != 0 and get1(): d = -d
            v += d                                     // 32-bit accumulator
        emit (int16)v                                  // w == 0 repeats v
    remaining -= g
```

#### 4.4.6 Run lengths — PROVEN (tree path hash-verified)

`FUN_18014aa0`: `if get1()==1: all = getbits(4)` (raw) `else` each element `= decode(run_tree)`.
Values 0..15; a run block uses `value + 1`.

---

## 5. Block reconstruction

### 5.0 Block loop — PROVEN (hash-verified)

`FUN_18015500` `0x18015ce0`–`0x18015e0d`. `dst` = current block in the frame being decoded, `ref` = the
same position in the previous frame (§6), `P` = pitch, `x,y` = block's top-left pixel, `W,H` = plane decode
size. Per block: `t = next block type`; jump table at VA `0x18017b68` (10 entries; `t > 9` → advance 8,
write nothing):

| t | Name | Target | Bits read from the stream (in order) | Bundles consumed |
|---|---|---|---|---|
| 0 | skip | `0x18015d24` | none | none |
| 1 | scaled | `0x18015e76` | see §5.2 | sub-type (+ per sub-type) |
| 2 | motion | `0x18016fc4` | none | x, y |
| 3 | run | `0x180170d2` | 4 + 1 per run | runs, colours |
| 4 | residue | `0x1801733c` | 7 + residue (only if motion in range) | x, y |
| 5 | intra | `0x18017429` | coefficients, 4 | intra DC |
| 6 | fill | `0x180174f0` | none | colours (1) |
| 7 | inter | `0x1801757f` | coefficients, 4 (only if motion in range) | x, y, inter DC |
| 8 | pattern | `0x180176a7` | none | colours (2), pattern (8) |
| 9 | raw | `0x18017a90` | none | colours (64) |

Each non-scaled block advances `x` by 8.

### 5.1 Skip (0) — PROVEN (hash-verified, frame 1)

Copy the 8×8 block at the same position from `ref` to `dst`. No bounds check.

### 5.2 Scaled (1) — PROVEN (hash-verified for sub-types 3, 5, 6, 8)

`0x18015e76`:

```
if W - x < 16:                 advance 8; done          // type consumed, nothing else
if (y & 8) != 0 or sub-type bundle used up:  advance 16; done   // odd block row: already written
s = next sub-type
if s not in {3,5,6,8,9}:       advance 16; done          // jump table VA 0x18017b90, s-3 > 6 → 0x18016f80
if H - y < 16:                 advance 8; done           // sub-type consumed (checked inside each case)
decode an 8×8 block of kind s into a temporary, write it 2× in both directions over the 16×16 area
advance 16
```

So a scaled block is coded once, on the **even** block row (`y & 8 == 0`); the encoder also puts a type-1
entry at the same x on the following odd row, where it only advances 16.

Sub-kinds (each writes `dst[y+2r+i][x+2c+j] = B[r][c]`, `i,j ∈ {0,1}`):

| s | Kind | How `B` is produced |
|---|---|---|
| 3 | run | exactly §5.4 |
| 5 | intra | exactly §5.6 (DC from intra-DC bundle, coefficients, 4-bit q, IDCT) then pixel-doubled by `FUN_1801e880` (`0x1801e880`, same transform as `FUN_1801e540`; doubling seen in its store code `uVar | uVar<<8` and two-row writes) |
| 6 | fill | one colour |
| 8 | pattern | §5.9 with the 2-bit-per-pixel-pair masks at VA `0x1802df10` (`00000000 0000ffff ffff0000 ffffffff`) and `0x1802dfe0` (`ffffffff ffff0000 0000ffff 00000000`): pattern bit k of row r → pixels `2k, 2k+1` of rows `2r, 2r+1`; bit 1 → second colour |
| 9 | raw | 64 colours, row-major (`0x18016b58`; doubled with `b0 b0 b1 b1` stores) |

### 5.3 Motion (2) — PROVEN (hash-verified, frame 1)

`0x18016fc4`: `mx = next x`, `my = next y` (signed bytes),
`src = ref_plane_base + (y + my)*P + (x + mx)` as a **linear address**. Valid iff
`src >= ref_plane_base` and `src <= ref_plane_base + (H − 8)*P + W − 8` (`0x1801701d`–`0x1801703b`,
bound computed at `0x1801554c`–`0x18015584`). If valid copy 8×8 from `src` to `dst`; otherwise **write
nothing** (the destination keeps whatever that buffer held). The check is linear, not per axis: a
negative `x+mx` in column 0 reads the end of the previous row and is accepted. Exact edge behaviour
therefore depends on `P`; use the DLL's own pitch (§2.2).

### 5.4 Run (3) — PROVEN (hash-verified, scaled form)

`0x180170d2` (Ghidra lines 993–1081):

```
p = getbits(4); scan = SCAN[p]            // 64-byte permutation, §5.4.1
i = 0; left = 64
loop:
    r = next run + 1
    if left < r: break                       // malformed
    left -= r
    if get1() == 0: repeat r: B[scan[i++]] = next colour
    else:           c = next colour; repeat r: B[scan[i++]] = c
    if left <= 1: break
if i == 63: B[scan[63]] = next colour
```

`B` is the 8×8 block in row-major order (`B[r*8+c]`), then copied to `dst`.

#### 5.4.1 Run scan patterns — PROVEN (byte-exact)

`SCAN[16][64]` at VA `0x18029600` (file `0x1dc00`); each byte is a row-major pixel index `r*8+c`:

```
001dc00  00 08 10 18 20 28 30 38 39 31 29 21 19 11 09 01   pattern 0
001dc10  02 0a 12 1a 22 2a 32 3a 3b 33 2b 23 1b 13 0b 03
001dc20  04 0c 14 1c 24 2c 34 3c 3d 35 2d 25 1d 15 0d 05
001dc30  06 0e 16 1e 26 2e 36 3e 3f 37 2f 27 1f 17 0f 07
001dc40  3b 3a 39 38 30 31 32 33 2b 2a 29 28 20 21 22 23   pattern 1
001dc50  1b 1a 19 18 10 11 12 13 0b 0a 09 08 00 01 02 03
001dc60  04 05 06 07 0f 0e 0d 0c 14 15 16 17 1f 1e 1d 1c
001dc70  24 25 26 27 2f 2e 2d 2c 34 35 36 37 3f 3e 3d 3c
001dc80  19 11 12 1a 1b 13 0b 03 02 0a 09 01 00 08 10 18   pattern 2
001dc90  20 28 30 38 39 31 29 2a 32 3a 3b 33 2b 23 22 21
001dca0  1d 15 16 1e 1f 17 0f 07 06 0e 0d 05 04 0c 14 1c
001dcb0  24 2c 34 3c 3d 35 2d 2e 36 3e 3f 37 2f 27 26 25
001dcc0  03 0b 02 0a 01 09 00 08 10 18 11 19 12 1a 13 1b   pattern 3
001dcd0  23 2b 22 2a 21 29 20 28 30 38 31 39 32 3a 33 3b
001dce0  3c 34 3d 35 3e 36 3f 37 2f 27 2e 26 2d 25 2c 24
001dcf0  1c 14 1d 15 1e 16 1f 17 0f 07 0e 06 0d 05 0c 04
001dd00  18 19 10 11 08 09 00 01 02 03 0a 0b 12 13 1a 1b   pattern 4
001dd10  1c 1d 14 15 0c 0d 04 05 06 07 0e 0f 16 17 1e 1f
001dd20  27 26 2f 2e 37 36 3f 3e 3d 3c 35 34 2d 2c 25 24
001dd30  23 22 2b 2a 33 32 3b 3a 39 38 31 30 29 28 21 20
001dd40  00 01 02 03 08 09 0a 0b 10 11 12 13 18 19 1a 1b   pattern 5
001dd50  20 21 22 23 28 29 2a 2b 30 31 32 33 38 39 3a 3b
001dd60  04 05 06 07 0c 0d 0e 0f 14 15 16 17 1c 1d 1e 1f
001dd70  24 25 26 27 2c 2d 2e 2f 34 35 36 37 3c 3d 3e 3f
001dd80  06 07 0f 0e 0d 05 0c 04 03 0b 02 0a 09 01 00 08   pattern 6
001dd90  10 18 11 19 12 1a 13 1b 14 1c 15 1d 16 1e 17 1f
001dda0  27 2f 26 2e 25 2d 24 2c 23 2b 22 2a 21 29 20 28
001ddb0  31 30 38 39 3a 32 3b 33 3c 34 3d 35 36 37 3f 3e
001ddc0  00 01 02 03 04 05 06 07 0f 0e 0d 0c 0b 0a 09 08   pattern 7
001ddd0  10 11 12 13 14 15 16 17 1f 1e 1d 1c 1b 1a 19 18
001dde0  20 21 22 23 24 25 26 27 2f 2e 2d 2c 2b 2a 29 28
001ddf0  30 31 32 33 34 35 36 37 3f 3e 3d 3c 3b 3a 39 38
001de00  00 08 09 01 02 03 0b 0a 12 13 1b 1a 19 11 10 18   pattern 8
001de10  20 28 29 21 22 23 2b 2a 32 31 30 38 39 3a 3b 33
001de20  34 3c 3d 3e 3f 37 36 35 2d 2c 24 25 26 2e 2f 27
001de30  1f 17 16 1e 1d 1c 14 15 0d 0c 04 05 06 0e 0f 07
001de40  18 19 10 11 08 09 00 01 02 03 0a 0b 12 13 1a 1b   pattern 9
001de50  1c 1d 14 15 0c 0d 04 05 06 07 0e 0f 16 17 1e 1f
001de60  26 27 2e 2f 36 37 3e 3f 3c 3d 34 35 2c 2d 24 25
001de70  22 23 2a 2b 32 33 3a 3b 38 39 30 31 28 29 20 21
001de80  00 08 01 09 02 0a 03 0b 13 1b 12 1a 11 19 10 18   pattern 10
001de90  20 28 21 29 22 2a 23 2b 33 3b 32 3a 31 39 30 38
001dea0  3c 34 3d 35 3e 36 3f 37 2f 27 2e 26 2d 25 2c 24
001deb0  1f 17 1e 16 1d 15 1c 14 0c 04 0d 05 0e 06 0f 07
001dec0  00 08 10 18 19 1a 1b 13 0b 03 02 01 09 11 12 0a   pattern 11
001ded0  04 0c 14 1c 1d 1e 1f 17 0f 07 06 05 0d 15 16 0e
001dee0  24 2c 34 3c 3d 3e 3f 37 2f 27 26 25 2d 35 36 2e
001def0  20 28 30 38 39 3a 3b 33 2b 23 22 21 29 31 32 2a
001df00  00 08 09 01 02 03 0b 0a 13 1b 1a 12 11 10 18 19   pattern 12
001df10  21 20 28 29 2a 22 23 2b 33 3b 3a 32 31 39 38 30
001df20  34 3c 3d 35 36 3e 3f 37 2f 27 26 2e 2d 2c 24 25
001df30  1d 1c 14 15 16 1e 1f 17 0e 0f 07 06 05 0d 0c 04
001df40  18 10 08 00 01 02 03 0b 13 1b 1a 19 11 0a 09 12   pattern 13
001df50  1c 14 0c 04 05 06 07 0f 17 1f 1e 1d 15 0e 0d 16
001df60  3c 34 2c 24 25 26 27 2f 37 3f 3e 3d 35 2e 2d 36
001df70  38 30 28 20 21 22 23 2b 33 3b 3a 39 31 2a 29 32
001df80  00 08 09 01 02 0a 12 11 10 18 19 1a 1b 13 0b 03   pattern 14
001df90  07 06 0e 0f 17 16 15 0d 05 04 0c 14 1c 1d 1e 1f
001dfa0  3f 3e 36 37 2f 2e 2d 35 3d 3c 34 2c 24 25 26 27
001dfb0  38 30 31 39 3a 32 2a 29 28 20 21 22 23 2b 33 3b
001dfc0  00 01 08 09 10 11 18 19 20 21 28 29 30 31 38 39   pattern 15
001dfd0  3a 3b 32 33 2a 2b 22 23 1a 1b 12 13 0a 0b 02 03
001dfe0  04 05 0c 0d 14 15 1c 1d 24 25 2c 2d 34 35 3c 3d
001dff0  3e 3f 36 37 2e 2f 26 27 1e 1f 16 17 0e 0f 06 07
```

### 5.5 Residue (4) — PROVEN by code; output path not yet exercised by a verified test (§8, §9)

`0x1801733c`: motion exactly as §5.3 (same bundles, same linear bounds check). **Only if in range**:
`m = getbits(7)` (`0x180173b3`–`0x18017401`), then `FUN_1801fdd0(dst=EDI, src=ESI)`:

1. `R[64]` signed bytes, zeroed; read by `FUN_1801f8e0` (`0x1801f8e0`):

```
b = getbits(3); v = 1 << b; passes = b + 1
list  = [ (4,G20), (24,G20), (44,G20), (0,G4) ]      // bytes 0x10,0x60,0xb0,0x02 stored at 0x1801f96c..0x1801f984
NZ    = []                                           // indices already non-zero
done  = 0                                            // updates made
repeat passes times:
    for idx in NZ (only those present when the pass starts):
        if get1() == 1:
            R[idx] += (R[idx] < 0 ? -v : v)          // 8-bit arithmetic
            if done == m: STOP ALL; done += 1
    walk the list from its current front to its current end (entries appended this pass are walked):
        e == 0: next
        if get1() == 0: next
        G20 at k: e := (k+4, G16); read_four(k); stay on e
        G16 at k: e := (k, G4); append (k+4,G4),(k+8,G4),(k+12,G4); stay on e
        G4  at k: e := 0; next; read_four(k)
        S   at k: NZ += k; R[k] = get1() ? -v : v; if done == m: STOP ALL; done += 1; e := 0; next
    v >>= 1
read_four(k): for j in 0..3:
    if get1() == 0: NZ += k+j; R[k+j] = get1() ? -v : v; if done == m: STOP ALL; done += 1
    else: PREPEND (k+j, S) to the list front    // walked from the next pass on
```

(The termination test is `bVar = (done == m); done++; if bVar stop`, so `m+1` updates are made at most;
"STOP ALL" stops reading bits for this block.)

2. Apply: `dst[r][c] = (src[r][c] + R[ZZ[r*8+c]]) & 0xff` (byte add, wraps, no clamp), where the
position map is the same permutation as the DCT's (§5.6.2): `ZZ` = inverse of `PERM`. Read from the
store sequence `0x1801fe07`–`0x180200d9`:

```
row 0:  R0  R1  R4  R5  R8  R9  R12 R13
row 1:  R2  R3  R6  R7  R10 R11 R14 R15
row 2:  R24 R25 R44 R45 R16 R17 R20 R21
row 3:  R26 R27 R46 R47 R18 R19 R22 R23
row 4:  R28 R29 R32 R33 R48 R49 R52 R53
row 5:  R30 R31 R34 R35 R50 R51 R54 R55
row 6:  R36 R37 R40 R41 R56 R57 R60 R61
row 7:  R38 R39 R42 R43 R58 R59 R62 R63
```

**List entry encoding** (shared with §5.6.1): one byte `(k << 2) | mode`; `k` = first coefficient index
(coding order), `mode` 0 = group of 20 (`G20`: coefficients k..k+3 read directly, rest becomes G16 at
k+4), 1 = group of 16 (`G16`, splits into four G4), 2 = group of 4 (`G4`), 3 = single (`S`). Byte 0 = empty
slot. Prepended entries go before the current front, appended ones after the current end.

### 5.6 Intra (5) — PROVEN (hash-verified)

`0x18017429` (Ghidra lines 1111–1134): `C[0] = next intra DC`; read coefficients into `C[1..63]`
(`FUN_1801efa0`, §5.6.1); `q = getbits(4)`; `FUN_1801e540(C, QUANT_INTRA[q], dst, P)` (§5.6.3). Order in the
stream: coefficients, then q.

#### 5.6.1 Coefficient reader — PROVEN (hash-verified)

`FUN_1801efa0` (`0x1801efa0`). `T[64]` zeroed (coding order).

```
n = getbits(4)                                       // number of magnitude levels; 0 = no AC coefficients
list = [ (4,G20), (24,G20), (44,G20), (1,S), (2,S), (3,S) ]   // bytes 0x10 0x60 0xb0 0x07 0x0b 0x0f (0x1801f057..)
for L = n down to 1:
    walk the list from its current front to its current end (appended entries walked this level):
        e == 0: next
        if get1() == 0: next
        G20 at k: e := (k+4, G16); read_four(k); stay on e
        G16 at k: e := (k, G4); append (k+4,G4),(k+8,G4),(k+12,G4); stay on e
        G4  at k: e := 0; next; read_four(k)
        S   at k: T[k] = value(getbits(L)); e := 0; next
read_four(k): for j in 0..3:
    if get1() == 0: T[k+j] = value(getbits(L))
    else: PREPEND (k+j, S) to the list front           // walked from the next level on
value(f) (L bits f):  mag = (1 << (L-1)) | (f & ((1 << (L-1)) - 1));  sign = f & (1 << (L-1))
                      return sign ? -mag : mag
```

So a coefficient that becomes significant at level L is sent in full in L bits (low L−1 bits of
magnitude below the implicit top bit, then the sign bit); there is no later refinement. The level-1 pass
is coded separately in the DLL (`0x1801f4b0`–`0x1801f776`) but is this same rule with L = 1 (value ±1,
one sign bit). Coefficients are stored as int16.

#### 5.6.2 Coefficient order → 8×8 position — PROVEN (hash-verified)

The reader writes `C[PERM[k]] = T[k]` for k = 1..63, row-major 8×8 (`C[r*8+c]`); `C[0]` is the DC set by
the caller. Read from the copy sequence `0x1801f77e`–`0x1801f8a1` (stack temp at `esp+0x38`, word pairs
moved as u32, offsets adjusted for the interleaved `pop`s):

```
PERM = [  0,  1,  8,  9,  2,  3, 10, 11,   4,  5, 12, 13,  6,  7, 14, 15,
         20, 21, 28, 29, 22, 23, 30, 31,  16, 17, 24, 25, 32, 33, 40, 41,
         34, 35, 42, 43, 48, 49, 56, 57,  50, 51, 58, 59, 18, 19, 26, 27,
         36, 37, 44, 45, 38, 39, 46, 47,  52, 53, 60, 61, 54, 55, 62, 63 ]
```

#### 5.6.3 Dequantisation and inverse transform — PROVEN (hash-verified)

`FUN_1801e540` (`0x1801e540`; asm `0x1801e561`–`0x1801e860`). All arithmetic 32-bit two's complement
(`imul r32`, `sar`). `Q = QUANT_INTRA[q]` (64 × s32, row-major like `C`).

1-D transform `IDCT8(e0..e7) → z0..z7` (identical in both passes):

```
a  = e0 + e4;           b  = e0 - e4
s  = e2 + e6;           t  = ((e2 - e6) * 0xB50 >> 11) - s
E0 = a + s;  E3 = a - s;  E1 = b + t;  E2 = b - t
u  = e5 + e3;  v = e5 - e3;  w = e1 + e7;  x = e1 - e7
o0 = u + w
k  = (x + v) * 0xEC8 >> 11
o1 = (v * -0x14E8 >> 11) - o0 + k
o2 = ((w - u) * 0xB50 >> 11) - o1
o3 = (x * 0x8A9 >> 11) - k + o2
z0 = E0 + o0;  z7 = E0 - o0
z1 = E1 + o1;  z6 = E1 - o1
z2 = E2 + o2;  z5 = E2 - o2
z4 = E3 + o3;  z3 = E3 - o3
```

(`0xB50` = 2896, `0xEC8` = 3784, `-0x14E8` = −5352, `0x8A9` = 2217; multiply then `>> 11`.)

Pass 1, for each **column** c: `e_r = (C[r*8+c] * Q[r*8+c]) >> 11` for r = 0..7, `tmp[r][c] = z_r`.
(The DLL skips the butterflies when `C[8+c..56+c]` are all zero and writes `e_0` to the whole column;
that equals the full computation.)
Pass 2, for each **row** r: `e_c = tmp[r][c]`; `dst[r][c] = ((z_c + 0x7F) >> 8) & 0xFF` — stored as a
byte, **no clamping** (`lea …+0x7f; sar 8; mov byte`).

`QUANT_INTRA[16][64]` (s32) at VA `0x18029a00` (file `0x1e000`) — Appendix A.

### 5.7 Fill (6) — PROVEN (hash-verified)

All 64 pixels = next colour.

### 5.8 Inter (7) — PROVEN (hash-verified, frame 1 Y)

`0x1801757f` (Ghidra lines 1167–1200): motion as §5.3. **Only if in range**: `C[0] = next inter DC`,
coefficients (§5.6.1), `q = getbits(4)`, then `FUN_1801ec20` (`0x1801ec20`): the same dequantise + IDCT
with `Q = QUANT_INTER[q]` (VA `0x1802aa00`, file `0x1f000`, Appendix B), and the store is
`dst[r][c] = (((z_c + 0x7F) >> 8) + src[r][c]) & 0xFF` (byte add, wraps, no clamp: `sar ebx,8; add bl,
byte [esi+k]; mov byte [ecx+k], bl` at `0x1801eef0`–`0x1801ef79`; table base `add edx, 0x1802aa00` at
`0x1801ec34`).

### 5.9 Pattern (8) — PROVEN (hash-verified)

`0x180176a7`: `c0 = next colour`, `c1 = next colour`, then 8 pattern bytes, one per row. Pixel `k`
(0..7) of row r = `c1` if bit k of that row's byte is 1, else `c0`. (Masks at VA `0x1802df60` (bit set →
`0xff` in byte k) and `0x1802dfa0` (complement), indexed by the low nibble for pixels 0–3 and the high
nibble for pixels 4–7.)

### 5.10 Raw (9) — PROVEN by code (not present in the verified planes)

`0x18017a90`: 64 colours in row-major order.

---

## 6. Frame buffers — PROVEN

The frame-buffer descriptor is the 0x78-byte struct `_BinkGetFrameBuffersInfo@8` fills and BinkOpen copies
to `BINK+0xbc` (decomp l.299–307):

| Off | Field |
|---|---|
| +0x00 | number of buffer sets (2; 1 when `BINK+0x394` is set) |
| +0x04/+0x08 | Y/alpha decode width/height (§2.2) |
| +0x0c/+0x10 | chroma decode width/height |
| +0x14 | index of the **current** buffer set (0 initially) |
| +0x18 + 0x30·i | set i: `+0x00 flag, +0x04 Y ptr, +0x08 Y pitch, +0x0c flag, +0x10 plane-A (V) ptr, +0x14 pitch, +0x18 flag, +0x1c plane-B (U) ptr, +0x20 pitch, +0x24 alpha flag, +0x28 alpha ptr, +0x2c pitch` |

Per frame (`0x18017c7d`–`0x18017cae`): `cur = [+0x14]`; `new = cur ^ 1` if set `cur^1` has a Y buffer,
else `new = cur`; unless run-time flag `0x100` is set, `[+0x14] := new`. Every plane is decoded **into set
`new`** with **set `cur` as the reference** (`edx = [new*0x30+esi+0x1c]` = destination,
`[cur*0x30+esi+0x1c]` = reference, `0x18017d8a`–`0x18017dcb`). So motion, skip, residue and inter blocks
always read the previously decoded frame, and the two sets ping-pong. The displayed frame is set
`[+0x14]` after the call (BinkCopyToBufferRect uses `[fb+0x14]*0x30 + 0x18`).

Edges: no padding or extension is done; out-of-range motion writes nothing (§5.3). What an un-written
block then shows is whatever the destination set held (the frame before the reference). Initial buffer
contents: not determined (§9).

Keyframes (index bit 0) need no special handling in the decoder: they simply contain no reference-using
blocks (observed: motion/inter bundles count 0 at row 0 of frame 0, §8).

---

## 7. Output

### 7.1 Planes and timing — PROVEN

The decoded result is planar 4:2:0: Y (W×H), V and U ((W/2)×(H/2) rounded as in §2.2), optional alpha
(W×H). For a display of `w×h`, crop Y to `w×h` and chroma to `ceil(w/2)×ceil(h/2)` (§8 matched ffmpeg's
`yuv420p` output this way: 600×720 Y, 300×360 chroma).

Frame rate = header@28 / header@32 (`BINK+0x14 / +0x18`); `_BinkSetFrameRate@8` with open flag `0x1000`
can override it (BinkOpen l.167–175). ffprobe's `r_frame_rate` agrees for all three test files.

### 7.2 `_BinkCopyToBuffer` colour conversion — partly PROVEN, partly open

`_BinkCopyToBuffer@28` (`0x1800e720`) → `_BinkCopyToBufferRect@44` (`0x1800e760`) → `FUN_180192b0` →
per-line converter `DAT_180241b8`. The converter is chosen by `FUN_18019a00` (`0x18019a00`), which calls
the 13 selector functions listed at VA `0x1802e2bc` in order until one accepts the copy flags
(`0x18001a30 0x180018d0 0x18001c20 0x18001b20 0x18001d60 0x18001cd0 0x18009260 0x18008f20 0x18006f30
0x18006bd0 0x18004860 0x18001df0 0x180057b0`). The first ones require CPU-feature bit 0 of `DAT_180242f8`
(set by `FUN_18001850` from CPUID.1:EDX bit 23 = MMX), so on any current CPU an MMX/SSE converter is used.

Copy-flag bits seen: low nibble = surface type; `0x800` swaps the Y and alpha sources; `0x10000` swaps
the two chroma planes (BinkOpen sets it for BIKf/BIKg, so in BIKi plane A stays the red-difference
input); `0x20000` grayscale. `_BinkDX9SurfaceType@4` (`0x1800cd10`) maps D3DFORMAT values `0x14→1, 0x15→5,
0x16→3, 0x17→10, 0x18→9, 0x19→8, 0x1a→7, 0x1e→7, 'YUY2'→13, 'UYVY'→14` (that `0x15` is
`D3DFMT_A8R8G8B8` is the public D3D9 enum value: INFERRED naming).

**C fallback converter for surface types 5/6** (`FUN_18009260` selects `FUN_18009080`), PROVEN:
tables built by `FUN_18018a20` (`0x18018a20`):

- `LUMA[Y]` (bytes at VA `0x1802e1b8`): 0 for Y < 16; `floor((Y−16) * 0x12A68 / 65536)` for
  16 ≤ Y ≤ 234 (219 entries); 255 for Y ≥ 235.
- `CHR[c]` (signed bytes at VA `0x1802e0b8`): `((c−128) * 0x67CC) >> 16` for c = 0..254; the loop stops
  before c = 255 (`iVar6 < 0x337e34`), so `CHR[255]` keeps its static file value **0**.
- `CLAMP[i]` (u32 at VA `0x18023998`, valid i = −256..511): 0 for i < 0, i for 0..255, 255 above (for the
  32-bit layouts, where all `param_7/8/2` shifts are 0).

With `Yl = LUMA[Y]`, `cA = CHR[plane A (V)]`, `cB = CHR[plane B (U)]` (asm `0x180090dd`–`0x18009139`; the
odd displacements `0x1802398e`, `0x18023994`, `0x1802399b` are unaligned reads into `CLAMP`):

```
X  = CLAMP[Yl + 4*cA - 2]          // read as CLAMP[]<<16
Z  = CLAMP[Yl + 5*cB - 1]
G  = CLAMP[Yl - cB - 2*cA + 1]     // read as CLAMP[]<<8
pixel = rotl((X << 16) | Z, DAT_180241ac) | (G << 8) | (alpha << 24)
```

For type 5 `DAT_180241ac = 0`, so `X` lands in bits 16–23 (R of A8R8G8B8), `Z` in bits 0–7 (B): plane A
is red-difference (V), plane B blue-difference (U), consistent with §1.5. Type 6 rotates by 16 (R/B
swapped). `CHR` steps of 0.4055 times 4 / 5 / 1 / 2 give ≈1.62·V, ≈2.03·U, ≈0.41·U + 0.81·V — INFERRED to be
an approximation of the BT.601 studio-swing matrix; not needed for decoding.

**MMX converter for surface types 3/4/5/6** — the one a current CPU gets. Selection: `FUN_18001a30`
(first in the list) accepts types 5/6 only when its second argument is non-zero, which
`_BinkCopyToBufferRect` passes as "buffer set 0 has an alpha plane" (`*(fb+0x40) != 0`, decomp
`iVar6 = *(int *)(param_1[0x2f] + 0x40)`); it then uses `FUN_180086f0`. Otherwise `FUN_180018d0`
accepts types 1–6 and installs `FUN_18008000` (`DAT_180241c4 = FUN_18008000`, made the line converter by
`FUN_18018e10`). Both share this per-pixel arithmetic, PROVEN from the single-pixel path
`0x1800806b`–`0x18008100` (`0x1800875f`–`0x18008809` in `FUN_180086f0`); the 2- and 8-pixel paths use
the same constants and operation sequence (read, not separately tested):

```
// Y = luma byte, V = plane A byte, U = plane B byte; all pmulhw operands are < 2^15
u = (U*16 * 0x6810) >> 16
v = (V*32 * 0x6810) >> 16
y = (Y*64 * 0x4A89) >> 16
cv(a, k) = (max(min(a + k, 0x7FFF) - 0x7000, 0)) >> 4      // paddsw, psubusw, psrlw 4 -> 0..255
X = cv(2*v + y,     0x61D0)       // shifted left by DAT_18023500
G = cv(y - (u + v), 0x78A7)       // shifted left by 8
Z = cv(5*u + y,     0x5E8D)       // shifted left by DAT_18023510
pixel = (X << DAT_18023500) | (G << 8) | (Z << DAT_18023510) [| alpha << 24 in FUN_180086f0]
```

Constants: `0x4A89`, `0x6810`, `0x7000` (×4 words) are static at VA `0x1802e080/88/90` (no writes found
in the decompile); `0x61D0`, `0x78A7`, `0x5E8D` are stored into all four words of `DAT_18023540…4c`,
`…50…5c`, `…60…6c` by the selector (the per-row rotation of those words in `FUN_180192b0` therefore has no
effect for these types). For type 5 the selector calls `FUN_18018740(0,0,0x18,0x10,8,0,0,0)`:
`DAT_18023500 = 0x10`, `DAT_18023510 = 0` → `X` (from V) is R in bits 16–23, `Z` (from U) is B in bits
0–7. Type 6 swaps those two shifts. In real numbers this is R ≈ 1.1646·Y + 1.6260·V − 227,
G ≈ 1.1646·Y − 0.4065·U − 0.8130·V + 138.4, B ≈ 1.1646·Y + 2.0325·U − 279.2 (INFERRED: BT.601
studio-swing; computed from the constants, not tested on output).

---

## 8. Verification — shell_mattias.bik frames 0 and 1

Black box: `ffmpeg -v error -y -i shell_mattias.bik -frames:v 2 -f rawvideo -pix_fmt yuv420p out.yuv`
→ 1 296 000 bytes = 2 × (600·720 + 2·300·360). Planes split with `dd`, hashed with `shasum -a 256`:

| Plane (ffmpeg) | sha256 |
|---|---|
| frame 0 Y | `5f8a505fb8651f6a84bd6fe031d86f3e2eccc35c9da5510f77206d4395485441` |
| frame 0 U | `40ddd0f64982c466b19bfd5f12628e3e2cc699849a8704fa9b321e24be62829c` |
| frame 0 V | `33e5ad60f1297ba46f0d77fbcd396fbf64030e0b23afbe21f63cfaa2c394b84e` |
| frame 1 Y | `d18c80fbe63bc9f17c66c489e0d445b2851b326f0f41e01c96a7420608ea5682` |
| frame 1 U | `2cb33f2a96f5c22a1e296593aa347eda35f9cebb9d5a1419af71aedc5c7332b9` |
| frame 1 V | `57c93697d615bce9d9f7c995529c7599d3bc1c77a987a015525160fe3937533f` |

Check method: a throw-away bash walker (scratch only, not a deliverable; written from §3–§5 of this
document, using the DLL's own tables read from the DLL file) decoded planes and wrote the cropped plane
bytes, which were hashed and compared with the table above. Results:

| Check | Result |
|---|---|
| Plane dimensions: Y 600×720, chroma decoded 304×360 cropped to 300×360 | matches ffmpeg's plane sizes — PROVEN |
| Frame 0 Y header + row 0 (`0x1160`): block-type tree `t=14`, explicit map `5 1 8 6 12 13 …`; block types start `1 1 1 …` (scaled), sub-types `6 6 …` (fill), first colour `16` | ffmpeg Y(0..15, 0..7) = `0x10` — matches |
| Frame 0 plane A (`0x8204`): full walk (types 1/6, sub-type 6) | sha256 = ffmpeg **V** `33e5ad60…` — **bit-exact**; plane A ends at bit 2671 → plane B at `0x8354` |
| Frame 0 plane B (`0x8354`): full walk (types 1/5/6; sub-types 3/5/6/8; intra DCT, scaled run, scaled pattern, intra-DC and run bundles) | sha256 = ffmpeg **U** `40ddd0f6…` — **bit-exact**; ends exactly at `0x871c` = frame 1 offset |
| Frame 1 plane A (`0xce54`), reference = frame 0 plane A (304 wide) | sha256 = ffmpeg V `57c93697…` — **bit-exact**; ends at `0xcec4` |
| Frame 1 plane B (`0xcec4`): block types 0 ×1202, 1 ×172, 2 ×66, 6 ×98; scaled sub-types 5 ×6, 6 ×80; no out-of-range motion | sha256 = ffmpeg U `2cb33f2a…` — **bit-exact**; ends exactly at `0xd0b0` = `index[2]` |
| Frame 0 Y and frame 1 Y full planes | both **bit-exact**, §8.1 |

This proves, against independent output: the bit reader, tree descriptors (explicit and shuffle forms),
the code tables, the per-row bundle refill and termination rule, block-type runs, colour contexts,
pattern, motion bundles and the x/y index sync, intra-DC and run bundles, the coefficient reader, `PERM`,
`QUANT_INTRA`, the IDCT and its rounding, skip, motion copy, fill, pattern, run, scaled blocks, the
plane order (A = V, B = U) and the word-aligned plane boundary.

### 8.1 Luma planes

| Check | Result |
|---|---|
| Frame 0 Y (`0x1160`, 600×720): full walk; block types 1/5/6/8, sub-types 3/5/6/8 (the walker aborts on any kind it does not implement, so no type 0/2/3/4/7/9 and no sub-type 9 occur); trees include a `d = 3` shuffle (pattern tree) | sha256 = ffmpeg Y `5f8a505f…` — **bit-exact**; 230 686 bits, so plane A starts at `0x1160 + 4·ceil(230686/32) = 0x8204`, which is exactly `0x115c + Y size 0x70a8` |
| Frame 1 Y (`0x8720`, reference = frame 0 Y): block types 0 ×3975, 1 ×20, 2 ×854, 5 ×847, 6 ×76, **7 ×895**, 8 ×63; scaled sub-types 5 ×7, 8 ×3; no out-of-range motion; no type 3, 4 or 9 | sha256 = ffmpeg Y `d18c80fb…` — **bit-exact**; 145 802 bits → plane A at `0xce54` = `0x871c + 0x4738` |

So inter blocks (§5.8), the inter-DC bundle (signed first value) and `QUANT_INTER` are also verified.
Not covered by any verified plane: residue (type 4), raw (type 9, sub-type 9), the bundle fill paths
for motion/runs/colours, and tree shuffles with `d = 0, 1, 2`.

### 8.2 Intermediate values for unit tests

Produced by the same walker whose final planes matched ffmpeg bit-exactly (so they are the values the
DLL's rules give on these bytes). Bit positions count from the plane's first bit.

| Plane | Trees end at bit | Plane length (bits) |
|---|---|---|
| frame 0 Y (`0x1160`) | 529 | 230 686 |
| frame 0 A/V (`0x8204`) | 148 | 2 671 |
| frame 0 B/U (`0x8354`) | 223 | 7 735 |
| frame 1 Y (`0x8720`) | 233 | 145 802 |
| frame 1 A/V (`0xce54`) | 144 | 866 |
| frame 1 B/U (`0xcec4`) | 180 | 3 934 |

Frame 0 Y header (tree number per §4.1: `t`, maxlen, map):

```
#1  block types   t=14 maxlen=7  map 5 1 8 6 12 13 0 2 3 4 7 9 10 11 14 15   (explicit list, k=5)
#2  sub-types     t=14 maxlen=7  map 6 5 12 8 14 3 0 1 2 4 7 9 10 11 13 15   (explicit list, k=5)
#3  colour hi[0]  t=0            identity
#4  colour hi[1]  t=11 maxlen=6  map 1 3 10 11 8 0 2 4 5 6 7 9 12 13 14 15
#19 colour lo     t=1  maxlen=5  map 0 8 1 2 3 4 5 6 7 9 10 11 12 13 14 15
#20 pattern       t=5  maxlen=5  map 0 3 1 2 4 6 7 8 9 12 14 15 5 13 10 11   (shuffle, d=3)
#23 runs          t=1  maxlen=5  map 15 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14
```

Frame 0 Y row 0 bundle counts: block types 560, sub-types 522, colours 550 (no fill), pattern 552,
motion x 0, motion y 0, intra DC 544, inter DC 0, runs 34; bit position after the row-0 bundles 13 205.
First values: block types `1 ×37, 6, 1 ×37, 6, …`; sub-types `6 ×46, 5, 3, 6, …`; colours `16 16 16 …`;
pattern `0 0 0 0 0 3 127 240 252 193 …`; intra DC `12 9 21 16 11 19 17 21 …`; runs `15 15 15 13 1 5 …`.

Frame 0 plane A row 0: block types 524, sub-types 392 (all 6), colours 534 (`128 ×24, 130 130 129 130
131 …`), then six zero counts (pattern, motion x, motion y, intra DC, inter DC: 10 bits each; runs: 12
bits) = 62 zero bits from bit 2288 to bit 2350.

---

## 9. Open

1. **Residue (type 4)** is specified from the code (`FUN_1801f8e0`, `FUN_1801fdd0`, asm-checked list
   bytes and store map) but no verified plane contains one (frames 0–1 of shell_mattias have none). It
   needs a test against a later frame or another movie. (Inter, type 7, is verified — §8.1.)
2. **Raw (type 9)** and **scaled raw (sub-type 9)** not exercised by a verified plane.
3. **Alpha plane** and **grayscale** paths: read from `FUN_18017bb0` only; no shipped file sets the flags
   (all three measured files have header@36 = 0).
4. **Initial content of the frame buffers** (matters only if the first decoded frame used skip/motion, or
   for out-of-range motion): allocation goes through `FUN_18001780`; whether it is zeroed was not read.
5. **Colour converters**: only the 32-bit ones are specified (C `FUN_18009080`, MMX `FUN_18008000` /
   `FUN_180086f0`, single-pixel path read instruction by instruction). The 16-bit, 24-bit, 8-bit and
   YUY2/UYVY converters (sections `BINK16`, `BINKY16`, `BINKY12`, `BINKP8`) were not read, and no RGB
   output was compared against anything.
6. **How the game consumes frames** (whether it calls `BinkCopyToBuffer` and with which surface type, or
   uses `BinkGetFrameBuffersInfo`/`BinkRegisterFrameBuffers` and converts on the GPU) was not traced in
   the game exe; this document covers the DLL only.
7. **BIKf/BIKg/BIKh** differences (colour reader `FUN_18014520`, missing Y size prefix, chroma swap) are
   only noted, not specified; no shipped file uses them.
8. The dirty-block map written through `param_8/9/10` of `FUN_18015500` (used for partial blits) and the
   skipped-block statistic are not part of decoding and are not specified.

---

## Appendix A — `QUANT_INTRA[16][64]` (s32), VA `0x18029a00`, file `0x1e000`

Table q occupies 256 bytes at file `0x1e000 + q*0x100`; entry order = row-major 8×8 (`[r*8+c]`), same
indexing as `C`. Dump (`od -A x -t x4`, file offset; VA = offset + `0x1800BA00`):

```
001e000          00010000        00016315        00014e7b        00016577
001e010          00010000        0000eeda        0000be80        0000611e
001e020          0001e83d        0002a535        0002f1e6        0002724c
001e030          00024102        00017f9b        0001083c        0000a552
001e040          0002346f        00030ee5        0002c628        00027f20
001e050          00021f88        0001dc53        00014819        0000a743
001e060          0001fbfa        0002c096        000297b5        00023f32
001e070          00027fad        0001f697        00015a31        00009688
001e080          0001d000        00028396        0002346f        0001fbfa
001e090          00025000        0001ab6b        00012669        00008d43
001e0a0          00019247        0001f9aa        0001dc53        000231b8
001e0b0          0001d122        000159b3        0000ee1f        000075ed
001e0c0          00012f12        0001e06c        0001c48c        00019748
001e0d0          0001490c        00010288        0000e0f1        000072ad
001e0e0          0000cb10        000119a8        00014e86        000122af
001e0f0          0000f735        0000ef51        0000a4d8        00006517
001e100          00015555        0001d971        0001bdf9        0001dc9f
001e110          00015555        00013e78        0000fe00        0000817d
001e120          00028afc        000386f1        0003ed33        00034311
001e130          00030158        0001ff7a        0001604f        0000dc6d
001e140          0002f095        000413dc        0003b2e0        0003542a
001e150          0002d4b5        00027b19        0001b577        0000df04
001e160          0002a54e        0003ab73        000374f1        0002feee
001e170          000354e7        00029e1f        0001cd96        0000c8b6
001e180          00026aab        00035a1e        0002f095        0002a54e
001e190          00031555        000239e4        0001888c        0000bc59
001e1a0          0002185e        0002a238        00027b19        0002ecf5
001e1b0          00026c2d        0001ccee        00013d7e        00009d3c
001e1c0          00019418        00028090        00025b66        00021f0b
001e1d0          0001b6bb        000158b5        00012bec        000098e6
001e1e0          00010ec0        0001778a        0001be09        00018394
001e1f0          0001499c        00013f17        0000dbcb        000086c9
001e200          0001aaab        00024fce        00022d78        000253c7
001e210          0001aaab        00018e16        00013d80        0000a1dc
001e220          00032dbb        000468ad        0004e87f        000413d5
001e230          0003c1ae        00027f58        0001b863        00011388
001e240          0003acba        000518d3        00049f98        00042935
001e250          000389e2        000319df        000222d4        000116c5
001e260          00034ea1        0004964f        0004522d        0003bea9
001e270          00042a21        000345a7        000240fc        0000fae3
001e280          00030555        000430a5        0003acba        00034ea1
001e290          0003daab        0002c85d        0001eaaf        0000eb6f
001e2a0          00029e76        00034ac5        000319df        0003a833
001e2b0          00030738        0002402a        00018cde        0000c48a
001e2c0          0001f91e        000320b4        0002f23f        0002a6ce
001e2d0          00022469        0001aee2        000176e7        0000bf20
001e2e0          00015270        0001d56d        00022d8b        0001e479
001e2f0          00019c02        00018edd        000112be        0000a87b
001e300          00020000        0002c62a        00029cf6        0002caef
001e310          00020000        0001ddb4        00017d01        0000c23c
001e320          0003d07a        00054a69        0005e3cc        0004e499
001e330          00048204        0002ff36        00021077        00014aa3
001e340          000468df        00061dca        00058c50        0004fe3f
001e350          00043f0f        0003b8a6        00029032        00014e86
001e360          0003f7f5        0005812c        00052f69        00047e65
001e370          0004ff5a        0003ed2e        0002b461        00012d11
001e380          0003a000        0005072c        000468df        0003f7f5
001e390          0004a000        000356d6        00024cd2        00011a85
001e3a0          0003248d        0003f353        0003b8a6        00046370
001e3b0          0003a243        0002b365        0001dc3e        0000ebd9
001e3c0          00025e24        0003c0d8        00038919        00032e91
001e3d0          00029218        00020510        0001c1e2        0000e559
001e3e0          00019620        0002334f        00029d0d        0002455e
001e3f0          0001ee69        0001dea2        000149b0        0000ca2d
001e400          0002aaab        0003b2e3        00037bf2        0003b93e
001e410          0002aaab        00027cf0        0001fc01        000102fa
001e420          000515f8        00070de2        0007da65        00068621
001e430          000602b1        0003fef3        0002c09f        0001b8da
001e440          0005e129        000827b8        000765c0        0006a855
001e450          0005a96a        0004f632        00036aed        0001be09
001e460          00054a9c        000756e5        0006e9e2        0005fddb
001e470          0006a9ce        00053c3e        00039b2d        0001916b
001e480          0004d555        0006b43b        0005e129        00054a9c
001e490          00062aab        000473c8        00031118        000178b2
001e4a0          000430bc        0005446f        0004f632        0005d9eb
001e4b0          0004d85a        000399dc        00027afd        00013a77
001e4c0          00032830        00050121        0004b6cc        00043e16
001e4d0          00036d76        0002b16a        000257d8        000131cc
001e4e0          00021d80        0002ef14        00037c11        00030728
001e4f0          00029337        00027e2e        0001b796        00010d91
001e500          00038000        0004daca        000492ae        0004e322
001e510          00038000        000343fb        00029ac1        000153e8
001e520          0006acd5        00094238        000a4ea5        0008900c
001e530          0007e388        00053e9f        00039cd0        0002429e
001e540          0007b786        000ab421        0009b58c        0008bcef
001e550          00076e5b        00068322        00047c57        0002496b
001e560          0006f1ed        0009a20d        000912f8        0007dd30
001e570          0008bede        0006df11        0004bbab        00020edd
001e580          00065800        0008cc8e        0007b786        0006f1ed
001e590          00081800        0005d7f7        00040670        0001ee69
001e5a0          00057ff7        0006e9d2        00068322        0007ae04
001e5b0          00065bf6        0004b9f1        0003416c        00019cbc
001e5c0          000424bf        0006917b        00062feb        0005917d
001e5d0          00047faa        000388dc        0003134c        0001915c
001e5e0          0002c6b8        0003d9cb        000492d7        0003f964
001e5f0          00036138        0003459c        000240f5        000161cf
001e600          00040000        00058c54        000539ec        000595dd
001e610          00040000        0003bb68        0002fa01        00018477
001e620          0007a0f4        000a94d3        000bc798        0009c932
001e630          00090409        0005fe6d        000420ee        00029547
001e640          0008d1be        000c3b94        000b18a0        0009fc7f
001e650          00087e1f        0007714c        00052064        00029d0d
001e660          0007efea        000b0258        000a5ed3        0008fcc9
001e670          0009feb5        0007da5d        000568c3        00025a21
001e680          00074000        000a0e59        0008d1be        0007efea
001e690          00094000        0006adac        000499a5        0002350b
001e6a0          0006491a        0007e6a7        0007714c        0008c6e0
001e6b0          00074487        000566ca        0003b87b        0001d7b3
001e6c0          0004bc48        000781b1        00071232        00065d22
001e6d0          00052430        00040a20        000383c5        0001cab3
001e6e0          00032c3f        0004669f        00053a1a        00048abc
001e6f0          0003dcd3        0003bd45        00029361        0001945a
001e700          00050000        0006ef69        00068867        0006fb55
001e710          00050000        0004aa42        0003b881        0001e595
001e720          00098931        000d3a07        000eb97e        000c3b7e
001e730          000b450b        00077e08        0005292a        00033a99
001e740          000b062d        000f4a78        000ddec8        000c7b9f
001e750          000a9da7        00094d9f        0006687d        00034450
001e760          0009ebe4        000dc2ee        000cf687        000b3bfb
001e770          000c7e62        0009d0f4        0006c2f4        0002f0aa
001e780          00091000        000c91ef        000b062d        0009ebe4
001e790          000b9000        00085917        0005c00e        0002c24d
001e7a0          0007db61        0009e050        00094d9f        000af898
001e7b0          000915a8        0006c07d        0004a69a        00024d9f
001e7c0          0005eb59        0009621d        0008d6be        0007f46a
001e7d0          00066d3c        00050ca7        000464b6        00023d5f
001e7e0          0003f74f        00058046        000688a0        0005ad6b
001e7f0          0004d407        0004ac96        00033839        0001f971
001e800          00060000        0008527e        0007d6e1        000860cc
001e810          00060000        0005991c        00047702        000246b3
001e820          000b716e        000fdf3c        0011ab63        000eadcb
001e830          000d860d        0008fda3        00063165        0003dfea
001e840          000d3a9c        0012595d        0010a4f0        000efabe
001e850          000cbd2e        000b29f1        0007b096        0003eb93
001e860          000be7df        00108384        000f8e3c        000d7b2e
001e870          000efe0f        000bc78b        00081d24        00038732
001e880          000ae000        000f1585        000d3a9c        000be7df
001e890          000de000        000a0482        0006e677        00034f90
001e8a0          00096da8        000bd9fa        000b29f1        000d2a50
001e8b0          000ae6ca        00081a2f        000594b9        0002c38c
001e8c0          00071a6b        000b4289        000a9b4a        00098bb2
001e8d0          0007b649        00060f2f        000545a7        0002b00c
001e8e0          0004c25f        000699ee        0007d727        0006d01a
001e8f0          0005cb3c        00059be7        0003dd11        00025e87
001e900          00080000        000b18a8        000a73d7        000b2bbb
001e910          00080000        000776cf        0005f402        000308ef
001e920          000f41e8        001529a5        00178f2f        00139264
001e930          00120812        000bfcd9        000841dc        00052a8e
001e940          0011a37b        00187727        00163140        0013f8fe
001e950          0010fc3e        000ee297        000a40c8        00053a1a
001e960          000fdfd4        001604b0        0014bda5        0011f992
001e970          0013fd69        000fb4b9        000ad186        0004b442
001e980          000e8000        00141cb1        0011a37b        000fdfd4
001e990          00128000        000d5b58        00093349        00046a15
001e9a0          000c9235        000fcd4d        000ee297        00118dc0
001e9b0          000e890d        000acd94        000770f7        0003af65
001e9c0          0009788f        000f0362        000e2463        000cba43
001e9d0          000a4861        0008143f        00070789        00039565
001e9e0          0006587f        0008cd3d        000a7434        00091577
001e9f0          0007b9a6        00077a89        000526c2        000328b4
001ea00          000c0000        0010a4fd        000fadc3        0010c198
001ea10          000c0000        000b3237        0008ee03        00048d66
001ea20          0016e2db        001fbe78        002356c7        001d5b96
001ea30          001b0c1a        0011fb46        000c62ca        0007bfd5
001ea40          001a7539        0024b2bb        002149e1        001df57d
001ea50          00197a5d        001653e3        000f612c        0007d727
001ea60          0017cfbd        00210709        001f1c78        001af65b
001ea70          001dfc1e        00178f16        00103a49        00070e64
001ea80          0015c000        001e2b0a        001a7539        0017cfbd
001ea90          001bc000        00140904        000dccee        00069f20
001eaa0          0012db4f        0017b3f4        001653e3        001a54a0
001eab0          0015cd94        0010345e        000b2972        00058718
001eac0          000e34d7        00168513        00153695        00131765
001ead0          000f6c91        000c1e5e        000a8b4e        00056018
001eae0          000984be        000d33dc        000fae4e        000da033
001eaf0          000b9678        000b37ce        0007ba22        0004bd0e
001eb00          00110000        00179466        0016362a        0017bced
001eb10          00110000        000fdc79        000ca685        000672fb
001eb20          00206c0c        002cf87f        00321044        00299714
001eb30          00265125        0019794e        00118bf4        000afa6d
001eb40          00257b66        0033fd33        002f28a9        002a711b
001eb50          00241804        001fa181        0015c9a9        000b1b77
001eb60          0021bba2        002ec9f7        002c12ff        00263256
001eb70          002a7a80        0021600a        0016fd3c        0009ff0d
001eb80          001ed000        002abcf9        00257b66        0021bba2
001eb90          00275000        001c621b        00138cfb        0009616e
001eba0          001ab6b0        00219444        001fa181        00254d38
001ebb0          001ee33c        0016f4db        000fd00c        0007d4b7
001ebc0          00142030        001fe730        001e0d52        001b0bcf
001ebd0          0015d9ce        00112b06        000ef004        00079d77
001ebe0          000d7c0e        0012b423        001636ee        00134d9e
001ebf0          00106a80        000fe464        000af25b        0006b67f
001ec00          00160000        001e83cf        001cbe90        001eb842
001ec10          00160000        001486ba        00105f06        00085891
001ec20          0029f53d        003a3286        0040c9c2        0035d293
001ec30          00319630        0020f756        0016b51e        000e3506
001ec40          00308193        004347ac        003d0771        0036ecba
001ec50          002eb5aa        0028ef20        001c3225        000e5fc7
001ec60          002ba786        003c8ce5        00390986        00316e52
001ec70          0036f8e1        002b30fe        001dc030        000cefb7
001ec80          0027e000        00374ee7        00308193        002ba786
001ec90          0032e000        0024bb33        00194d09        000c23bb
001eca0          00229212        002b7494        0028ef20        003045d0
001ecb0          0027f8e4        001db557        001476a6        000a2256
001ecc0          001a0b89        0029494d        0026e410        00230039
001ecd0          001c470a        001637ad        001354b9        0009dad6
001ece0          0011735d        00183469        001cbf8f        0018fb09
001ecf0          00153e87        001490fa        000e2a94        0008aff0
001ed00          001c0000        0026d64d        00249572        0027190e
001ed10          001c0000        001a1fd6        0014d607        000a9f44
001ed20          003566aa        004a11c2        00527525        0044805e
001ed30          003f1c3e        0029f4f9        001ce683        001214f0
001ed40          003dbc30        0055a109        004dac61        0045e778
001ed50          003b72d9        00341911        0023e2bb        00124b5b
001ed60          00378f64        004d1069        004897c2        003ee97f
001ed70          0045f6f0        0036f889        0025dd54        001076e9
001ed80          0032c000        0046646c        003dbc30        00378f64
001ed90          0040c000        002ebfb5        00203380        000f734b
001eda0          002bffb9        00374e8e        00341911        003d7020
001edb0          0032dfae        0025cf86        001a0b5f        000ce5e2
001edc0          002125f5        00348bd6        00317f5b        002c8beb
001edd0          0023fd53        001c46dc        00189a60        000c8ae2
001ede0          001635bc        001ece57        002496b6        001fcb22
001edf0          001b09c4        001a2ce1        001207a5        000b0e77
001ee00          00220000        002f28cc        002c6c53        002f79da
001ee10          00220000        001fb8f1        00194d09        000ce5f7
001ee20          0040d818        0059f0fe        00642089        00532e29
001ee30          004ca24b        0032f29c        002317e8        0015f4db
001ee40          004af6cc        0067fa67        005e5152        0054e237
001ee50          00483007        003f4303        002b9351        001636ee
001ee60          00437743        005d93ee        005825fe        004c64ad
001ee70          0054f4ff        0042c014        002dfa79        0013fe1a
001ee80          003da000        005579f1        004af6cc        00437743
001ee90          004ea000        0038c437        002719f7        0012c2db
001eea0          00356d61        00432888        003f4303        004a9a70
001eeb0          003dc678        002de9b5        001fa018        000fa96e
001eec0          00284060        003fce60        003c1aa5        0036179d
001eed0          002bb39b        0022560c        001de007        000f3aee
001eee0          001af81b        00256845        002c6ddd        00269b3c
001eef0          0020d500        001fc8c8        0015e4b7        000d6cfe
001ef00          002c0000        003d079e        00397d20        003d7083
001ef10          002c0000        00290d75        0020be0c        0010b121
001ef20          0053ea79        0074650c        00819383        006ba525
001ef30          00632c61        0041eeac        002d6a3b        001c6a0c
001ef40          00610326        00868f57        007a0ee2        006dd974
001ef50          005d6b54        0051de40        0038644b        001cbf8f
001ef60          00574f0b        007919ca        0072130c        0062dca3
001ef70          006df1c2        005661fb        003b8060        0019df6d
001ef80          004fc000        006e9dce        00610326        00574f0b
001ef90          0065c000        00497665        00329a12        00184776
001efa0          00452423        0056e928        0051de40        00608ba0
001efb0          004ff1c9        003b6aae        0028ed4d        001444ac
001efc0          00341713        0052929a        004dc821        00460071
001efd0          00388e14        002c6f5a        0026a973        0013b5ad
001efe0          0022e6ba        003068d2        00397f1e        0031f611
001eff0          002a7d0f        002921f4        001c5528        00115fdf
```

## Appendix B — `QUANT_INTER[16][64]` (s32), VA `0x1802aa00`, file `0x1f000`

```
001f000          00010000        00017946        00016363        000152a7
001f010          00012000        0000e248        0000a486        000053e0
001f020          0001a5a9        000248dc        000243ec        000209ea
001f030          0001bbda        00015cbc        0000f036        00008095
001f040          0001b701        000260eb        00023d97        00020437
001f050          0001b701        00016959        0000f8e7        00007ee4
001f060          00019de9        00023e1b        00021ccc        0001e6b4
001f070          0001b0b9        000153fd        0000ea30        00007763
001f080          00017000        0001fe6e        0001e0d1        0001b0b9
001f090          00018000        00012db5        0000cfd2        00006e5c
001f0a0          00012db5        0001a27b        00018a33        0001718d
001f0b0          000146d9        000100ce        0000b0e4        00005a2d
001f0c0          0000d87a        00014449        00013178        000112ea
001f0d0          0000e9cc        0000b7b1        00008337        000042e5
001f0e0          00007b9a        0000ab71        0000ad08        00009bb9
001f0f0          0000846f        00006b85        00004a10        00002831
001f100          00015555        0001f708        0001d9d9        0001c389
001f110          00018000        00012db5        0000db5d        00006fd5
001f120          00023237        00030bd0        0003053b        0002b7e3
001f130          00024fce        0001d0fa        00014048        0000ab71
001f140          00024957        00032be4        0002fcc9        0002b04a
001f150          00024957        0001e1cc        00014bde        0000a92f
001f160          000227e1        0002fd7a        0002d110        000288f1
001f170          000240f7        0001c551        00013840        00009f2f
001f180          0001eaab        0002a893        00028116        000240f7
001f190          00020000        00019247        00011518        00009325
001f1a0          00019247        00022df9        00020d99        0001ecbc
001f1b0          0001b3cc        00015668        0000ebda        0000783d
001f1c0          000120a3        0001b061        0001974b        00016e8e
001f1d0          000137bb        0000f4ed        0000aef4        00005931
001f1e0          0000a4ce        0000e497        0000e6b5        0000cfa2
001f1f0          0000b093        00008f5c        000062bf        00003597
001f200          0001aaab        000274cb        0002504f        0002346c
001f210          0001e000        00017922        00011235        00008bca
001f220          0002bec4        0003cec4        0003c689        000365dc
001f230          0002e3c1        00024539        0001905a        0000d64d
001f240          0002dbad        0003f6dd        0003bbfb        00035c5c
001f250          0002dbad        00025a40        00019ed6        0000d37b
001f260          0002b1d9        0003bcd8        00038554        00032b2d
001f270          0002d134        000236a5        00018650        0000c6fb
001f280          00026555        000352b8        0003215c        0002d134
001f290          00028000        0001f6d8        00015a5e        0000b7ef
001f2a0          0001f6d8        0002b977        00029100        000267eb
001f2b0          000220c0        0001ac02        000126d1        0000964c
001f2c0          000168cc        00021c7a        0001fd1e        0001ca31
001f2d0          000185a9        00013228        0000dab2        00006f7d
001f2e0          0000ce01        00011dbd        00012062        0001038a
001f2f0          0000dcb8        0000b333        00007b6f        000042fc
001f300          00020000        0002f28d        0002c6c5        0002a54e
001f310          00024000        0001c48f        0001490c        0000a7bf
001f320          00034b52        000491b8        000487d8        000413d5
001f330          000377b5        0002b977        0001e06c        0001012a
001f340          00036e03        0004c1d6        00047b2d        0004086e
001f350          00036e03        0002d2b3        0001f1ce        0000fdc7
001f360          00033bd1        00047c37        00043998        0003cd69
001f370          00036172        0002a7fa        0001d460        0000eec7
001f380          0002e000        0003fcdd        0003c1a1        00036172
001f390          00030000        00025b6a        00019fa3        0000dcb8
001f3a0          00025b6a        000344f5        00031466        0002e31b
001f3b0          00028db3        0002019b        000161c7        0000b45b
001f3c0          0001b0f5        00028892        000262f1        000225d5
001f3d0          0001d398        00016f63        0001066f        000085c9
001f3e0          0000f735        000156e2        00015a10        00013772
001f3f0          000108dd        0000d70a        0000941f        00005062
001f400          0002aaab        0003ee11        0003b3b2        00038713
001f410          00030000        00025b6a        0001b6bb        0000dfaa
001f420          0004646d        000617a0        00060a75        00056fc6
001f430          00049f9b        0003a1f4        00028090        000156e2
001f440          000492ae        000657c8        0005f991        00056093
001f450          000492ae        0003c399        000297bd        0001525f
001f460          00044fc1        0005faf4        0005a220        000511e1
001f470          000481ed        00038aa2        00027080        00013e5e
001f480          0003d555        00055126        0005022d        000481ed
001f490          00040000        0003248d        00022a2f        0001264b
001f4a0          0003248d        00045bf2        00041b33        0003d979
001f4b0          00036799        0002accf        0001d7b5        0000f079
001f4c0          00024147        000360c3        00032e96        0002dd1c
001f4d0          00026f75        0001e9d9        00015de9        0000b262
001f4e0          0001499c        0001c92e        0001cd6a        00019f43
001f4f0          00016127        00011eb8        0000c57f        00006b2d
001f500          00038000        00052876        0004dbd9        0004a148
001f510          0003f000        000317fb        00023fd5        0001258f
001f520          0005c3cf        0007ff02        0007edba        000722b4
001f530          0006117c        0004c491        000348bd        0001c209
001f540          00060085        00085336        0007d78f        00070ec1
001f550          00060085        0004f0b9        00036728        0001bc1c
001f560          0005a8ae        0007d960        000764ca        0006a777
001f570          0005ea87        0004a5f5        000333a8        0001a1db
001f580          00050800        0006fa82        000692da        0005ea87
001f590          00054000        00041ff9        0002d75e        00018242
001f5a0          00041ff9        0005b8ae        000563b2        00050d6e
001f5b0          000477f9        000382d0        00026b1d        00013b9f
001f5c0          0002f5ad        00046f00        00042d25        0003c235
001f5d0          0003324a        000282ed        0001cb42        0000ea21
001f5e0          0001b09c        0002580c        00025d9b        00022108
001f5f0          0001cf83        00017851        00010336        00008cac
001f600          00040000        0005e519        00058d8a        00054a9c
001f610          00048000        0003891f        00029218        00014f7e
001f620          000696a4        00092370        00090fb0        000827aa
001f630          0006ef69        000572ee        0003c0d8        00020254
001f640          0006dc05        000983ac        0008f65a        000810dd
001f650          0006dc05        0005a565        0003e39b        0001fb8e
001f660          000677a2        0008f86e        00087330        00079ad1
001f670          0006c2e4        00054ff3        0003a8c0        0001dd8d
001f680          0005c000        0007f9b9        00078343        0006c2e4
001f690          00060000        0004b6d4        00033f47        0001b970
001f6a0          0004b6d4        000689eb        000628cc        0005c635
001f6b0          00051b65        00040337        0002c38f        000168b6
001f6c0          000361ea        00051124        0004c5e1        00044baa
001f6d0          0003a730        0002dec6        00020cdd        00010b93
001f6e0          0001ee69        0002adc5        0002b41f        00026ee5
001f6f0          000211ba        0001ae14        0001283e        0000a0c4
001f700          00050000        00075e60        0006f0ed        00069d43
001f710          0005a000        00046b67        0003369e        0001a35e
001f720          00083c4d        000b6c4c        000b539c        000a3194
001f730          0008ab44        0006cfaa        0004b10f        000282e8
001f740          00089307        000be497        000b33f1        000a1514
001f750          00089307        00070ebf        0004dc82        00027a72
001f760          0008158b        000b3689        000a8ffc        00098186
001f770          0008739c        0006a3f0        000492f0        000254f0
001f780          00073000        0009f827        00096413        0008739c
001f790          00078000        0005e489        00040f19        000227cc
001f7a0          0005e489        00082c66        0007b2ff        000737c2
001f7b0          0006623f        00050405        00037473        0001c2e3
001f7c0          00043a64        0006556d        0005f75a        00055e94
001f7d0          000490fc        00039677        00029015        00014e78
001f7e0          00026a04        00035936        00036127        00030a9e
001f7f0          00029629        00021999        0001724e        0000c8f5
001f800          00060000        0008d7a6        00085450        0007efea
001f810          0006c000        00054dae        0003db24        0001f73e
001f820          0009e1f6        000db528        000d9788        000c3b7e
001f830          000a671e        00082c66        0005a145        0003037d
001f840          000a4a08        000e4582        000d7187        000c194b
001f850          000a4a08        00087818        0005d569        0002f955
001f860          0009b373        000d74a5        000cacc8        000b683a
001f870          000a2455        0007f7ed        00057d20        0002cc54
001f880          0008a000        000bf696        000b44e4        000a2455
001f890          00090000        0007123e        0004deea        00029629
001f8a0          0007123e        0009cee0        00093d32        0008a950
001f8b0          0007a918        000604d2        00042556        00021d11
001f8c0          000512df        000799b6        000728d2        0006717f
001f8d0          00057ac8        00044e28        0003134c        0001915c
001f8e0          0002e59e        000404a7        00040e2f        0003a657
001f8f0          00031a97        0002851e        0001bc5d        0000f126
001f900          00080000        000bca33        000b1b15        000a9538
001f910          00090000        0007123e        00052430        00029efd
001f920          000d2d48        001246e0        00121f5f        00104f53
001f930          000dded2        000ae5dd        000781b1        000404a7
001f940          000db80b        00130757        0011ecb4        001021b9
001f950          000db80b        000b4acb        0007c736        0003f71d
001f960          000cef44        0011f0dc        0010e661        000f35a3
001f970          000d85c7        000a9fe7        00075180        0003bb1a
001f980          000b8000        000ff372        000f0686        000d85c7
001f990          000c0000        00096da8        00067e8e        000372e1
001f9a0          00096da8        000d13d6        000c5198        000b8c6a
001f9b0          000a36cb        0008066e        0005871e        0002d16b
001f9c0          0006c3d4        000a2248        00098bc3        00089754
001f9d0          00074e60        0005bd8b        000419bb        00021726
001f9e0          0003dcd3        00055b8a        0005683e        0004ddc9
001f9f0          00042374        00035c28        0002507c        00014188
001fa00          000c0000        0011af4c        0010a89f        000fdfd4
001fa10          000d8000        000a9b5d        0007b649        0003ee7b
001fa20          0013c3ec        001b6a50        001b2f0f        001876fd
001fa30          0014ce3c        001058cb        000b4289        000606fb
001fa40          00149410        001c8b03        001ae30e        00183296
001fa50          00149410        0010f030        000baad2        0005f2ab
001fa60          001366e6        001ae949        00195991        0016d074
001fa70          001448ab        000fefda        000afa40        000598a7
001fa80          00114000        0017ed2b        001689c8        001448ab
001fa90          00120000        000e247c        0009bdd5        00052c51
001faa0          000e247c        00139dc1        00127a63        0011529f
001fab0          000f5230        000c09a5        00084aac        00043a21
001fac0          000a25be        000f336d        000e51a4        000ce2fe
001fad0          000af590        00089c51        00062698        000322b9
001fae0          0005cb3c        0008094e        00081c5d        00074cae
001faf0          0006352e        00050a3b        000378ba        0001e24d
001fb00          00110000        00190dac        0017998c        00167d16
001fb10          00132000        000f06c3        000aece7        000591d9
001fb20          001c0039        0026d69c        002682ab        0022a891
001fb30          001d797f        00172876        000ff398        000889e3
001fb40          001d2717        00286f9a        002616ff        002247aa
001fb50          001d2717        0017feef        00108754        00086d1d
001fb60          001b7c71        00261fd3        0023e98d        002051fa
001fb70          001cbc47        001693ca        000f8d30        0007ed98
001fb80          00187000        0021e552        001feddc        001cbc47
001fb90          00198000        00140904        000dccee        0007541e
001fba0          00140904        001bca27        001a2d62        00188a62
001fbb0          0015b46f        00110daa        000bbf1f        0005fd04
001fbc0          000e6022        001588da        001448fe        00124192
001fbd0          000f868b        000c32c8        0008b6ad        00047130
001fbe0          00083540        000b6284        000b7d84        000a574b
001fbf0          0008cb57        000723d4        0004eb08        0002ab42
001fc00          00160000        00206c0c        001e8a79        001d1a59
001fc10          0018c000        0013722a        000e2385        00073537
001fc20          00243c86        003242e8        0031d646        002cda25
001fc30          002624c3        001df820        0014a4a7        000b0ccc
001fc40          0025ba1d        00345430        00314aef        002c5cbe
001fc50          0025ba1d        001f0dae        001563d6        000ae78e
001fc60          002391fb        0031565c        002e798a        0029d380
001fc70          00252fe4        001d37bb        00142021        000a4288
001fc80          001fa000        002bdd7a        002951ef        00252fe4
001fc90          00210000        0019ed8d        0011dc06        00097bea
001fca0          0019ed8d        0023f68c        0021e061        001fc224
001fcb0          001c16ae        001611ae        000f3391        0007bfe7
001fcc0          00129a87        001bde47        001a4058        0017a026
001fcd0          00141787        000fc93e        000b46c1        0005bfa8
001fce0          000a9f44        000ebbba        000edeab        000d61e9
001fcf0          000b617f        00093d6d        00065d55        00037437
001fd00          001c0000        002943b2        0026dec9        00250a43
001fd10          001f8000        0018bfd8        0011fea9        00092c75
001fd20          002e1e7c        003ff810        003f6dce        003915a3
001fd30          00308be1        00262485        001a45eb        000e1049
001fd40          00300425        004299b2        003ebc76        00387608
001fd50          00300425        002785c6        001b393f        000de0e4
001fd60          002d456e        003ecb00        003b2652        00353bba
001fd70          002f5439        00252fa8        00199d41        000d0edc
001fd80          00284000        0037d40f        003496d3        002f5439
001fd90          002a0000        0020ffcb        0016baf1        000c1213
001fda0          0020ffcb        002dc56d        002b1d93        00286b74
001fdb0          0023bfc6        001c1681        001358e8        0009dcf8
001fdc0          0017ad66        002377fe        0021692a        001e11a5
001fdd0          0019924f        00141767        000e5a0d        00075104
001fde0          000d84e2        0012c062        0012ecda        00110840
001fdf0          000e7c16        000bc28a        000819b2        0004655d
001fe00          00220000        00321b58        002f3318        002cfa2d
001fe10          00264000        001e0d86        0015d9ce        000b23b2
001fe20          00380072        004dad38        004d0556        00455122
001fe30          003af2fe        002e50eb        001fe730        001113c7
001fe40          003a4e2d        0050df33        004c2dfd        00448f54
001fe50          003a4e2d        002ffddf        00210ea8        0010da39
001fe60          0036f8e1        004c3fa5        0047d31b        0040a3f5
001fe70          0039788e        002d2795        001f1a61        000fdb2f
001fe80          0030e000        0043caa5        003fdbb7        0039788e
001fe90          00330000        00281209        001b99db        000ea83b
001fea0          00281209        0037944d        00345ac4        003114c3
001feb0          002b68df        00221b53        00177e3e        000bfa09
001fec0          001cc044        002b11b4        002891fc        00248324
001fed0          001f0d17        0018658f        00116d5a        0008e260
001fee0          00106a80        0016c509        0016fb08        0014ae97
001fef0          001196ae        000e47a8        0009d60f        00055684
001ff00          002c0000        0040d818        003d14f2        003a34b2
001ff10          00318000        0026e454        001c470a        000e6a6e
001ff20          0048790c        006485d0        0063ac8d        0059b44a
001ff30          004c4986        003bf03f        0029494d        00161998
001ff40          004b743a        0068a861        006295de        0058b97b
001ff50          004b743a        003e1b5c        002ac7ac        0015cf1d
001ff60          004723f6        0062acb8        005cf313        0053a701
001ff70          004a5fc7        003a6f75        00284041        00148510
001ff80          003f4000        0057baf3        0052a3de        004a5fc7
001ff90          00420000        0033db1a        0023b80d        0012f7d4
001ffa0          0033db1a        0047ed19        0043c0c2        003f8448
001ffb0          00382d5c        002c235d        001e6723        000f7fcf
001ffc0          0025350d        0037bc8e        003480b0        002f404c
001ffd0          00282f0e        001f927d        00168d83        000b7f50
001ffe0          00153e87        001d7775        001dbd56        001ac3d2
001fff0          0016c2ff        00127ad9        000cbaaa        0006e86e
```
