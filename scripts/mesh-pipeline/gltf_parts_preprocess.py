#!/usr/bin/env python3
"""Blender headless: asset-store glTF -> one engine mesh PER MATERIAL, decimated.

Why this exists alongside `fbx_preprocess.py`: that stage JOINS every object into a
single mesh with a single material. That is wrong for a vehicle for two reasons:

  1. Mercs2 draws ONE MATERIAL PER PRMG GROUP (each PRMT sub-strip carries its own
     material — vehicle_model_spec.md §6). Joining collapses the source's 5 materials
     into one, so the gear/glass/rotor would all be sampled through the body's texture.
  2. A vehicle's moving parts are addressed BY HIER NODE. The rotor must be its own
     draw group so its SEGM row can bind to the rotor node — that is what makes
     `BoneCtrlLocalRotation` spin the blades. Joined into the body, it can never move.

So: group objects by material, decimate EACH part to its own budget, and emit one
`.mesh` blob per part. The conform step (`inject_static`) injects each into its own
PRMG group and sets that group's SEGM row (node + LOD mask).

Budgets matter: `to_strip` emits a degenerate-joined u16 strip (~3-6 indices/tri), so
EACH part must stay well under ~10,900 tris. Source hubs are absurd — this model's
`Rotor_Hub` is 24,072 tris of linkage detail that is invisible 5 m up under the blades.

Run:
  "<blender>" -b -P tools/gltf_parts_preprocess.py -- <in.gltf> <outdir> \
      [--budget <material>=<tris>]... [--default-tris N]

Emits <outdir>/<material>.mesh, each:
  "MESH" | u32 nv | u32 nt | nv*(3f32 pos) | nv*(3f32 nrm) | nv*(2f32 uv) | nt*(3u32 tri)
in ENGINE space (Y-up), the same frame + blob format `inject_static` consumes.
"""
import struct
import sys
import os

import bpy
import mathutils


def argv_after_ddash():
    a = sys.argv
    return a[a.index("--") + 1:] if "--" in a else []


# Axis convention. FBX arrives Z-up, so fbx_preprocess.py maps (x, z, -y) -> engine Y-up.
# glTF is ALREADY Y-up (this source's raw bbox is width x height x length = X x Y x Z), and
# Blender's importer hands it back in that frame, so applying the Z-up mapping would tip the
# model on its side (a rotor disc came out thin in Z instead of thin in Y). Verify against the
# emitted bbox: a heli must read WIDE in X (rotor span), SHORT in Y (height), LONG in Z.
AXIS = "yup"
FLIP_V = True  # --no-flip-v to disable
# `--planar <degrees>`: PLANAR (dissolve) pre-decimation before the collapse pass.
#
# ★Essential for hard-surface sculpts. A 2.8M-poly tank is mostly FLAT armour plates carrying
# millions of coplanar triangles. Collapsing straight to a ~9.5k budget is a ~1% ratio and the
# quadric collapse chews the silhouette into blocky mush. Planar dissolve first merges coplanar
# faces — which costs almost NO shape — typically taking 840k -> tens of thousands, so the collapse
# that follows runs at a sane ratio and keeps the form.
PLANAR = 0.0
# `--axis-map "y,z,-x"`: an explicit source->engine axis permutation, one token per ENGINE axis
# (X=width, Y=up, Z=forward). Named modes cover the common cases, but every asset-store model picks
# its own convention: the T-34 runs along Blender Y, this community tank runs along Blender X with
# the gun pointing -X. Guessing wastes a build; state it and verify against the emitted bbox
# (a tank must read NARROW in X, SHORT in Y, LONG in Z).
AXIS_MAP = None


def _axis_pick(tok, co):
    neg = tok.startswith("-")
    k = {"x": 0, "y": 1, "z": 2}[tok[-1]]
    v = co[k]
    return -v if neg else v


def to_engine(co):
    if AXIS_MAP:
        c = (co[0], co[1], co[2])
        return tuple(_axis_pick(t, c) for t in AXIS_MAP)
    if AXIS == "zup":
        return (co.x, co.z, -co.y)  # Blender/FBX Z-up (RH) -> engine Y-up
    return (co.x, co.y, co.z)  # already engine-space Y-up (glTF)


