#!/usr/bin/env python3
"""Extract the retail shader registry into the two committed TSVs qm reads.

    extract_shader_registry.py --exe <SecuROM-unpacked mercs2 exe> --data <game data dir> --out <dir>

Writes `<out>/shader_families.tsv` and `<out>/registered_shaders.tsv`.

The exe is the SecuROM-unpacked image (`mercs2_unpacked.exe`, the runtime dump in `securom_dump/`);
`--data` is the game's `data` folder holding the six `shader*.bin` stores. Needs `pefile` and
`unicorn`.

`registered_shaders.tsv`'s `inputs` column lists, for a vertex shader, each store holding a record
for its `.sho` stem and the `(usage.index)` of every input the record's bytecode declares. Its
`constants` column lists, for the same records, the names of the constants the record's CTAB
declares (`-` for a pixel shader, as for `inputs`).

What it does, and why this way
------------------------------
The registry is `FUN_0084f130`. Every registration is `mov ecx, <static record>; call
FUN_0085ac90(name, sho, class)`, and the record's vtable decides the registry (vertex or pixel) and
the family. Several registrations sit in islands reached through `jmp [stub]` (`FUN_02475bc0`,
`FUN_005726e0`, `FUN_006188b0`, `FUN_02485980`). The islands are relocated plaintext joined by
`push <continuation>; push FUN_0085ac90; ret`, readable by disassembly. The registry is executed
because each row's `.sho` depends on the caps bits, the ShaderLevel byte and the `_li` handle tests,
and because six call sites (and one in the water registrar) take the `.sho` from two or three
branches:

1. Every record object is constructed by emulating the static initializer that names it
   (`mov r32, <record>` then the base constructor `FUN_0085ace0` / `FUN_0085ade0`, directly or in an
   array loop). The vtable it installs is the family.
2. `FUN_0084f130` runs, through its sub-registrars, once per configuration: caps word `+0x5e4` bits
   2 (VT stores) and 3 (R2VB stores) and the ShaderLevel byte `DAT_00dfc345`. `FUN_0085b3f0` (store
   load) returns at once. `FUN_0085ac90` is intercepted: the call is logged and the record's D3D
   handle field is set as `FUN_0085b6f0` would, non-null exactly when `stem + "_3.sho"`
   (ShaderLevel 1) or `stem + "_3l.sho"` (ShaderLevel 0) is an id in a store resident in that
   configuration. The `_li` alternates branch on that field.
3. Each family's constant binder (vtable `+0x10`) is emulated with `FUN_0085ac40` intercepted, which
   lists the constant names the engine resolves for that family.

A configuration is `vt`, `r2vb` or `none` (the resident extra store pair: caps bit 2 set; bit 2
clear and bit 3 set; neither), crossed with ShaderLevel `0` / `1`.

`PgCompositeFP` registers outside `FUN_0084f130`, in the composite pass constructor (vtable
`0x00BAAE9C`, relocated body at `0x0246A383`), once per process behind the byte `0x011759C0`. Its
site is decoded statically (`composite_registration`): `push "PgCompositeFP"` at `0x0246A443`,
`mov ecx, 0x0127CB58`, then `push <continuation>; push FUN_0085ac90; ret`. The `.sho` pointer is
`[0x0245A8C4] ^ [0x007295B6]` (`0x0246A433`). The class argument is the result of
`push 0x024581EE; call eax` with `eax = 0xE842355F ^ [0x02460160]` (`0x0246A40E`), a SecuROM call
whose target the dump does not hold, so the row's class is `-`. Between the run-once test
(`0x0246A3E4`) and the call nothing reads the caps word or the ShaderLevel byte, and the row lists
every configuration.
"""
import argparse
import os
import re
import struct
import sys

import pefile
from unicorn import Uc, UcError, UC_ARCH_X86, UC_MODE_32, UC_HOOK_CODE, UC_HOOK_MEM_WRITE
from unicorn.x86_const import (UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_EDI, UC_X86_REG_EIP,
                               UC_X86_REG_ESP)

