#!/usr/bin/env python3
"""Universal mesh (npz: pos/nrm/uv/tris) -> Mercenaries 2 UCFX model container.

Regenerates a game model FAITHFULLY from a universal triangle mesh: builds a u16
triangle STRIP (degenerate-joined, matching the engine's IBUF), computes the
per-triangle f16 AREA chunk, encodes the STRM vertex buffer (stride 20:
f16 pos@0, f16x2 UV@8, f16x4 normal@12), and rewrites PRMG bounds + PRMT draw
records. All non-geometry chunks of the reference container (INFO/HIER/MTRL/
SEGM/PHY2/STAM/INDX/decl) are preserved byte-for-byte; only the two meshes'
STRM/IBUF/AREA/PRMG-INFO/PRMT leaves change. The container is then re-packed
(data area re-emitted, every descriptor offset recomputed) and the CSUM trailer
recomputed.

This is a prototype of the universal->game half of the asset import pipeline.
"""
import struct, sys, math
import numpy as np


def crc32_mercs2(data: bytes) -> int:
    return (__import__("zlib").crc32(data, 0xFFFFFFFF) ^ 0xFFFFFFFF) & 0xFFFFFFFF


def f16(x: float) -> bytes:
    return struct.pack("<e", max(-65504.0, min(65504.0, float(x))))


# ---- triangle strip (degenerate-joined), with a decode verifier ------------
def to_strip(tris):
    s = []
    for (a, b, c) in tris:
        if not s:
            s = [a, b, c]
            continue
        z = s[-1]
        # Fully-degenerate bridge: ...,z,z,a,a,(a),b,c. Every bridge triple shares
        # a repeated vertex, so only (a,b,c) is a real triangle. The optional extra
        # 'a' forces (a,b,c) to start at an even index (correct strip winding).
        s.append(z)
        s.append(a)
        s.append(a)
        if len(s) % 2 == 0:
            s.append(a)
        s.append(b)
        s.append(c)
    return s


def strip_to_tris(s):
    out = []
    for k in range(len(s) - 2):
        a, b, c = s[k], s[k + 1], s[k + 2]
        if a == b or b == c or a == c:
            continue
        out.append((a, b, c) if k % 2 == 0 else (a, c, b))
    return out


def tri_area(p, a, b, c):
    u = p[b] - p[a]
    v = p[c] - p[a]
    return 0.5 * float(np.linalg.norm(np.cross(u, v)))


# ---- UCFX container parse / re-pack ----------------------------------------
def parse_rows(cont):
    data_off = struct.unpack_from("<I", cont, 4)[0]
    ndesc = struct.unpack_from("<I", cont, 16)[0]
    rows = []
    for d in range(ndesc):
        ro = 20 + d * 20
        tag = cont[ro:ro + 4]
        u0, sz, u2, u3 = struct.unpack_from("<IIII", cont, ro + 4)
        rows.append([tag, u0, sz, u2, u3])
    return data_off, ndesc, rows


def leaf_body(cont, data_off, u0, sz):
    s = data_off + u0
    return cont[s:s + sz]


def find_mesh_leaves(rows):
    """Return list of dicts per PRMG mesh: row indices of INFO(60),strm info/data,
    area info/data, ibuf info/data, prmt."""
    meshes = []
    i = 0
    cur = None
    state = None
    while i < len(rows):
        tag, u0, sz, u2, u3 = rows[i]
        cont_marker = (u0 == 0xFFFFFFFF)
        if tag == b"PRMG" and cont_marker:
            cur = {"prmg_info": None, "strm_info": None, "strm_data": None,
                   "area_info": None, "area_data": None, "ibuf_info": None,
                   "ibuf_data": None, "prmt": None}
            meshes.append(cur)
            state = None
        elif cur is not None:
            if tag == b"INFO" and sz == 60 and not cont_marker:
                cur["prmg_info"] = i
            elif tag == b"STRM" and cont_marker:
                state = "STRM"
            elif tag == b"AREA" and cont_marker:
                state = "AREA"
            elif tag == b"IBUF" and cont_marker:
                state = "IBUF"
            elif tag == b"PRMT" and not cont_marker:
                cur["prmt"] = i
            elif tag == b"info" and not cont_marker:
                if state == "STRM":
                    cur["strm_info"] = i
                elif state == "AREA":
                    cur["area_info"] = i
                elif state == "IBUF":
                    cur["ibuf_info"] = i
            elif tag == b"data" and not cont_marker:
                if state == "STRM":
                    cur["strm_data"] = i
                elif state == "AREA":
                    cur["area_data"] = i
                elif state == "IBUF":
                    cur["ibuf_data"] = i
        i += 1
    return meshes


