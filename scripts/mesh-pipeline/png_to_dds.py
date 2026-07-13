#!/usr/bin/env python3
"""PNG/JPEG -> uncompressed RGBA .dds, resized to a game-sane power-of-two.

Feeds `tools/dds_to_ucfx_texture.py`, which expects an uncompressed BGRA .dds (128-byte
header, pixels at offset 128) and does the DXT1 + mip-chain encode itself.

Asset-store models ship 4096^2 PBR sheets; Mercenaries 2 skins are 512-1024 (the UH1's
body sheet is 1024^2 BC1). Shipping 4K would blow the texture pool (5120-cell cap) for
no visible gain at gameplay distance.

Also handles the PBR->legacy conversion the engine needs: Mercs2 predates metallic-
roughness and wants diffuse/normal/SPECULAR. glTF packs roughness in G and metallic in
B, so `--from-mr` derives a specular sheet as (1 - roughness), brightened by metallic.

Usage:
  png_to_dds.py <in.png> <out.dds> [--size 1024] [--from-mr] [--invert-g]
"""
import struct
import sys

import numpy as np
from PIL import Image

DDSD_CAPS, DDSD_HEIGHT, DDSD_WIDTH, DDSD_PIXELFORMAT = 0x1, 0x2, 0x4, 0x1000
DDPF_ALPHAPIXELS, DDPF_RGB = 0x1, 0x40
DDSCAPS_TEXTURE = 0x1000


def write_dds_rgba(path, img):
    """Uncompressed A8R8G8B8 (BGRA byte order), the layout dds_to_ucfx_texture.py reads."""
    h, w = img.shape[:2]
    hdr = bytearray(128)
    hdr[0:4] = b"DDS "
    struct.pack_into("<I", hdr, 4, 124)  # header size
    struct.pack_into("<I", hdr, 8, DDSD_CAPS | DDSD_HEIGHT | DDSD_WIDTH | DDSD_PIXELFORMAT)
    struct.pack_into("<I", hdr, 12, h)
    struct.pack_into("<I", hdr, 16, w)
    struct.pack_into("<I", hdr, 76, 32)  # pixelformat size
    struct.pack_into("<I", hdr, 80, DDPF_RGB | DDPF_ALPHAPIXELS)
    struct.pack_into("<I", hdr, 88, 32)  # bit count
    struct.pack_into("<I", hdr, 92, 0x00FF0000)  # R mask
    struct.pack_into("<I", hdr, 96, 0x0000FF00)  # G
    struct.pack_into("<I", hdr, 100, 0x000000FF)  # B
    struct.pack_into("<I", hdr, 104, 0xFF000000)  # A
    struct.pack_into("<I", hdr, 108, DDSCAPS_TEXTURE)
    bgra = img[:, :, [2, 1, 0, 3]]  # RGBA -> BGRA
    with open(path, "wb") as f:
        f.write(hdr)
        f.write(bgra.astype(np.uint8).tobytes())


def main():
    if len(sys.argv) < 3:
        print(__doc__)
        sys.exit(2)
    src, out = sys.argv[1], sys.argv[2]
    size = 1024
    from_mr = "--from-mr" in sys.argv
    invert_g = "--invert-g" in sys.argv
    if "--size" in sys.argv:
        size = int(sys.argv[sys.argv.index("--size") + 1])

    im = Image.open(src).convert("RGBA")
    im = im.resize((size, size), Image.LANCZOS)
    a = np.asarray(im).astype(np.float32)

    if from_mr:
        # glTF metallic-roughness: G = roughness, B = metallic. Mercs2 wants a specular
        # sheet: shinier = less rough; metal reads brighter.
        rough = a[:, :, 1] / 255.0
        metal = a[:, :, 2] / 255.0
        spec = np.clip((1.0 - rough) * (0.35 + 0.65 * metal), 0.0, 1.0) * 255.0
        a = np.dstack([spec, spec, spec, np.full_like(spec, 255.0)])
    elif invert_g:
        # Some engines expect the opposite normal-map green channel.
        a[:, :, 1] = 255.0 - a[:, :, 1]

    write_dds_rgba(out, a)
    print(f"{src} -> {out}  ({size}x{size} RGBA{' [MR->spec]' if from_mr else ''})")


main()