REGISTRY = 0x0084F130
REGISTRY_END = 0x008523D0
REGISTRY_STOP = 0x008522DE  # after the four sub-registrar calls, before the material templates
REGISTER = 0x0085AC90
STORE_LOAD = 0x0085B3F0
PS_CTOR, VS_CTOR = 0x0085ACE0, 0x0085ADE0
VS_LOAD, PS_LOAD = 0x0085AF00, 0x0085B1A0
BIND_ONE = 0x0085AC40
ATEXIT = 0x009EE331
CAPS_PTR = 0x01176288
SHADERLEVEL = 0x00DFC345
COMPOSITE = dict(sho_a=0x0246A433, sho_b=0x0246A439, name=0x0246A443, rec=0x0246A448,
                 cont=0x0246A44D, target=0x0246A452, ret=0x0246A457)
HANDLE_FIELD = {VS_LOAD: 0x110, PS_LOAD: 0xF8}
BASE_SIZE = {VS_LOAD: 0x114, PS_LOAD: 0xFC}
STAGE = {VS_LOAD: 'vertex', PS_LOAD: 'pixel'}
RETURN_PAGE = 0xDEAD0000
STACK, HEAP = 0x70000000, 0x60000000

# The family names qm and the m2-sdk expose, one per decoded record vtable. Each name is the
# subsystem whose retail registrations construct records of that vtable.
FAMILY_NAMES = {
    0x00BE89EC: 'pixel', 0x00BE8A04: 'vertex',
    0x00BE8A58: 'blur_pixel', 0x00BE85B4: 'anti_aliasing_pixel', 0x00BAADA0: 'hdr_flare_pixel',
    0x00BAADB8: 'tone_mapping_pixel', 0x00BAADD0: 'adaptive_luminance_pixel',
    0x00BAADE8: 'down_sample_pixel', 0x00BAAE5C: 'shimmer_pixel', 0x00BAAE84: 'motion_blur_pixel',
    0x00BAAEDC: 'composite_pixel', 0x00BAAF74: 'rain_vertex', 0x00BAAF8C: 'rain_pixel',
    0x00BAB130: 'blob_shadow_vertex', 0x00BAB148: 'blob_shadow_pixel',
    0x00BAB56C: 'cloud_render_vertex', 0x00BAB584: 'cloud_render_pixel',
    0x00BAB59C: 'cloud_gen_pixel', 0x00BAB5EC: 'sky_vertex', 0x00BAB604: 'sky_pixel',
    0x00BAB61C: 'sun_vertex', 0x00BAB94C: 'water_wake_vertex', 0x00BAB964: 'water_wake_pixel',
    0x00BAB97C: 'water_height_map_pixel', 0x00BAC150: 'water_vertex', 0x00BAC168: 'water_pixel',
    0x00BAC2FC: 'fx_vertex', 0x00BAC314: 'fx_pixel', 0x00BAC3F8: 'ribbon_vertex',
    0x00BAC410: 'ribbon_pixel', 0x00BAC448: 'road_vertex', 0x00BAC5FC: 'decal_vertex',
    0x00BAC614: 'decal_pixel', 0x00BAC6B0: 'mesh_combiner_vertex',
    0x00BAC858: 'billboard_tree_vertex', 0x00BAC870: 'billboard_tree_instance_vertex',
    0x00BAC888: 'billboard_tree_pixel', 0x00BAC97C: 'scrub_vertex',
    0x00BACCD8: 'terrain_mesh_vertex', 0x00BAD0F8: 'scaleform_strip_vertex',
    0x00BAD110: 'scaleform_glyph_vertex', 0x00BAD128: 'scaleform_strip_pixel',
    0x00BAD140: 'scaleform_text_texture_pixel', 0x00BAD158: 'scaleform_cxform_pixel',
    0x00BAD170: 'scaleform_solid_color_pixel',
}

CONFIGS = [('none', 0, 0), ('r2vb', 0, 1), ('vt', 1, 0), ('vt', 1, 1)]
STORES = ['shader3.bin', 'shader3Low.bin', 'shaderVT.bin', 'shaderVTLow.bin', 'shaderR2VB.bin',
          'shaderR2VBLow.bin']