def build_mesh_buffers(pos, nrm, uv, tris):
    """Encode STRM (stride 20), IBUF strip (u16), AREA (f16 per tri)."""
    strip = to_strip([tuple(t) for t in tris])
    # verify the strip reproduces the triangle set (winding-insensitive)
    got = {tuple(sorted(t)) for t in strip_to_tris(strip)}
    want = {tuple(sorted(map(int, t))) for t in tris}
    assert got == want, f"strip mismatch: {len(got)} vs {len(want)} triangles"
    assert max(strip) < 65535 and len(strip) < 65535, "exceeds u16"

    vb = bytearray()
    for i in range(len(pos)):
        vb += f16(pos[i, 0]) + f16(pos[i, 1]) + f16(pos[i, 2]) + b"\x00\x00"
        vb += f16(uv[i, 0]) + f16(uv[i, 1])
        vb += f16(nrm[i, 0]) + f16(nrm[i, 1]) + f16(nrm[i, 2]) + b"\x00\x00"
    ib = b"".join(struct.pack("<H", x) for x in strip)
    area = bytearray()
    for k in range(len(strip) - 2):
        a, b, c = strip[k], strip[k + 1], strip[k + 2]
        ar = 0.0 if (a == b or b == c or a == c) else tri_area(pos, a, b, c)
        area += f16(ar)
    return bytes(vb), ib, bytes(area), len(strip), len(pos)


