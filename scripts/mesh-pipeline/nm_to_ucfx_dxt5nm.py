#!/usr/bin/env python3
"""Normal map -> Mercenaries 2 UCFX texture container, DXT5nm (fully resident).

★Mercs2 stores a model's normal map (MTRL texture SLOT 2) as **DXT5nm**, NOT as an RGB normal:

    normal.x = ALPHA channel
    normal.y = the colour channels (which are GREYSCALE - R==G==B)
    normal.z = sqrt(1 - x^2 - y^2)   reconstructed in the shader

Proven by decoding the ztz98's own slot-2 texture (0xE1F66E9B, DXT5 1024^2): under this packing
x mean = -0.004, y mean = -0.002, **100.00%** of texels satisfy x^2+y^2 <= 1, and the reconstructed
z averages 0.986. Its R and B endpoint statistics are byte-identical, i.e. the colour is greyscale --
DXT5 is being used to carry two independent channels.

Ship an RGB normal map here (e.g. a DXT1 one) and DXT1's implicit alpha=255 makes normal.x = 1.0, so
every normal points along the tangent: lighting explodes and a matte black tank renders WHITE.

Slot 1 is the SPECULAR map (DXT1, INFO byte[8] = 0x20) -- see docs/format_reference.md.

Usage: nm_to_ucfx_dxt5nm.py <normal.png|jpg> <name> <out_container.bin> [--size 1024] [--invert-g]
"""
import struct, sys, zlib
import numpy as np
from PIL import Image


def crc32_mercs2(d):
    return (zlib.crc32(d, 0xFFFFFFFF) ^ 0xFFFFFFFF) & 0xFFFFFFFF


def box_down(a):
    return (a[0::2, 0::2] + a[1::2, 0::2] + a[0::2, 1::2] + a[1::2, 1::2]) * 0.25


def dxt5nm_compress(x, y):
    """x, y: float32 HxW in [0,255]. x -> alpha block, y -> greyscale colour block."""
    h, w = x.shape
    out = bytearray()
    for by in range(0, h, 4):
        for bx in range(0, w, 4):
            a = x[by:by + 4, bx:bx + 4].ravel()
            g = y[by:by + 4, bx:bx + 4].ravel()

            # ---- alpha block (8-value mode: a0 > a1) ----
            a0, a1 = int(round(a.max())), int(round(a.min()))
            if a0 == a1:
                a0 = min(255, a1 + 1)
            pal = np.array([a0, a1] + [((7 - i) * a0 + i * a1) // 7 for i in range(1, 7)], dtype=np.float32)
            ai = np.abs(a[:, None] - pal[None, :]).argmin(1).astype(np.uint64)
            bits = 0
            for k in range(16):
                bits |= int(ai[k]) << (3 * k)
            out += bytes([a0, a1]) + bits.to_bytes(6, "little")

            # ---- colour block: greyscale y, 4-colour mode (c0 > c1) ----
            g0, g1 = int(round(g.max())), int(round(g.min()))
            c0 = ((g0 >> 3) << 11) | ((g0 >> 2) << 5) | (g0 >> 3)
            c1 = ((g1 >> 3) << 11) | ((g1 >> 2) << 5) | (g1 >> 3)
            if c0 <= c1:                       # 4-colour mode requires c0 > c1
                c0 = min(0xFFFF, c1 + 1)
            q0 = ((c0 >> 5) & 63) * 255.0 / 63.0   # decoded green of each endpoint
            q1 = ((c1 >> 5) & 63) * 255.0 / 63.0
            cpal = np.array([q0, q1, (2 * q0 + q1) / 3.0, (q0 + 2 * q1) / 3.0], dtype=np.float32)
            ci = np.abs(g[:, None] - cpal[None, :]).argmin(1).astype(np.uint32)
            cb = 0
            for k in range(16):
                cb |= int(ci[k]) << (2 * k)
            out += struct.pack("<HHI", c0, c1, cb)
    return bytes(out)


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    size = 1024
    if "--size" in sys.argv:
        size = int(sys.argv[sys.argv.index("--size") + 1])
    invert_g = "--invert-g" in sys.argv
    src, name, out = args[0], args[1], args[2]

    im = Image.open(src).convert("RGB").resize((size, size), Image.LANCZOS)
    a = np.asarray(im, dtype=np.float32)
    X = a[:, :, 0]                     # normal.x  -> alpha
    Y = 255.0 - a[:, :, 1] if invert_g else a[:, :, 1]   # normal.y -> greyscale colour

    w = h = size
    mips = max(1, min(w, h).bit_length() - 2)
    body = bytearray()
    cx, cy = X, Y
    for m in range(mips):
        body += dxt5nm_compress(cx, cy)
        if m < mips - 1:
            cx = box_down(cx[:, :, None])[:, :, 0]
            cy = box_down(cy[:, :, None])[:, :, 0]
    body = bytes(body)
    print(f"{src}: {w}x{h} -> DXT5nm {mips} mips, BODY={len(body)} bytes")

    name_b = name.encode() + b"\x00"
    while len(name_b) % 2 != 0:
        name_b += b"\x00"
    info = bytearray(34)
    struct.pack_into("<HHHHHHH", info, 0, w, h, 1, mips, 0, 1, 1)
    info[14:18] = b"DXT5"              # ★DXT5, matching the donor's slot-2 format
    struct.pack_into("<I", info, 22, len(body))
    struct.pack_into("<H", info, 32, 0xFFFF)   # fully resident

    rows = [(b"NAME", len(name_b), 2), (b"INFO", 34, 1), (b"BODY", len(body), 0)]
    data_off = 20 + 3 * 20
    blob = bytearray()
    placed = []
    for tag, sz, u2 in rows:
        while len(blob) % 4 != 0:
            blob.append(0)
        placed.append((tag, len(blob), sz, u2))
        blob += {b"NAME": name_b, b"INFO": bytes(info), b"BODY": body}[tag]

    c = bytearray()
    c += b"UCFX" + struct.pack("<IIII", data_off, 0, 0, 3)
    for tag, off, sz, u2 in placed:
        c += tag + struct.pack("<IIII", off, sz, u2, 0)
    c += blob
    c += b"CSUM" + struct.pack("<I", crc32_mercs2(bytes(c)))
    open(out, "wb").write(c)
    print(f"wrote {out} ({len(c)} bytes)")


if __name__ == "__main__":
    main()