class Image:
    def __init__(self, path):
        pe = pefile.PE(path, fast_load=True)
        self.base = pe.OPTIONAL_HEADER.ImageBase
        self.data = open(path, 'rb').read()
        self.sections = [(s.Name.rstrip(b'\0').decode('latin1'), self.base + s.VirtualAddress,
                          max(s.Misc_VirtualSize, s.SizeOfRawData), s.PointerToRawData,
                          s.SizeOfRawData) for s in pe.sections]

    def read(self, va, n):
        for _, vs, vsz, praw, sraw in self.sections:
            if vs <= va < vs + vsz:
                d = va - vs
                b = self.data[praw + d:praw + min(d + n, sraw)]
                return b + b'\0' * (n - len(b))
        return None

    def u32(self, va):
        b = self.read(va, 4)
        if b is None:
            raise SystemExit(f'{va:#010x} is not mapped in the exe')
        return struct.unpack('<I', b)[0]

    def cstr(self, va):
        out = b''
        while True:
            b = self.read(va, 64)
            if b is None:
                raise SystemExit(f'string at {va:#010x} is not mapped in the exe')
            i = b.find(b'\0')
            if i >= 0:
                return (out + b[:i]).decode('latin1')
            out += b
            va += 64

    def emulator(self):
        mu = Uc(UC_ARCH_X86, UC_MODE_32)
        hi = max(vs + vsz for _, vs, vsz, _, _ in self.sections)
        mu.mem_map(self.base, ((hi + 0xFFF) & ~0xFFF) - self.base)
        mu.mem_write(self.base, self.data[:0x1000])
        for _, vs, _, praw, sraw in self.sections:
            mu.mem_write(vs, self.data[praw:praw + sraw])
        mu.mem_map(STACK, 0x100000)
        mu.mem_map(HEAP, 0x100000)
        mu.mem_map(RETURN_PAGE, 0x1000)
        return mu

    def code_sections(self):
        return [s for s in self.sections if s[0] in ('.text', 'Stext', '.securom')]


def pandemic_hash_m2(text):
    """The engine's FUN_00824270: FNV-1a over `(signed char) | 0x20`, then `^ 0x2a`, `* prime`."""
    h = 0x811C9DC5
    for c in text.encode('latin1'):
        v = (c | 0xFFFFFF00) if c & 0x80 else c
        h = ((h ^ (v | 0x20)) * 0x01000193) & 0xFFFFFFFF
    return ((h ^ 0x2A) * 0x01000193) & 0xFFFFFFFF


def store_records(path):
    """`{id: blob}` of one store: `[count][count x (id, offset, size, kind)][blobs]`."""
    b = open(path, 'rb').read()
    n = struct.unpack_from('<I', b, 0)[0]
    out = {}
    for i in range(n):
        rid, off, size, _kind = struct.unpack_from('<IIII', b, 4 + 16 * i)
        out[rid] = b[off:off + size]
    return out


def vertex_inputs(blob):
    """`(usage, usage index)` of every `dcl` of an input register (`v#`), in order."""
    tokens = struct.unpack(f'<{len(blob) // 4}I', blob)
    if tokens[0] != 0xFFFE0300:
        raise SystemExit('vertex_inputs of a blob that is not vs_3_0')
    out, i = [], 1
    while i < len(tokens):
        tok = tokens[i]
        if tok == 0x0000FFFF:
            break
        op = tok & 0xFFFF
        if op == 0xFFFE:
            i += 1 + ((tok >> 16) & 0x7FFF)
            continue
        nparams = (tok >> 24) & 0xF
        if op == 0x1F and nparams >= 2:
            usage_tok, dst = tokens[i + 1], tokens[i + 2]
            if (((dst >> 28) & 0x7) | ((dst >> 8) & 0x18)) == 1:
                out.append((usage_tok & 0xF, (usage_tok >> 16) & 0xF))
        i += 1 + nparams
    return out


def ctab_constants(blob):
    """The names of the constants a shader's CTAB comment declares, in table order."""
    tokens = struct.unpack(f'<{len(blob) // 4}I', blob)
    i = 1
    while i < len(tokens):
        tok = tokens[i]
        if tok == 0x0000FFFF:
            break
        if tok & 0xFFFF == 0xFFFE:
            n = (tok >> 16) & 0x7FFF
            body = blob[4 * (i + 1):4 * (i + 1 + n)]
            if body[:4] == b'CTAB':
                t = body[4:]
                count, info = struct.unpack_from('<II', t, 12)
                names = []
                for k in range(count):
                    name_off = struct.unpack_from('<I', t, info + 20 * k)[0]
                    names.append(t[name_off:t.index(b'\0', name_off)].decode('latin1'))
                return names
            i += 1 + n
            continue
        i += 1 + ((tok >> 24) & 0xF)
    raise SystemExit('ctab_constants of a blob with no CTAB')