def to_engine_n(n):
    if AXIS_MAP:
        return tuple(_axis_pick(t, n) for t in AXIS_MAP)
    if AXIS == "zup":
        return (n[0], n[2], -n[1])
    return (n[0], n[1], n[2])


def write_mesh_blob(path, pos, nrm, uv, tris):
    with open(path, "wb") as f:
        f.write(b"MESH")
        f.write(struct.pack("<II", len(pos), len(tris)))
        for p in pos:
            f.write(struct.pack("<3f", *p))
        for n in nrm:
            f.write(struct.pack("<3f", *n))
        for t in uv:
            f.write(struct.pack("<2f", *t))
        for t in tris:
            f.write(struct.pack("<3I", *t))


def extract(obj, tris_budget):
    """Triangulate -> decimate to budget -> unweld to per-corner verts (engine space)."""
    bpy.ops.object.select_all(action="DESELECT")
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj

    me = obj.data
    if me.shape_keys:
        obj.shape_key_clear()

    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.mesh.quads_convert_to_tris(quad_method="BEAUTY", ngon_method="BEAUTY")
    bpy.ops.object.mode_set(mode="OBJECT")

    ntris = len(obj.data.polygons)
    if PLANAR > 0.0 and ntris > tris_budget:
        import math
        mod = obj.modifiers.new("planar", "DECIMATE")
        mod.decimate_type = "DISSOLVE"
        mod.angle_limit = math.radians(PLANAR)
        bpy.ops.object.modifier_apply(modifier=mod.name)
        bpy.ops.object.mode_set(mode="EDIT")
        bpy.ops.mesh.select_all(action="SELECT")
        bpy.ops.mesh.quads_convert_to_tris(quad_method="BEAUTY", ngon_method="BEAUTY")
        bpy.ops.object.mode_set(mode="OBJECT")
        print(f"    planar({PLANAR}deg) {ntris} -> {len(obj.data.polygons)} tris")
        ntris = len(obj.data.polygons)
    if ntris > tris_budget:
        ratio = max(0.001, tris_budget / ntris)
        mod = obj.modifiers.new("dec", "DECIMATE")
        mod.ratio = ratio
        bpy.ops.object.modifier_apply(modifier=mod.name)
        bpy.ops.object.mode_set(mode="EDIT")
        bpy.ops.mesh.select_all(action="SELECT")
        bpy.ops.mesh.quads_convert_to_tris(quad_method="BEAUTY", ngon_method="BEAUTY")
        bpy.ops.object.mode_set(mode="OBJECT")
        print(f"    decimated {ntris} -> {len(obj.data.polygons)} tris (ratio {ratio:.4f})")

    me = obj.data
    me.calc_loop_triangles()
    uv_layer = me.uv_layers.active.data if me.uv_layers.active else None
    try:
        corner_normals = me.corner_normals  # Blender 4.1+/5.x
    except Exception:
        corner_normals = None

    pos, nrm, uv, tris = [], [], [], []
    dedup = {}
    for lt in me.loop_triangles:
        tri = []
        for k in range(3):
            li = lt.loops[k]
            vi = lt.vertices[k]
            p = to_engine(me.vertices[vi].co)
            nv = corner_normals[li].vector if corner_normals is not None else me.loops[li].normal
            n = to_engine_n((nv.x, nv.y, nv.z))
            # glTF/Blender UV origin is BOTTOM-left after import, but the engine samples with a
            # TOP-left origin (D3D convention) — ship V unflipped and the skin reads mirrored/
            # "rotated". Flip it here so the texture lands the way the source authored it.
            uv_raw = tuple(uv_layer[li].uv) if uv_layer else (0.0, 0.0)
            u = (uv_raw[0], 1.0 - uv_raw[1]) if FLIP_V else uv_raw
            key = (round(p[0], 5), round(p[1], 5), round(p[2], 5),
                   round(n[0], 4), round(n[1], 4), round(n[2], 4),
                   round(u[0], 5), round(u[1], 5))
            idx = dedup.get(key)
            if idx is None:
                idx = len(pos)
                dedup[key] = idx
                pos.append(p)
                nrm.append(n)
                uv.append(u)
            tri.append(idx)
        tris.append(tri)
    return pos, nrm, uv, tris


