#!/usr/bin/env python3
"""Convert a preprocessed npz (pos/nrm/uv/tris) to the raw MESH blob that
`model_forge` reads, with optional uniform scale + target-size normalisation.

MESH blob (LE): "MESH" | u32 nverts | u32 ntris |
  pos f32[3*nv] | nrm f32[3*nv] | uv f32[2*nv] | tris u32[3*nt]

Usage:
  npz_to_mesh.py <in.npz> <out.mesh> [--scale S] [--target-height H]
    --scale S          multiply all positions by S (e.g. 0.01 for cm->m)
    --target-height H  scale uniformly so the model's Y extent == H metres
                       (overrides --scale). Also re-grounds min-Y to 0.
"""
import argparse
import struct
import numpy as np


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("npz")
    ap.add_argument("out")
    ap.add_argument("--scale", type=float, default=1.0)
    ap.add_argument("--target-height", type=float, default=None)
    ap.add_argument("--ground", action="store_true", help="translate so min-Y = 0")
    a = ap.parse_args()

    m = np.load(a.npz)
    pos = m["pos"].astype(np.float64)
    nrm = m["nrm"].astype(np.float64)
    uv = m["uv"].astype(np.float64)
    tris = m["tris"].astype(np.uint32)

    if a.target_height is not None:
        ext_y = float(pos[:, 1].max() - pos[:, 1].min())
        s = a.target_height / ext_y if ext_y > 1e-6 else 1.0
        pos *= s
        print(f"scaled by {s:.5f} to target height {a.target_height} m (was {ext_y:.2f} u)")
    elif a.scale != 1.0:
        pos *= a.scale
        print(f"scaled by {a.scale}")

    if a.ground or a.target_height is not None:
        pos[:, 1] -= pos[:, 1].min()  # feet on ground

    pos = pos.astype("<f4")
    nrm = nrm.astype("<f4")
    uv = uv.astype("<f4")
    with open(a.out, "wb") as f:
        f.write(b"MESH")
        f.write(struct.pack("<II", len(pos), len(tris)))
        f.write(pos.tobytes())
        f.write(nrm.tobytes())
        f.write(uv.tobytes())
        f.write(tris.astype("<u4").tobytes())
    bmin = pos.min(0).round(3).tolist()
    bmax = pos.max(0).round(3).tolist()
    print(f"wrote {a.out}: {len(pos)} verts {len(tris)} tris bbox {bmin}..{bmax}")


if __name__ == "__main__":
    main()