def composite_registration(img):
    """`(record, name, sho)` of the `PgCompositeFP` site, decoded from its bytes."""
    c = COMPOSITE
    want = [(c['sho_a'], b'\x8b\x0d'), (c['sho_b'], b'\x33\x0d'), (c['name'], b'\x68'),
            (c['rec'], b'\xb9'), (c['cont'], b'\x68'),
            (c['target'], b'\x68' + struct.pack('<I', REGISTER)), (c['ret'], b'\xc3')]
    for va, prefix in want:
        if img.read(va, len(prefix)) != prefix:
            raise SystemExit(f'the PgCompositeFP site at {va:#010x} is not {prefix.hex()}')
    sho = img.cstr(img.u32(img.u32(c['sho_a'] + 2)) ^ img.u32(img.u32(c['sho_b'] + 2)))
    name = img.cstr(img.u32(c['name'] + 1))
    if not sho.lower().endswith('.sho'):
        raise SystemExit(f'the PgCompositeFP site decodes the file {sho!r}, not a .sho')
    return img.u32(c['rec'] + 1), name, sho


def pop_return(mu, arg_bytes):
    esp = mu.reg_read(UC_X86_REG_ESP)
    mu.reg_write(UC_X86_REG_EIP, struct.unpack('<I', mu.mem_read(esp, 4))[0])
    mu.reg_write(UC_X86_REG_ESP, esp + 4 + arg_bytes)


def start_frame(mu):
    sp = STACK + 0x80000
    mu.reg_write(UC_X86_REG_ESP, sp)
    mu.mem_write(sp, struct.pack('<II', RETURN_PAGE, RETURN_PAGE))
    return sp


def run_registry(img, bit2, bit3, level, stores, vtables):
    mu = img.emulator()
    caps = HEAP
    mu.mem_write(caps + 0x5E4, struct.pack('<I', (bit2 << 2) | (bit3 << 3)))
    mu.mem_write(CAPS_PTR, struct.pack('<I', caps))
    mu.mem_write(SHADERLEVEL, bytes([level]))
    for obj, vt in (vtables or {}).items():
        mu.mem_write(obj, struct.pack('<I', vt))
    resident = ['shader3.bin', 'shader3Low.bin']
    if bit2:
        resident += ['shaderVT.bin', 'shaderVTLow.bin']
    elif bit3:
        resident += ['shaderR2VB.bin', 'shaderR2VBLow.bin']
    ids = set().union(*(stores[s] for s in resident))
    suffix = '_3.sho' if level else '_3l.sho'
    log = []
    start_frame(mu)
    mu.reg_write(UC_X86_REG_ECX, HEAP + 0x2000)

    def hook(uc, addr, size, _):
        if addr == REGISTRY_STOP:
            uc.emu_stop()
        elif addr == STORE_LOAD:
            pop_return(uc, 4)
        elif addr == REGISTER:
            esp = uc.reg_read(UC_X86_REG_ESP)
            rec = uc.reg_read(UC_X86_REG_ECX)
            name_p, sho_p, cls = struct.unpack('<III', uc.mem_read(esp + 4, 12))
            sho = img.cstr(sho_p)
            if not sho.lower().endswith('.sho'):
                raise SystemExit(f'registered file {sho!r} does not end in .sho')
            handle = struct.pack('<I', 0x5000 if pandemic_hash_m2(sho[:-4] + suffix) in ids else 0)
            if vtables is None:
                # Collecting record addresses: the stage is not known, so both handle fields hold
                # the store's answer and either `_li` branch reads it.
                for off in HANDLE_FIELD.values():
                    uc.mem_write(rec + off, handle)
            elif rec not in vtables:
                raise SystemExit(f'registration into {rec:#010x}, which no static initializer builds')
            else:
                uc.mem_write(rec + HANDLE_FIELD[img.u32(vtables[rec] + 8)], handle)
            log.append((rec, img.cstr(name_p), sho, cls))
            pop_return(uc, 12)

    mu.hook_add(UC_HOOK_CODE, hook)
    mu.emu_start(REGISTRY, 0xFFFFFFFF, count=5_000_000)
    if mu.reg_read(UC_X86_REG_EIP) != REGISTRY_STOP:
        raise SystemExit(f'the registry emulation stopped at {mu.reg_read(UC_X86_REG_EIP):#010x}')
    return log