def main():
    args = argv_after_ddash()
    if len(args) < 2:
        print("usage: ... -- <in.gltf> <outdir> [--budget mat=N]... [--default-tris N]")
        sys.exit(2)
    src, outdir = args[0], args[1]
    budgets = {}
    name_rules = []  # [(object-name substring, part name)] — takes precedence over material
    cyl_rules = []   # [(part, cx, cz, radius, min_y)] — spatial cylinder (engine space, Y up)
    below_rules = [] # [(part, max_y)] — everything under a height line (tank running gear)
    box_rules = []   # [(part, xmin,xmax, ymin,ymax, zmin,zmax)] — general AABB (engine space)
    default_tris = 4000
    i = 2
    while i < len(args):
        if args[i] == "--budget":
            k, v = args[i + 1].split("=")
            budgets[k.lower()] = int(v)
            i += 2
        elif args[i] == "--part":
            sub, p = args[i + 1].split("=")
            name_rules.append((sub, p))
            i += 2
        elif args[i] == "--part-box":
            # <part>=<xmin>,<xmax>,<ymin>,<ymax>,<zmin>,<zmax>  (engine space, Y up)
            pname, rest = args[i + 1].split("=")
            v = [float(x) for x in rest.split(",")]
            box_rules.append((pname, *v))
            i += 2
        elif args[i] == "--part-below":
            pname, v = args[i + 1].split("=")
            below_rules.append((pname, float(v)))
            i += 2
        elif args[i] == "--part-cyl":
            # <part>=<cx>,<cz>,<radius>,<min_y>  (engine space, Y up)
            pname, rest = args[i + 1].split("=")
            cx, cz, rad, miny = (float(x) for x in rest.split(","))
            cyl_rules.append((pname, cx, cz, rad, miny))
            i += 2
        elif args[i] == "--default-tris":
            default_tris = int(args[i + 1])
            i += 2
        elif args[i] == "--axis":
            global AXIS
            AXIS = args[i + 1]
            i += 2
        elif args[i] == "--planar":
            global PLANAR
            PLANAR = float(args[i + 1])
            i += 2
        elif args[i] == "--axis-map":
            global AXIS_MAP
            AXIS_MAP = [t.strip().lower() for t in args[i + 1].split(",")]
            i += 2
        elif args[i] == "--no-flip-v":
            global FLIP_V
            FLIP_V = False
            i += 1
        else:
            i += 1

    os.makedirs(outdir, exist_ok=True)
    ext = src.lower().rsplit(".", 1)[-1]
    if ext == "blend":
        # A .blend is a SCENE, not an interchange file — open it rather than importing into an
        # empty one (importing would drop the object/material graph we actually want).
        bpy.ops.wm.open_mainfile(filepath=src)
    else:
        bpy.ops.wm.read_factory_settings(use_empty=True)
    if ext == "blend":
        pass
    elif ext in ("gltf", "glb"):
        bpy.ops.import_scene.gltf(filepath=src)
    elif ext == "obj":
        bpy.ops.wm.obj_import(filepath=src)
    else:
        bpy.ops.import_scene.fbx(filepath=src)

    meshes = [o for o in bpy.context.scene.objects if o.type == "MESH"]
    if not meshes:
        print("no mesh objects imported")
        sys.exit(1)

    # Bake the FULL world matrix into each mesh so every part shares ONE model space.
    # NOTE: `transform_apply` only bakes an object's OWN local transform — it does NOT walk the
    # parent chain, and this source nests parts under posed parent nodes (the rotor's own node has
    # translation [0,0,0]; its height lives on a parent). With transform_apply the main rotor came
    # out at y≈0.4 — buried inside the fuselage instead of on the mast. Baking `matrix_world`
    # directly is parent-safe.
    for o in meshes:
        o.data.transform(o.matrix_world)
        o.matrix_world = mathutils.Matrix.Identity(4)

    # Group objects into PARTS, in priority order:
    #   1. `--part-cyl <part>=<cx>,<cz>,<radius>,<min_y>`  — SPATIAL: an upright cylinder in engine
    #      space (Y up). Everything whose centre falls inside it, above `min_y`, joins that part.
    #      This is how you isolate a TANK TURRET: it has no useful object name (this T-34 ships 232
    #      objects called Cube.003_low / Cylinder.029_low, all on one material), so the only honest
    #      discriminator is the turret RING — above the deck AND within the ring radius. A plain
    #      height cut is wrong: it sweeps in the rear engine-deck grilles, which do not rotate.
    #   2. `--part <name_substr>=<part>`  — by OBJECT NAME (used for the heli's Rotor_Hub/rotor_q).
    #   3. otherwise the object's MATERIAL (one material per glTF primitive).
    #
    # Moving parts must be split by NODE, not by material: a tank's turret and gun ride different
    # HIER nodes (and the gun's node is a CHILD of the turret's, so it yaws AND elevates).
    def centre(o):
        vs = [v.co for v in o.data.vertices]
        if not vs:
            return None
        mn = [min(v[k] for v in vs) for k in range(3)]
        mx = [max(v[k] for v in vs) for k in range(3)]
        return to_engine(mathutils.Vector([(mn[k] + mx[k]) * 0.5 for k in range(3)]))

    by_mat = {}
    for o in meshes:
        part = None
        c = centre(o)
        # `--part-box`: a plain AABB. Used to halve a tank's running gear into LEFT/RIGHT groups —
        # one group is capped at ~10,900 tris by the u16 strip index, so splitting it doubles the
        # triangle budget and lets each half decimate gently instead of shredding the track links.
        if c is not None:
            for pname, x0, x1, y0, y1, z0, z1 in box_rules:
                if x0 <= c[0] <= x1 and y0 <= c[1] <= y1 and z0 <= c[2] <= z1:
                    part = pname
                    break
        if part is None and c is not None:
            for pname, cx, cz, rad, miny in cyl_rules:
                if c[1] >= miny and ((c[0] - cx) ** 2 + (c[2] - cz) ** 2) ** 0.5 <= rad:
                    part = pname
                    break
        # `--part-below <part>=<max_y>`: everything under a height line. This exists to give a
        # tank's RUNNING GEAR its own group+budget. Joining it into the hull and decimating the lot
        # to one budget destroys it: the track links are ~100 separate ~520-tri objects, and a 9%
        # collapse leaves each a ~47-tri shard (in-game: a fan of jagged triangles under the tank).
        # Split it out and each half gets a gentle ratio instead of one brutal one.
        if part is None and c is not None:
            for pname, maxy in below_rules:
                if c[1] < maxy:
                    part = pname
                    break
        if part is None:
            for sub, p in name_rules:
                if sub.lower() in o.name.lower():
                    part = p
                    break
        if part is None:
            part = o.data.materials[0].name if o.data.materials else "none"
        by_mat.setdefault(part, []).append(o)
    print(f"imported {len(meshes)} objects in {len(by_mat)} part(s)")

    for mat, objs in sorted(by_mat.items()):
        bpy.ops.object.select_all(action="DESELECT")
        for o in objs:
            o.select_set(True)
        bpy.context.view_layer.objects.active = objs[0]
        if len(objs) > 1:
            bpy.ops.object.join()
        obj = bpy.context.view_layer.objects.active

        budget = budgets.get(mat.lower(), default_tris)
        print(f"  [{mat}] {len(objs)} object(s), budget {budget} tris")
        pos, nrm, uv, tris = extract(obj, budget)

        mnx = [min(p[k] for p in pos) for k in range(3)]
        mxx = [max(p[k] for p in pos) for k in range(3)]
        safe_mat = "".join(c if c.isalnum() or c in "-_" else "_" for c in mat)
        out = os.path.join(outdir, f"{safe_mat}.mesh")
        write_mesh_blob(out, pos, nrm, uv, tris)
        warn = ""
        if len(pos) >= 65535:
            warn += "  !! verts >= u16"
        if len(tris) * 6 >= 65535:
            warn += "  !! strip may exceed u16 — lower this budget"
        print(f"    -> {out}: {len(pos)} verts, {len(tris)} tris, "
              f"bbox [{mnx[0]:.1f},{mnx[1]:.1f},{mnx[2]:.1f}]..[{mxx[0]:.1f},{mxx[1]:.1f},{mxx[2]:.1f}]{warn}")


main()