def main():
    src_container = sys.argv[1]   # reference crate model container (raw bytes)
    npz = sys.argv[2]
    out = sys.argv[3]
    cont = open(src_container, "rb").read()
    m = np.load(npz)
    pos, nrm, uv, tris = m["pos"], m["nrm"], m["uv"], m["tris"]

    # Map the universal mesh into the reference model's vertex space (so the
    # in-world size/placement matches the crate it replaces).
    data_off, ndesc, rows = parse_rows(cont)
    meshes = find_mesh_leaves(rows)
    assert meshes, "no PRMG meshes found"
    # reference vertex bbox (decode all mesh STRM positions)
    allv = []
    for mh in meshes:
        si = rows[mh["strm_info"]]; sd = rows[mh["strm_data"]]
        a, stride, count = struct.unpack("<III", leaf_body(cont, data_off, si[1], si[2]))
        body = leaf_body(cont, data_off, sd[1], sd[2])
        for v in range(count):
            o = v * stride
            allv.append([struct.unpack_from("<e", body, o)[0],
                         struct.unpack_from("<e", body, o + 2)[0],
                         struct.unpack_from("<e", body, o + 4)[0]])
    allv = np.array(allv)
    tgt_min, tgt_max = allv.min(0), allv.max(0)
    src_min, src_max = pos.min(0), pos.max(0)
    # uniform scale (preserve cube proportions) + center into target box
    scale = float(np.min((tgt_max - tgt_min) / np.maximum(src_max - src_min, 1e-6)))
    tgt_c = (tgt_min + tgt_max) / 2
    src_c = (src_min + src_max) / 2
    mpos = ((pos - src_c) * scale + tgt_c).astype(np.float32)
    bmin, bmax = mpos.min(0), mpos.max(0)
    print(f"mapped bbox -> min={bmin.round(3).tolist()} max={bmax.round(3).tolist()}")

    vb, ib, area, strip_len, vcount = build_mesh_buffers(mpos, nrm, uv, tris)
    print(f"strip indices={strip_len} (tris={strip_len-2}) verts={vcount} "
          f"VB={len(vb)}B IB={len(ib)}B AREA={len(area)}B")

    # New leaf bodies: put the same cube into both meshes.
    new_bodies = {}  # row_index -> bytes
    for mh in meshes:
        # STRM info: keep flag, set stride=20, count
        si = rows[mh["strm_info"]]
        flag = struct.unpack("<III", leaf_body(cont, data_off, si[1], si[2]))[0]
        new_bodies[mh["strm_info"]] = struct.pack("<III", flag, 20, vcount)
        new_bodies[mh["strm_data"]] = vb
        new_bodies[mh["ibuf_info"]] = struct.pack("<I", strip_len)
        new_bodies[mh["ibuf_data"]] = ib
        new_bodies[mh["area_info"]] = struct.pack("<I", strip_len - 2)
        new_bodies[mh["area_data"]] = area
        # PRMG INFO: keep first 20 bytes, rewrite bounds (center,radius,min,max)
        pi = rows[mh["prmg_info"]]
        body = bytearray(leaf_body(cont, data_off, pi[1], pi[2]))
        cx, cy, cz = ((bmin + bmax) / 2).tolist()
        r = float(np.linalg.norm((bmax - bmin) / 2))
        struct.pack_into("<10f", body, 20, cx, cy, cz, r,
                         bmin[0], bmin[1], bmin[2], bmax[0], bmax[1], bmax[2])
        new_bodies[mh["prmg_info"]] = bytes(body)
        # PRMT: PRESERVE the donor's record COUNT (destructible/LOD models carry
        # twin/multi sub-records per group; the destruction state machine iterates
        # them, so dropping to one makes it read off the end into CSUM — the
        # 0x00478E43 crash). Rewrite each sub-record to draw our whole strip,
        # keeping that record's own material. Faithful for identical-twin donors;
        # for genuinely-partitioned pieces both states then show the full mesh
        # (no break animation) but load cleanly.
        pr = rows[mh["prmt"]]
        rec_bytes = leaf_body(cont, data_off, pr[1], pr[2])
        n_prmt = max(1, len(rec_bytes) // 16)
        buf = bytearray()
        for k in range(n_prmt):
            mat_k = struct.unpack_from("<I", rec_bytes, k * 16)[0]
            buf += struct.pack("<IIHHHH", mat_k, 0, strip_len, 0, vcount - 1, vcount)
        new_bodies[mh["prmt"]] = bytes(buf)

    # Re-emit the data area: walk rows in order, place each leaf body (new or
    # original) 16-byte aligned, recompute u0; keep container-marker rows.
    new_data = bytearray()
    for idx, row in enumerate(rows):
        tag, u0, sz, u2, u3 = row
        if u0 == 0xFFFFFFFF:
            continue
        body = new_bodies.get(idx, leaf_body(cont, data_off, u0, sz))
        while len(new_data) % 16 != 0:
            new_data.append(0)
        row[1] = len(new_data)   # new u0
        row[2] = len(body)       # new size
        new_data += body

    new_data_off = 20 + ndesc * 20
    out_c = bytearray()
    out_c += b"UCFX"
    out_c += struct.pack("<I", new_data_off)
    out_c += cont[8:16]          # preserve u8/u12
    out_c += struct.pack("<I", ndesc)
    for row in rows:
        out_c += row[0] + struct.pack("<IIII", row[1], row[2], row[3], row[4])
    out_c += new_data
    # CSUM trailer
    out_c += b"CSUM" + struct.pack("<I", crc32_mercs2(bytes(out_c)))
    open(out, "wb").write(out_c)
    print(f"wrote {out} ({len(out_c)} bytes, was {len(cont)})")


if __name__ == "__main__":
    main()