def registry_objects(img, stores):
    """Every record object a registration names, in any configuration."""
    objs = set()
    for _, bit2, bit3 in CONFIGS:
        for level in (0, 1):
            objs.update(rec for rec, *_ in run_registry(img, bit2, bit3, level, stores, None))
    return sorted(objs)


def construct_records(img, objs):
    """Emulate every static initializer that loads a record's address into a register."""
    sites = set()
    for _, vs, _, praw, sraw in img.code_sections():
        blob = img.data[praw:praw + sraw]
        for o in objs:
            for op in range(0xB8, 0xC0):
                for m in re.finditer(re.escape(bytes([op]) + struct.pack('<I', o)), blob):
                    va = vs + m.start()
                    if not REGISTRY <= va < REGISTRY_END:
                        sites.add(va)
    vtables, ctors, extents = {}, {}, {}
    for va in sorted(sites):
        mu = img.emulator()
        start_frame(mu)
        for o in objs:
            mu.mem_write(o, b'\0' * 4)
        calls, writes = [], []

        def memw(uc, access, addr, size, value, _):
            writes.append((addr, size))

        def hook(uc, addr, size, _):
            if addr == ATEXIT:
                pop_return(uc, 0)
            elif addr == PS_CTOR:
                calls.append((addr, uc.reg_read(UC_X86_REG_ECX)))
            elif addr == VS_CTOR:
                calls.append((addr, uc.reg_read(UC_X86_REG_EAX)))
            elif addr == RETURN_PAGE:
                uc.emu_stop()

        mu.hook_add(UC_HOOK_CODE, hook)
        mu.hook_add(UC_HOOK_MEM_WRITE, memw)
        try:
            mu.emu_start(va, 0xFFFFFFFF, count=3000)
        except UcError:
            pass  # a site inside a function runs off its caller's frame once the object is built
        built = sorted(this for _, this in calls)
        for o in objs:
            if o in vtables:
                continue
            vt = struct.unpack('<I', mu.mem_read(o, 4))[0]
            if vt and img.read(vt + 8, 4) is not None and img.u32(vt + 8) in (VS_LOAD, PS_LOAD):
                vtables[o] = vt
                ctors[o] = [a for a, this in calls if this == o]
                bound = min([b for b in built if b > o] + [o + 0x1000])
                extents[o] = max((a + n - o for a, n in writes if o <= a < bound), default=0)
    missing = [hex(o) for o in objs if o not in vtables]
    if missing:
        raise SystemExit(f'no static initializer builds records {missing}')
    for o, vt in vtables.items():
        want = PS_CTOR if img.u32(vt + 8) == PS_LOAD else VS_CTOR
        if ctors[o] != [want]:
            raise SystemExit(f'record {o:#010x} (vtable {vt:#010x}) is built by {ctors[o]}, not {want:#x}')
    return vtables, extents


def family_vtables(img):
    out = []
    for name, vs, _, praw, sraw in img.sections:
        if name not in ('.rdata', 'Srdata'):
            continue
        blob = img.data[praw:praw + sraw]
        for i in range(0, len(blob) - 12, 4):
            _, reg, load = struct.unpack_from('<III', blob, i)
            if reg == REGISTER and load in (VS_LOAD, PS_LOAD):
                out.append(vs + i)
    return out


def binder_constants(img, binder):
    mu = img.emulator()
    obj = HEAP + 0x10000
    start_frame(mu)
    mu.reg_write(UC_X86_REG_ECX, obj)
    for off in (0x10C, 0xF4):  # the constant-table field of each stage
        mu.mem_write(obj + off, struct.pack('<I', HEAP + 0x20000))
    out = []

    def hook(uc, addr, size, _):
        if addr == BIND_ONE:
            out.append((img.cstr(uc.reg_read(UC_X86_REG_EAX)), uc.reg_read(UC_X86_REG_EDI) - obj - 4))
            uc.reg_write(UC_X86_REG_EAX, 0)
            pop_return(uc, 0)
        elif addr == RETURN_PAGE:
            uc.emu_stop()

    mu.hook_add(UC_HOOK_CODE, hook)
    mu.emu_start(binder, 0xFFFFFFFF, count=20000)
    if mu.reg_read(UC_X86_REG_EIP) != RETURN_PAGE:
        raise SystemExit(f'binder {binder:#010x} did not return')
    return out


