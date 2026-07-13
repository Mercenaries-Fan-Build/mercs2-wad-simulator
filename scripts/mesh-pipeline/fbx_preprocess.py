#!/usr/bin/env python3
"""Blender headless preprocess: asset-store FBX -> engine-ready universal mesh.

Raw asset-store / Sketchfab models are millions of polys, multi-material, PBR, and
carry UNBAKED FBX node transforms (so `fbx_reader.py`, which reads raw Geometry
verts, piles every part at the origin). This stage fixes all of that with Blender:

  import FBX (bakes node transforms) -> join all meshes -> triangulate ->
  decimate to a triangle budget -> unweld to per-corner verts (pos/uv/normal) ->
  convert Blender Z-up(RH) to engine Y-up -> write the npz `gltf_to_ucfx_model.py`
  wants (pos[N,3] f32, nrm[N,3], uv[N,2], tris[M,3]) + a preview .glb.

Run:
  "<blender>" -b -P tools/fbx_preprocess.py -- <in.fbx> <out.npz> [--tris N] [--glb out.glb]

Notes
- --tris caps the *triangle* count. Keep it small: `gltf_to_ucfx_model.py`'s
  degenerate strip uses ~6 indices/tri and the strip is u16, so tris must stay
  under ~10,900 (default 9000).
- Single merged mesh only (the prototype game converter is single-material). Bake
  a texture atlas separately if you need the source's own materials.
"""
import sys, math
import bpy
import numpy as np


def argv_after_ddash():
    a = sys.argv
    return a[a.index("--") + 1:] if "--" in a else []


def main():
    args = argv_after_ddash()
    if len(args) < 2:
        print("usage: ... -- <in.fbx> <out.npz> [--tris N] [--glb path]")
        sys.exit(2)
    in_fbx, out_npz = args[0], args[1]
    tris_budget = 9000
    glb_out = None
    i = 2
    while i < len(args):
        if args[i] == "--tris":
            tris_budget = int(args[i + 1]); i += 2
        elif args[i] == "--glb":
            glb_out = args[i + 1]; i += 2
        else:
            i += 1

    # Clean scene
    bpy.ops.wm.read_factory_settings(use_empty=True)
    ext = in_fbx.lower().rsplit(".", 1)[-1]
    if ext == "obj":
        # Blender 4.x/5.x native OBJ importer.
        bpy.ops.wm.obj_import(filepath=in_fbx)
    elif ext in ("gltf", "glb"):
        bpy.ops.import_scene.gltf(filepath=in_fbx)
    else:
        bpy.ops.import_scene.fbx(filepath=in_fbx)

    meshes = [o for o in bpy.context.scene.objects if o.type == "MESH"]
    if not meshes:
        print("no mesh objects imported"); sys.exit(1)
    print(f"imported {len(meshes)} mesh object(s)")

    # Apply transforms so world layout is baked into geometry.
    for o in bpy.context.scene.objects:
        o.select_set(False)
    for o in meshes:
        o.select_set(True)
    bpy.context.view_layer.objects.active = meshes[0]
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)

    # Join into one object.
    bpy.ops.object.join()
    obj = bpy.context.view_layer.objects.active
    me = obj.data

    # Drop shape keys (facial morphs) — they block the decimate modifier and we
    # only want the base (bind) geometry.
    if me.shape_keys:
        obj.shape_key_clear()

    # Triangulate.
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.mesh.quads_convert_to_tris(quad_method="BEAUTY", ngon_method="BEAUTY")
    bpy.ops.object.mode_set(mode="OBJECT")

    ntris = len(me.polygons)
    print(f"joined mesh: {len(me.vertices)} verts, {ntris} tris")

    # Decimate to budget.
    if ntris > tris_budget:
        ratio = max(0.001, tris_budget / ntris)
        mod = obj.modifiers.new("dec", "DECIMATE")
        mod.ratio = ratio
        bpy.ops.object.modifier_apply(modifier=mod.name)
        me = obj.data
        # re-triangulate any ngons decimate may have produced
        bpy.ops.object.mode_set(mode="EDIT")
        bpy.ops.mesh.select_all(action="SELECT")
        bpy.ops.mesh.quads_convert_to_tris(quad_method="BEAUTY", ngon_method="BEAUTY")
        bpy.ops.object.mode_set(mode="OBJECT")
        me = obj.data
        print(f"decimated -> {len(me.polygons)} tris (ratio {ratio:.4f})")

    if glb_out:
        bpy.ops.export_scene.gltf(filepath=glb_out, export_format="GLB",
                                  use_selection=True)
        print(f"wrote preview {glb_out}")

    # Unweld to per-corner verts; convert Blender Z-up(RH) -> engine Y-up.
    me.calc_loop_triangles()
    uv_layer = me.uv_layers.active.data if me.uv_layers.active else None
    try:
        me.calc_normals_split()  # <4.x; no-op path on 5.x handled below
    except Exception:
        pass
    corner_normals = None
    try:
        corner_normals = me.corner_normals  # Blender 4.1+/5.x
    except Exception:
        corner_normals = None

    pos, nrm, uv, tris = [], [], [], []
    dedup = {}

    def gnorm(loop_index, vidx):
        if corner_normals is not None:
            v = corner_normals[loop_index].vector
        else:
            v = me.loops[loop_index].normal
        return (v.x, v.y, v.z)

    def to_engine(co):
        # Blender (x, y, z; Z up, RH) -> engine (x, z, -y; Y up)
        return (co.x, co.z, -co.y)

    def to_engine_n(n):
        return (n[0], n[2], -n[1])

    for lt in me.loop_triangles:
        tri = []
        for k in range(3):
            loop_index = lt.loops[k]
            vidx = lt.vertices[k]
            p = to_engine(me.vertices[vidx].co)
            n = to_engine_n(gnorm(loop_index, vidx))
            if uv_layer:
                u = tuple(uv_layer[loop_index].uv)
            else:
                u = (0.0, 0.0)
            key = (round(p[0], 5), round(p[1], 5), round(p[2], 5),
                   round(n[0], 4), round(n[1], 4), round(n[2], 4),
                   round(u[0], 5), round(u[1], 5))
            idx = dedup.get(key)
            if idx is None:
                idx = len(pos)
                dedup[key] = idx
                pos.append(p); nrm.append(n); uv.append(u)
            tri.append(idx)
        tris.append(tri)

    pos = np.array(pos, dtype=np.float32)
    nrm = np.array(nrm, dtype=np.float32)
    uv = np.array(uv, dtype=np.float32)
    tris = np.array(tris, dtype=np.int32)
    print(f"unwelded: verts={len(pos)} tris={len(tris)} "
          f"bbox={pos.min(0).round(3).tolist()}..{pos.max(0).round(3).tolist()}")
    if len(pos) >= 65535:
        print(f"WARNING: {len(pos)} verts >= 65535 (u16 limit); lower --tris")
    if len(tris) * 6 >= 65535:
        print(f"WARNING: strip ~{len(tris)*6} >= 65535 (u16 limit); lower --tris")

    np.savez(out_npz, pos=pos, nrm=nrm, uv=uv, tris=tris)
    print(f"wrote {out_npz}")


main()
