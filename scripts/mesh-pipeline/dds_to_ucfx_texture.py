#!/usr/bin/env python3
"""Uncompressed RGBA .dds -> Mercenaries 2 UCFX texture container (DXT1, fully resident).

Builds a NAME/INFO/BODY texture chunk the engine can read inline with NO streaming:
INFO[26:32]=0 (+ FFFF sentinel) marks it fully resident, and BODY is exactly the
dimension-derived DXT1 mip chain (`linear_mip_chain_size`), avoiding the
BUFFER_TOO_SMALL streaming over-read documented in mercs2_formats::texsize.

Usage: dds_to_ucfx_texture.py <in.dds> <name> <out_container.bin>
"""
import struct, sys, zlib
import numpy as np


def crc32_mercs2(d):
    return (zlib.crc32(d, 0xFFFFFFFF) ^ 0xFFFFFFFF) & 0xFFFFFFFF


def load_dds_rgb(path):
    d = open(path, "rb").read()
    h, w = struct.unpack_from("<II", d, 12)  # height, width
    px = np.frombuffer(d, dtype=np.uint8, offset=128).reshape(h, w, 4)
    # stored BGRA (masks R=00FF0000,G=0000FF00,B=000000FF) -> RGB
    rgb = px[:, :, [2, 1, 0]].astype(np.float32)
    return w, h, rgb


def box_down(img):
    h, w, _ = img.shape
    return (img[0::2, 0::2] + img[1::2, 0::2] + img[0::2, 1::2] + img[1::2, 1::2]) * 0.25


def to565(c):
    r = (int(round(c[0])) >> 3) & 0x1F
    g = (int(round(c[1])) >> 2) & 0x3F
    b = (int(round(c[2])) >> 3) & 0x1F
    return (r << 11) | (g << 5) | b


def from565(v):
    r = (v >> 11) & 0x1F
    g = (v >> 5) & 0x3F
    b = v & 0x1F
    return np.array([(r << 3) | (r >> 2), (g << 2) | (g >> 4), (b << 3) | (b >> 2)], np.float32)


def dxt1_block(block):
    """block: 4x4x3 float RGB -> 8 bytes DXT1 (opaque 4-color mode)."""
    px = block.reshape(16, 3)
    cmin = px.min(0)
    cmax = px.max(0)
    c0 = to565(cmax)
    c1 = to565(cmin)
    if c0 == c1:
        # flat block: ensure c0>c1 so we stay in 4-colour (opaque) mode
        if c1 == 0:
            c0 = 1
        else:
            c1 = c0 - 1
    if c0 < c1:
        c0, c1 = c1, c0
    e0 = from565(c0)
    e1 = from565(c1)
    pal = np.stack([e0, e1, (2 * e0 + e1) / 3.0, (e0 + 2 * e1) / 3.0])  # 4x3
    # nearest palette index per pixel
    d = ((px[:, None, :] - pal[None, :, :]) ** 2).sum(2)  # 16x4
    idx = d.argmin(1).astype(np.uint32)  # 0..3
    bits = 0
    for i in range(16):
        bits |= int(idx[i]) << (2 * i)
    return struct.pack("<HHI", c0, c1, bits)


def dxt1_compress(rgb):
    h, w, _ = rgb.shape
    out = bytearray()
    for by in range(0, h, 4):
        for bx in range(0, w, 4):
            blk = rgb[by:by + 4, bx:bx + 4]
            if blk.shape[0] != 4 or blk.shape[1] != 4:
                tmp = np.zeros((4, 4, 3), np.float32)
                tmp[:blk.shape[0], :blk.shape[1]] = blk
                blk = tmp
            out += dxt1_block(blk)
    return bytes(out)


def dxt_mip_count(w, h):
    m = min(w, h)
    return max(1, (m.bit_length()) - 2)


def main():
    src, name, out = sys.argv[1], sys.argv[2], sys.argv[3]
    w, h, rgb = load_dds_rgb(src)
    mips = dxt_mip_count(w, h)
    body = bytearray()
    cur = rgb
    cw, ch = w, h
    for m in range(mips):
        body += dxt1_compress(cur)
        if m < mips - 1:
            cur = box_down(cur)
            cw //= 2; ch //= 2
    body = bytes(body)
    print(f"{src}: {w}x{h} -> DXT1 {mips} mips, BODY={len(body)} bytes")

    name_b = name.encode() + b"\x00"
    while len(name_b) % 2 != 0:
        name_b += b"\x00"
    info = bytearray(34)
    struct.pack_into("<HHHHHHH", info, 0, w, h, 1, mips, 0, 1, 1)
    info[14:18] = b"DXT1"
    struct.pack_into("<I", info, 22, len(body))     # total_size
    # info[26:32] left 0 = fully resident
    struct.pack_into("<H", info, 32, 0xFFFF)        # resident sentinel

    # container: header(20) + 3 rows(60) + data(NAME|INFO|BODY) + CSUM
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