def main():
    ap = argparse.ArgumentParser(description=__doc__.split('\n\n')[0])
    ap.add_argument('--exe', required=True)
    ap.add_argument('--data', required=True)
    ap.add_argument('--out', required=True)
    a = ap.parse_args()
    img = Image(a.exe)
    records = {s: store_records(os.path.join(a.data, s)) for s in STORES}
    stores = {s: set(r) for s, r in records.items()}

    composite = composite_registration(img)
    objs = sorted(set(registry_objects(img, stores)) | {composite[0]})
    vtables, extents = construct_records(img, objs)

    fams = {}
    for vt in family_vtables(img):
        if vt not in FAMILY_NAMES:
            raise SystemExit(f'record vtable {vt:#010x} has no family name')
        load = img.u32(vt + 8)
        consts = binder_constants(img, img.u32(vt + 0x10))
        bind_end = max((off + 8 for _, off in consts), default=0)
        ctor_end = max((extents[o] for o, v in vtables.items() if v == vt), default=0)
        fams[vt] = dict(name=FAMILY_NAMES[vt], stage=STAGE[load], consts=[c for c, _ in consts],
                        size=max(BASE_SIZE[load], bind_end, ctor_end))
    unknown = set(FAMILY_NAMES) - set(fams)
    if unknown:
        raise SystemExit(f'named vtables {sorted(hex(v) for v in unknown)} are not record vtables')

    rows = {}
    for cfg, bit2, bit3 in CONFIGS:
        for level in (0, 1):
            for rec, name, sho, cls in run_registry(img, bit2, bit3, level, stores, vtables):
                key = (rec, name, sho, cls)
                rows.setdefault(key, set()).add(f'{cfg}{level}')
    rows[composite + ('-',)] = {f'{cfg}{level}' for cfg, _, _ in CONFIGS for level in (0, 1)}

    fam_order = sorted(fams.values(), key=lambda f: (f['stage'] != 'pixel', f['name'] != f['stage'], f['name']))
    with open(os.path.join(a.out, 'shader_families.tsv'), 'w', newline='\n') as f:
        f.write('family\tstage\tvtable\tsize\tconstants\n')
        for fam in fam_order:
            vt = next(v for v, x in fams.items() if x is fam)
            f.write(f"{fam['name']}\t{fam['stage']}\t0x{vt:08X}\t0x{fam['size']:X}\t{','.join(fam['consts'])}\n")
    order = {'none0': 0, 'none1': 1, 'r2vb0': 2, 'r2vb1': 3, 'vt0': 4, 'vt1': 5}
    with open(os.path.join(a.out, 'registered_shaders.tsv'), 'w', newline='\n') as f:
        f.write('name\tkey\tsho\tstage\tfamily\tclass\tconfigs\tinputs\tconstants\n')
        for (rec, name, sho, cls), cfgs in sorted(rows.items(), key=lambda kv: (kv[0][1].lower(), kv[0][2].lower(), str(kv[0][3]))):
            fam = fams[vtables[rec]]
            inputs, constants = '-', '-'
            if fam['stage'] == 'vertex':
                held, declared = [], []
                for store in STORES:
                    rid = pandemic_hash_m2(sho[:-4] + ('_3l.sho' if 'Low' in store else '_3.sho'))
                    if rid in records[store]:
                        dcl = '+'.join(f'{u}.{i}' for u, i in vertex_inputs(records[store][rid]))
                        held.append(f'{store}={dcl}')
                        declared.append(f"{store}={'+'.join(ctab_constants(records[store][rid]))}")
                inputs = ';'.join(held) if held else '-'
                constants = ';'.join(declared) if declared else '-'
            f.write(f"{name}\t0x{pandemic_hash_m2(name):08X}\t{sho}\t{fam['stage']}\t{fam['name']}\t{cls}\t"
                    f"{','.join(sorted(cfgs, key=order.get))}\t{inputs}\t{constants}\n")
    print(f'{len(fams)} families, {len(rows)} registrations', file=sys.stderr)


if __name__ == '__main__':
    main()
