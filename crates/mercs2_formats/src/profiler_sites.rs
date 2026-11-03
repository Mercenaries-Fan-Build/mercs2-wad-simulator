//! The code sites the profiler ASI (`shipments/mercs2-profiler`) detours or reads, found in the game image
//! without per-build addresses. Every site resolves exactly once or resolution fails, naming each site's
//! match count. Byte signatures over `.text`, in the form of [`crate::lua_sites::Pattern`]: `call rel32`
//! targets and absolute addresses are `??`. Roles, from the disassembly of `mercs2_unpacked.exe`:
//!
//! | site | VA (unpacked) | what it is |
//! |---|---|---|
//! | [`Site::RunFrame`] | `0x00630EF0` | `void __cdecl (void)`: one frame. `WinMain` (`FUN_00631670`) calls it once per loop; it brackets the frame with `QueryPerformanceCounter`, runs the master tick (`FUN_004C14F0`), RenderFrame (view vtable `+0x14`), the frame cap (`FUN_004F59A0`) and Submit/Present (view vtable `+0x10`) |
//! | [`Site::BindDevice`] | `0x00749060` | the renderer's device bind (`FUN_00749060`, `this` in `EDI`): `[this+0x2D24] = IDirect3D9 global`; `[this+0x2D28] = RenderSystem ? [RenderSystem + 0x5BC] : 0`. Read, not detoured: its operands name the RenderSystem global ([`ProfilerSites::render_system`]) and the device's offset in it |
//! | [`Site::JobRegister`] | `0x0084B230` | the Pimp job system registering the jobtype `0xEC7AA7F0` (handler `0x0084B200`): `i = count; cpu0.handlers[i] = {fn, hash}; cpu1.handlers[i] = {fn, hash}; count = i + 1`. Read, not detoured: its operands name the jobtype count global and the two per-CPU handler tables ([`ProfilerSites::jobs`]) |
//! | [`Site::JobWait`] | `0x008765C0` | `void __cdecl (int fence[2])`: wait until `fence[0] == fence[1]`, running jobs from the calling thread's CPU queues meanwhile (`cpu = TLS+8`, control block `cpu*300`). Its prologue jumps through a SecuROM island back into the body at `0x008765CF`; every caller enters at the prologue. Its control-block operand must be [`JobTables::handlers`]`[0] - JOB_HANDLERS_AT` |
//! | [`Site::CellsChangeRing`] | `0x00735AEF` | The jobtype-`0xECAED415` change-list append (inside `FUN_00735640`): `ring[counter++] = (entity_id, old_new, extra[0], extra[1])`. Read, not detoured: its operands name the ring counter global and the ring base ([`ProfilerSites::cells_change`]), which the profiler reads before and after each `0xECAED415` invocation to count the changes it produced |
//! | [`Site::PacketProducer`] | `0x008546A0` | The render packet producer (`FUN_008546A0`): `if (count == 0x2000) return;`, then fills packet `count` of the frame's buffer (stride `0x58`) and adds to `count` at `+0x236`. A frame's packets past the cap are dropped. Read, not detoured: its operands name the packet count global and the cap ([`ProfilerSites::frame_stores`]) |
//! | [`Site::PacketFrameEnd`] | `0x00854B38` | The frame's packet hand-off (`FUN_00854B38`, which hashes `"PgPrimitive::Sort()"`): sorts and submits the frame's packets, then tail-jumps to the begin-frame at `+0x8D9`. Read, not detoured: it must read the producer's count, and its jump names the begin-frame |
//! | [`Site::FrameBegin`] | `0x00853FBD` | Inside the begin-frame `FUN_00853E00` (`+0x1BD`), `void __cdecl (void)`: flips the frame's buffers and zeroes the packet count, the display-list cursor (dwords) and the constant sidecar's cursor (bytes); `+0x64C` resets the world-matrix counter to 2. Its function is detoured: at its entry every counter still holds the finished frame's total ([`FrameStores::begin`]) |
//! | [`Site::StoreAlloc`] | `0x00853CEE` | The frame stores' allocator (`FUN_00853C70`), per buffer parity: the display list (`0x50000` bytes) and the constant sidecar (`0x20000` bytes). Read, not detoured: its immediates are the two capacities |
//! | [`Site::MatrixIndex`] | `0x00477E20` | A world-matrix push (`FUN_00477E20`): `i = counter++`, the entry at `(parity · 0x1400 + i) · 0x40`. Read, not detoured: it must use the counter the begin-frame resets, and its multiplier is the table's capacity per parity |
//!
//! Nothing that writes the display list, the sidecar or the matrix table checks its capacity
//! (`docs/reverse_engineer/render_frame_stores.md`).
//!
//! The Pimp per-CPU control block (`0x019F9040` in `mercs2_unpacked.exe`, stride 300) is `+0x00` busy flag,
//! `+0x04` high-priority queue, `+0x08` low-priority queue, `+0x0C` the handler table: [`JOB_SLOTS`] entries of
//! `{handler, jobtype hash}`. The job loops call `handler(job arg)` as `cdecl` with one argument
//! (`push arg; call [cpu*300 + 0x19F904C + type*8]`, `add esp, 4` after).

use crate::exe_image::Image;
use crate::lua_sites::Pattern;

/// The resolved sites, in table order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Site {
    RunFrame,
    BindDevice,
    JobRegister,
    JobWait,
    CellsChangeRing,
    PacketProducer,
    PacketFrameEnd,
    FrameBegin,
    StoreAlloc,
    MatrixIndex,
}

/// The Pimp per-CPU control block's size.
pub const JOB_BLOCK_STRIDE: u32 = 300;
/// The handler table's offset in a control block.
pub const JOB_HANDLERS_AT: u32 = 0x0c;
/// Entries in a handler table: what is left of a control block after its three header words.
pub const JOB_SLOTS: u32 = (JOB_BLOCK_STRIDE - JOB_HANDLERS_AT) / 8;
/// The jobtype [`Site::JobRegister`] registers.
const JOB_REGISTER_HASH: u32 = 0xec7a_a7f0;
/// Offsets in the [`Site::JobRegister`] pattern of the count global (read, then written back), of each
/// handler table's `{handler, hash}` stores, and of the hash immediate.
const JOB_COUNT_AT: usize = 2;
const JOB_CPU0_HANDLER_AT: usize = 14;
const JOB_HASH_AT: usize = 19;
const JOB_CPU0_HASH_AT: usize = 26;
const JOB_CPU1_HANDLER_AT: usize = 33;
const JOB_CPU1_HASH_AT: usize = 40;
const JOB_COUNT_STORE_AT: usize = 58;
/// Offset in the [`Site::JobWait`] pattern of the control block's busy-flag `disp32`
/// (`mov dword [eax + disp32], 1` after `imul eax, eax, 300`).
const JOB_WAIT_BLOCK_AT: usize = 31;
/// Offsets in the [`Site::CellsChangeRing`] pattern of the counter global (read, then written back) and of
/// the ring base (`add eax, ring`).
const CELLS_COUNTER_AT: usize = 1;
const CELLS_RING_AT: usize = 23;
const CELLS_COUNTER_STORE_AT: usize = 54;
/// Offsets in the [`Site::PacketProducer`] pattern of the count global (`mov ecx, [count]`) and of the cap
/// (`cmp ecx, imm32`), and the offset from the site of its `add [count], ebx` (`01 1d disp32`).
const PACKET_COUNT_AT: usize = 2;
const PACKET_CAP_AT: usize = 8;
const PACKET_COUNT_ADD_AT: usize = 0x236;
/// Offset in the [`Site::PacketFrameEnd`] pattern of the count global (`mov ebx, [count]`), and the
/// offset from the site of its transfer to the begin-frame (`e9 rel32` or `e8 rel32`).
const FRAME_END_COUNT_AT: usize = 3;
const FRAME_END_BEGIN_CALL_AT: usize = 0x8D9;
/// The [`Site::FrameBegin`] site's offset in its function, and offsets in its pattern of the packet count,
/// the display-list cursor and the sidecar cursor (each `mov [global], esi`). The matrix counter's reset
/// (`66 c7 05 disp32 02 00`) sits at `+0x64C` in the function.
const BEGIN_SITE_IN_FUNCTION: u32 = 0x1BD;
const BEGIN_PACKETS_AT: usize = 2;
const BEGIN_LIST_AT: usize = 15;
const BEGIN_SIDECAR_AT: usize = 21;
/// Offsets in the [`Site::FrameBegin`] pattern of the parity global (`mov [parity], ebx`) and the
/// display-list base pointer (`mov [list base], ecx`), both written after the counters are zeroed.
const BEGIN_PARITY_AT: usize = 36;
const BEGIN_LIST_BASE_AT: usize = 42;
const BEGIN_MATRIX_RESET_IN_FUNCTION: u32 = 0x64C;
/// The begin-frame's `mov [sidecar base], eax` (`a3 moffs32`), from the function's start.
const BEGIN_SIDECAR_BASE_IN_FUNCTION: u32 = 0x10D;
/// From the [`Site::PacketProducer`] site: its `mov ebp, [parity]` (`8b 2d disp32`), its
/// `imul eax, eax, stride` (`6b c0 imm8`) and its `add eax, packet array` (`05 imm32`). A packet's address is
/// `array + (parity · cap + index) · stride`.
const PACKET_PARITY_AT: usize = 0x4D;
const PACKET_STRIDE_AT: usize = 0x5E;
const PACKET_ARRAY_AT: usize = 0x61;
/// Offsets in the [`Site::PacketProducer`] pattern of the class global (`cmp byte [class], 0`), and from the site
/// of the producer's reads of the program global (`movzx edx, word [program]`, `0f b7 15 disp32`, stored at packet
/// `+0x40`) and of the class global again (`mov dl, [class]`, `8a 15 disp32`, stored at `+0x44`).
const PACKET_CLASS_AT: usize = 26;
const PACKET_PROGRAM_READ_AT: usize = 0xEF;
const PACKET_CLASS_READ_AT: usize = 0x104;
/// Offsets in the [`Site::StoreAlloc`] pattern of the display list's size (`mov eax, imm32`, then
/// `push imm32` on the second allocation path) and of the sidecar's.
const ALLOC_LIST_AT: [usize; 2] = [1, 24];
const ALLOC_SIDECAR_AT: [usize; 2] = [113, 136];
/// Offsets in the [`Site::MatrixIndex`] pattern of the counter (read `mov ax, [moffs]`, then written back
/// `mov [moffs], ax`) and of the per-parity capacity (`imul eax, eax, imm32`).
const MATRIX_COUNTER_AT: usize = 21;
const MATRIX_COUNTER_STORE_AT: usize = 35;
const MATRIX_PARITY_AT: usize = 40;
const MATRIX_CAP_AT: usize = 46;
/// Offset in the [`Site::MatrixIndex`] pattern of the table's base (`add eax, imm32`); an entry is
/// `base + (parity · cap + index) · 0x40`.
const MATRIX_BASE_AT: usize = 69;

/// The frame's render stores, from [`Site::PacketProducer`], [`Site::PacketFrameEnd`], [`Site::FrameBegin`],
/// [`Site::StoreAlloc`] and [`Site::MatrixIndex`]: each counter's address and its capacity. At the entry of
/// [`FrameStores::begin`] every counter holds the finished frame's total.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameStores {
    /// The begin-frame function (`void __cdecl (void)`).
    pub begin: u32,
    /// The packet count; the producer drops every packet once it reaches `packet_cap`.
    pub packets: u32,
    pub packet_cap: u32,
    /// The display-list cursor, in dwords, and the list's size in bytes.
    pub list_dwords: u32,
    pub list_bytes: u32,
    /// The constant sidecar's cursor and size, in bytes.
    pub sidecar_bytes_used: u32,
    pub sidecar_bytes: u32,
    /// The world-matrix counter (`u16`, reset to 2) and the table's entries per parity.
    pub matrices: u32,
    pub matrix_cap: u32,
    /// The buffer parity global (0 or 1): which half of the packet array and matrix table the frame fills.
    pub parity: u32,
    /// The packet array (`packet_cap` per parity) and a packet's size.
    pub packet_array: u32,
    pub packet_bytes: u32,
    /// The world-matrix table (`matrix_cap` 4×4 `f32` entries per parity).
    pub matrix_table: u32,
    /// The globals holding the frame's display-list and constant-sidecar base pointers.
    pub list_base: u32,
    pub sidecar_base: u32,
    /// The globals the producer copies into each packet: the draw's program (`u16`, packet `+0x40`) and its class
    /// (`u8`, packet `+0x44`; the producer then rewrites some classes, so a packet can show another).
    pub draw_program: u32,
    pub draw_class: u32,
}

/// Jobtype `0xECAED415`'s change-list ring, from [`Site::CellsChangeRing`]: a `u32` counter and the ring base.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellsChangeRing {
    /// The address of the counter global (how many entries the ring currently holds).
    pub counter: u32,
    /// The address of the ring: `[counter]` entries of 16 bytes each at the base.
    pub ring: u32,
}

impl CellsChangeRing {
    /// A ring entry's size in bytes (`shl eax, 4` before indexing).
    pub const ENTRY_BYTES: usize = 16;

    /// The `CRITICAL_SECTION` every writer of the ring enters around its push: the counter is the lock
    /// object's `+0x18` and the entries start at `+0x20` (the consumer `FUN_00518890` reads the counter as
    /// `[obj + 0x18]` with `obj = &DAT_00DD4770`, the object `FUN_00735840` and `FUN_00518830` push
    /// before `EnterCriticalSection`).
    pub fn critical_section(&self) -> u32 {
        self.counter - 0x18
    }
}

/// The Pimp job system's registry, from [`Site::JobRegister`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JobTables {
    /// The address of the jobtype count global: jobtypes `0..[count]` are registered.
    pub count: u32,
    /// The address of each CPU's handler table (`{handler, hash}` × [`JOB_SLOTS`]).
    pub handlers: [u32; 2],
}

/// The `IDirect3DDevice9*` field of the RenderSystem object (`mov eax, [eax+0x5bc]` in [`Site::BindDevice`]).
pub const RENDER_SYSTEM_DEVICE: u32 = 0x5bc;
/// Offset in the [`Site::BindDevice`] pattern of the `IDirect3D9` global's address (`mov eax, [moffs32]`).
const BIND_D3D_GLOBAL_AT: usize = 24;
/// Offset in the [`Site::BindDevice`] pattern of the RenderSystem global's address (`mov eax, [moffs32]`).
const BIND_RENDER_SYSTEM_AT: usize = 35;
/// Offset in the [`Site::BindDevice`] pattern of the device field's `disp32` (`mov eax, [eax+disp32]`).
const BIND_DEVICE_FIELD_AT: usize = 45;

impl Site {
    pub const ALL: [Site; 10] = [
        Site::RunFrame,
        Site::BindDevice,
        Site::JobRegister,
        Site::JobWait,
        Site::CellsChangeRing,
        Site::PacketProducer,
        Site::PacketFrameEnd,
        Site::FrameBegin,
        Site::StoreAlloc,
        Site::MatrixIndex,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Site::RunFrame => "RunFrame",
            Site::BindDevice => "BindDevice",
            Site::JobRegister => "JobRegister",
            Site::JobWait => "JobWait",
            Site::CellsChangeRing => "CellsChangeRing",
            Site::PacketProducer => "PacketProducer",
            Site::PacketFrameEnd => "PacketFrameEnd",
            Site::FrameBegin => "FrameBegin",
            Site::StoreAlloc => "StoreAlloc",
            Site::MatrixIndex => "MatrixIndex",
        }
    }

    /// The byte signature, as space-separated hex with `??` wildcards.
    fn pattern(self) -> &'static str {
        match self {
            // sub esp,0x14; push ebp; mov ebp,[QueryPerformanceCounter]; push edi; lea eax,[esp+0x10];
            // push eax; call ebp; cmp [first-frame state],1; jne +0x25; call <start-up>; mov eax,[settings];
            // movss xmm0,[1.0]; and [eax+0x40],~4; movss [eax+0x3c],xmm0; mov [first-frame state],2;
            // call <dt>; movss xmm1,[dt]
            Site::RunFrame => {
                "83 ec 14 55 8b 2d ?? ?? ?? ?? 57 8d 44 24 10 50 ff d5 83 3d ?? ?? ?? ?? 01 75 25 e8 ?? ?? ?? ?? \
                 a1 ?? ?? ?? ?? f3 0f 10 05 ?? ?? ?? ?? 83 60 40 fb f3 0f 11 40 3c c7 05 ?? ?? ?? ?? 02 00 00 00 \
                 e8 ?? ?? ?? ?? f3 0f 10 0d"
            }
            // push esi; push <critical section>; call [InitializeCriticalSection]; mov esi,[edi+0x2d20];
            // call <...>; mov eax,[IDirect3D9 global]; mov [edi+0x2d24],eax; mov eax,[RenderSystem global];
            // test eax,eax; je +0x0e; mov eax,[eax+0x5bc]; mov [edi+0x2d28],eax; pop esi; ret
            Site::BindDevice => {
                "56 68 ?? ?? ?? ?? ff 15 ?? ?? ?? ?? 8b b7 20 2d 00 00 e8 ?? ?? ?? ?? a1 ?? ?? ?? ?? 89 87 24 2d \
                 00 00 a1 ?? ?? ?? ?? 85 c0 74 0e 8b 80 bc 05 00 00 89 87 28 2d 00 00 5e c3"
            }
            // push ecx; mov eax,[count]; mov ecx,<handler>; mov [eax*8+cpu0 handlers],ecx; mov edx,0xec7aa7f0;
            // mov [eax*8+cpu0 handlers+4],edx; mov [eax*8+cpu1 handlers],ecx; mov [eax*8+cpu1 handlers+4],edx;
            // mov [jobtype id],eax; add eax,1; push <critical section>; mov [count],eax
            Site::JobRegister => {
                "51 a1 ?? ?? ?? ?? b9 ?? ?? ?? ?? 89 0c c5 ?? ?? ?? ?? ba f0 a7 7a ec 89 14 c5 ?? ?? ?? ?? \
                 89 0c c5 ?? ?? ?? ?? 89 14 c5 ?? ?? ?? ?? a3 ?? ?? ?? ?? 83 c0 01 68 ?? ?? ?? ?? a3 ?? ?? ?? ??"
            }
            // push ebp; mov ebp,esp; and esp,-8; sub esp,0x1c; jmp [SecuROM island]; mov ecx,[eax];
            // mov eax,[ecx+8]; mov ecx,[ebp+8]; imul eax,eax,300; mov dword [eax+busy flags],1;
            // mov edx,[ecx]; mov ecx,[ecx+4]
            Site::JobWait => {
                "55 8b ec 83 e4 f8 83 ec 1c ff 25 ?? ?? ?? ?? 8b 08 8b 41 08 8b 4d 08 69 c0 2c 01 00 00 \
                 c7 80 ?? ?? ?? ?? 01 00 00 00 8b 11 8b 49 04"
            }
            // mov eax,[counter]; mov ecx,[esp+0x80]; mov edx,[esp+0x84]; shl eax,4; add eax,ring; mov [eax],ecx;
            // mov ecx,[esp+0x88]; mov [eax+4],edx; mov edx,[esp+0x8c]; mov [eax+8],ecx; mov [eax+0xc],edx;
            // add dword [counter], 1
            Site::CellsChangeRing => {
                "a1 ?? ?? ?? ?? 8b 8c 24 80 00 00 00 8b 94 24 84 00 00 00 c1 e0 04 05 ?? ?? ?? ?? \
                 89 08 8b 8c 24 88 00 00 00 89 50 04 8b 94 24 8c 00 00 00 89 48 08 89 50 0c \
                 83 05 ?? ?? ?? ?? 01"
            }
            // mov ecx,[count]; cmp ecx,0x2000; push esi; mov esi,eax; je <return>; push edi; xor edi,edi;
            // cmp byte [state],0; jne +0x20
            Site::PacketProducer => "8b 0d ?? ?? ?? ?? 81 f9 00 20 00 00 56 8b f0 0f 84 ?? ?? ?? ?? 57 33 ff 80 3d ?? ?? ?? ?? 00 75 20",
            // push ebx; mov ebx,[count]; sete al; xor ecx,ecx; cmp [a],ecx; push esi; sete cl; xor edx,edx;
            // cmp [b],edx; mov [c],eax; sete dl; xor eax,eax; cmp [d],eax; push edi; mov edi,[buffer index];
            // sete al; test ebx,ebx; mov [a],ecx; mov [b],edx; mov [last frame],ebx; mov [d],eax; jg +9
            Site::PacketFrameEnd => {
                "53 8b 1d ?? ?? ?? ?? 0f 94 c0 33 c9 39 0d ?? ?? ?? ?? 56 0f 94 c1 33 d2 39 15 ?? ?? ?? ?? \
                 a3 ?? ?? ?? ?? 0f 94 c2 33 c0 39 05 ?? ?? ?? ?? 57 8b 3d ?? ?? ?? ?? 0f 94 c0 85 db \
                 89 0d ?? ?? ?? ?? 89 15 ?? ?? ?? ?? 89 1d ?? ?? ?? ?? a3 ?? ?? ?? ?? 7f 09"
            }
            // mov [packets],esi; mov word [a],si; mov [list cursor],esi; mov [sidecar cursor],esi;
            // mov word [b],0xc00
            // mov word [b],0xc00; mov [parity],ebx; mov [list base],ecx
            Site::FrameBegin => {
                "89 35 ?? ?? ?? ?? 66 89 35 ?? ?? ?? ?? 89 35 ?? ?? ?? ?? 89 35 ?? ?? ?? ?? 66 c7 05 ?? ?? ?? ?? 00 0c \
                 89 1d ?? ?? ?? ?? 89 0d ?? ?? ?? ??"
            }
            // mov eax,<list size>; call <pool alloc>; mov edi,eax; test edi,edi; jne +0x1a; mov ecx,[heap];
            // push esi; push <list size>; push <heap>; mov al,1; call <heap alloc>; mov edi,eax; push <cs>;
            // call ebx; mov esi,[n]; cmp esi,1; mov [ebp+list slots],edi; jle +0xf; cmp esi,0x20000; jle +7;
            // sar esi,1; cmp esi,1; jg -0xf; push <cs>; call [LeaveCriticalSection]; cmp byte [a],0; je +0x1b;
            // cmp byte [b],0; je +0x12; mov ecx,esi; mov eax,<sidecar size>; call <pool alloc>; mov edi,eax;
            // test edi,edi; jne +0x1a; mov ecx,[heap]; push esi; push <sidecar size>
            Site::StoreAlloc => {
                "b8 ?? ?? ?? ?? e8 ?? ?? ?? ?? 8b f8 85 ff 75 1a 8b 0d ?? ?? ?? ?? 56 68 ?? ?? ?? ?? 68 ?? ?? ?? ?? \
                 b0 01 e8 ?? ?? ?? ?? 8b f8 68 ?? ?? ?? ?? ff d3 8b 35 ?? ?? ?? ?? 83 fe 01 89 bd ?? ?? ?? ?? \
                 7e 0f 81 fe 00 00 02 00 7e 07 d1 fe 83 fe 01 7f f1 68 ?? ?? ?? ?? ff 15 ?? ?? ?? ?? \
                 80 3d ?? ?? ?? ?? 00 74 1b 80 3d ?? ?? ?? ?? 00 74 12 8b ce b8 ?? ?? ?? ?? e8 ?? ?? ?? ?? \
                 8b f8 85 ff 75 1a 8b 0d ?? ?? ?? ?? 56 68 ?? ?? ?? ??"
            }
            // push ebx; push esi; mov esi,ecx; movzx eax,word [esi+0x1c2]; mov cx,[esi+eax*4+0x1c4];
            // mov ax,[counter]; push edi; movzx edi,ax; add ax,1; mov [counter],ax; mov eax,[parity];
            // imul eax,eax,<per parity>; movzx edx,di; add eax,edx; shl eax,6; mov [a],cx; lea ecx,[esi+0x40]
            // (a twin push, `FUN_004A8B70`, has `lea ebx,[esi+0x40]` there)
            Site::MatrixIndex => {
                "53 56 8b f1 0f b7 86 c2 01 00 00 66 8b 8c 86 c4 01 00 00 66 a1 ?? ?? ?? ?? 57 0f b7 f8 \
                 66 05 01 00 66 a3 ?? ?? ?? ?? a1 ?? ?? ?? ?? 69 c0 ?? ?? ?? ?? 0f b7 d7 03 c2 c1 e0 06 \
                 66 89 0d ?? ?? ?? ?? 8d 4e 40 05 ?? ?? ?? ??"
            }
        }
    }
}

/// Where every site was found, and the globals read out of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProfilerSites {
    va: [u32; 10],
    render_system: u32,
    jobs: JobTables,
    cells_change: CellsChangeRing,
    frame_stores: FrameStores,
}

impl ProfilerSites {
    pub fn va(&self, s: Site) -> u32 {
        self.va[s as usize]
    }

    /// The address of the RenderSystem pointer global: the device is `[[render_system] + RENDER_SYSTEM_DEVICE]`
    /// once the global is non-null.
    pub fn render_system(&self) -> u32 {
        self.render_system
    }

    pub fn jobs(&self) -> JobTables {
        self.jobs
    }

    pub fn cells_change(&self) -> CellsChangeRing {
        self.cells_change
    }

    pub fn frame_stores(&self) -> FrameStores {
        self.frame_stores
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfilerSitesError {
    NoSection(&'static str),
    /// Each site's match count, when any is not exactly one.
    Matches(Vec<(Site, usize)>),
    /// [`Site::BindDevice`]'s operands are not the two adjacent globals it should read.
    BindDevice { site: u32, why: String },
    /// [`Site::JobRegister`]'s operands are not one count and two handler tables a control block apart.
    JobRegister { site: u32, why: String },
    /// [`Site::JobWait`]'s control blocks are not the ones [`Site::JobRegister`]'s handler tables sit in.
    JobWait { site: u32, why: String },
    /// [`Site::CellsChangeRing`]'s operands are not a counter and a ring 8 bytes apart.
    CellsChangeRing { site: u32, why: String },
    /// The frame-store sites do not name one set of counters.
    FrameStores { site: u32, why: String },
}

impl std::fmt::Display for ProfilerSitesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProfilerSitesError::NoSection(s) => write!(f, "the image has no {s} section"),
            ProfilerSitesError::Matches(m) => write!(
                f,
                "profiler sites did not resolve to exactly one match each: {}",
                m.iter().map(|(s, n)| format!("{}={n} match(es)", s.name())).collect::<Vec<_>>().join(", ")
            ),
            ProfilerSitesError::BindDevice { site, why } => write!(f, "BindDevice at {site:#x} is inconsistent: {why}"),
            ProfilerSitesError::JobRegister { site, why } => write!(f, "JobRegister at {site:#x} is inconsistent: {why}"),
            ProfilerSitesError::JobWait { site, why } => write!(f, "JobWait at {site:#x} is inconsistent: {why}"),
            ProfilerSitesError::CellsChangeRing { site, why } => write!(f, "CellsChangeRing at {site:#x} is inconsistent: {why}"),
            ProfilerSitesError::FrameStores { site, why } => write!(f, "the frame-store sites at {site:#x} are inconsistent: {why}"),
        }
    }
}

impl std::error::Error for ProfilerSitesError {}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// The count global and the two handler tables out of [`Site::JobRegister`] at `site` (`b` is its bytes).
fn job_tables(img: &Image, site: u32, b: &[u8]) -> Result<JobTables, ProfilerSitesError> {
    let bad = |why: String| ProfilerSitesError::JobRegister { site, why };
    let count = u32_at(b, JOB_COUNT_AT);
    let handlers = [u32_at(b, JOB_CPU0_HANDLER_AT), u32_at(b, JOB_CPU1_HANDLER_AT)];
    if u32_at(b, JOB_HASH_AT) != JOB_REGISTER_HASH {
        return Err(bad(format!("registers jobtype {:#x}, not {JOB_REGISTER_HASH:#x}", u32_at(b, JOB_HASH_AT))));
    }
    if u32_at(b, JOB_COUNT_STORE_AT) != count {
        return Err(bad(format!("reads the count at {count:#x} but writes it to {:#x}", u32_at(b, JOB_COUNT_STORE_AT))));
    }
    for (cpu, (handler, hash)) in [(JOB_CPU0_HANDLER_AT, JOB_CPU0_HASH_AT), (JOB_CPU1_HANDLER_AT, JOB_CPU1_HASH_AT)].into_iter().enumerate() {
        if u32_at(b, hash) != u32_at(b, handler).wrapping_add(4) {
            return Err(bad(format!("cpu {cpu}'s hash store {:#x} is not 4 bytes after its handler store {:#x}", u32_at(b, hash), u32_at(b, handler))));
        }
    }
    if handlers[1].wrapping_sub(handlers[0]) != JOB_BLOCK_STRIDE {
        return Err(bad(format!("the handler tables {:#x} and {:#x} are not one {JOB_BLOCK_STRIDE}-byte control block apart", handlers[0], handlers[1])));
    }
    for (what, va) in [("the count global", count), ("cpu 0's handler table", handlers[0]), ("cpu 1's last handler", handlers[1] + 8 * (JOB_SLOTS - 1) + 4)] {
        if img.u32_at(va).is_none() {
            return Err(bad(format!("{what} {va:#x} is not in the image")));
        }
    }
    Ok(JobTables { count, handlers })
}

/// Resolve every site in `img` and read the RenderSystem global out of [`Site::BindDevice`] and the job
/// tables out of [`Site::JobRegister`].
pub fn resolve(img: &Image) -> Result<ProfilerSites, ProfilerSitesError> {
    let text = img.section(".text").ok_or(ProfilerSitesError::NoSection(".text"))?;
    let found: Vec<(Site, Vec<u32>)> = Site::ALL
        .iter()
        .map(|&s| {
            let hits = Pattern::parse(s.pattern()).expect("built-in pattern parses").find_all(text.bytes);
            (s, hits.iter().map(|&o| text.va + o as u32).collect())
        })
        .collect();
    if !found.iter().all(|(_, v)| v.len() == 1) {
        return Err(ProfilerSitesError::Matches(found.iter().map(|(s, v)| (*s, v.len())).collect()));
    }
    let mut va = [0u32; 10];
    for (s, v) in &found {
        va[*s as usize] = v[0];
    }

    let bind = va[Site::BindDevice as usize];
    let at = (bind - text.va) as usize;
    let d3d = u32_at(text.bytes, at + BIND_D3D_GLOBAL_AT);
    let render_system = u32_at(text.bytes, at + BIND_RENDER_SYSTEM_AT);
    let field = u32_at(text.bytes, at + BIND_DEVICE_FIELD_AT);
    if field != RENDER_SYSTEM_DEVICE {
        return Err(ProfilerSitesError::BindDevice { site: bind, why: format!("reads the device at +{field:#x}, not +{RENDER_SYSTEM_DEVICE:#x}") });
    }
    if render_system != d3d.wrapping_add(4) {
        return Err(ProfilerSitesError::BindDevice {
            site: bind,
            why: format!("the RenderSystem global {render_system:#x} does not follow the IDirect3D9 global {d3d:#x}"),
        });
    }
    if img.u32_at(render_system).is_none() {
        return Err(ProfilerSitesError::BindDevice { site: bind, why: format!("the RenderSystem global {render_system:#x} is not in the image") });
    }
    let reg = va[Site::JobRegister as usize];
    let jobs = job_tables(img, reg, &text.bytes[(reg - text.va) as usize..])?;
    let wait = va[Site::JobWait as usize];
    let blocks = u32_at(text.bytes, (wait - text.va) as usize + JOB_WAIT_BLOCK_AT);
    if blocks != jobs.handlers[0].wrapping_sub(JOB_HANDLERS_AT) {
        return Err(ProfilerSitesError::JobWait {
            site: wait,
            why: format!(
                "its control blocks are at {blocks:#x}, but cpu 0's handler table {:#x} puts them at {:#x}",
                jobs.handlers[0],
                jobs.handlers[0].wrapping_sub(JOB_HANDLERS_AT)
            ),
        });
    }
    let ring = va[Site::CellsChangeRing as usize];
    let cells_change = cells_change_ring(img, ring, &text.bytes[(ring - text.va) as usize..])?;
    let frame_stores = frame_stores(img, text.va, text.bytes, &va)?;
    Ok(ProfilerSites { va, render_system, jobs, cells_change, frame_stores })
}

/// The frame stores' counters and capacities out of the five frame-store sites, each checked against the
/// others: the producer adds to the count it compares, the hand-off reads it and calls the function the
/// begin-frame site sits in, that function zeroes it and resets the matrix counter the matrix push uses,
/// and each allocation path of the allocator names one size.
fn frame_stores(img: &Image, text_va: u32, text: &[u8], va: &[u32; 10]) -> Result<FrameStores, ProfilerSitesError> {
    let at = |s: Site| va[s as usize];
    let bytes = |v: u32, n: usize| text.get((v.wrapping_sub(text_va)) as usize..).and_then(|b| b.get(..n));
    let producer = at(Site::PacketProducer);
    let bad = |why: String| ProfilerSitesError::FrameStores { site: producer, why };
    let b = bytes(producer, PACKET_COUNT_ADD_AT + 6).ok_or_else(|| bad("the producer runs past .text".into()))?;
    let packets = u32_at(b, PACKET_COUNT_AT);
    let packet_cap = u32_at(b, PACKET_CAP_AT);
    match &b[PACKET_COUNT_ADD_AT..] {
        [0x01, 0x1d, d @ ..] if u32::from_le_bytes([d[0], d[1], d[2], d[3]]) == packets => {}
        other => {
            return Err(bad(format!("the producer reads the count at {packets:#x}, but +{PACKET_COUNT_ADD_AT:#x} is {other:02x?}, not add [{packets:#x}], ebx")));
        }
    }

    let end = at(Site::PacketFrameEnd);
    let e = bytes(end, FRAME_END_BEGIN_CALL_AT + 5).ok_or_else(|| bad(format!("the hand-off at {end:#x} runs past .text")))?;
    if u32_at(e, FRAME_END_COUNT_AT) != packets {
        return Err(bad(format!("the producer counts at {packets:#x}, the frame hand-off reads {:#x}", u32_at(e, FRAME_END_COUNT_AT))));
    }
    // The begin-frame is the hand-off's last act: a tail `jmp rel32` in the retail builds, `call rel32` in
    // others; both transfer through the same rel32.
    if !matches!(e[FRAME_END_BEGIN_CALL_AT], 0xe8 | 0xe9) {
        return Err(bad(format!("the hand-off's +{FRAME_END_BEGIN_CALL_AT:#x} is {:#04x}, not call or jmp rel32", e[FRAME_END_BEGIN_CALL_AT])));
    }
    let call_end = end + FRAME_END_BEGIN_CALL_AT as u32 + 5;
    let called = call_end.wrapping_add(u32_at(e, FRAME_END_BEGIN_CALL_AT + 1));

    let site = at(Site::FrameBegin);
    let begin = site.wrapping_sub(BEGIN_SITE_IN_FUNCTION);
    if called != begin {
        return Err(bad(format!("the hand-off calls {called:#x}, but the begin-frame site {site:#x} puts its function at {begin:#x}")));
    }
    let r = bytes(site, BEGIN_LIST_BASE_AT + 4).ok_or_else(|| bad(format!("the begin-frame site {site:#x} runs past .text")))?;
    if u32_at(r, BEGIN_PACKETS_AT) != packets {
        return Err(bad(format!("the begin-frame zeroes {:#x}, not the packet count {packets:#x}", u32_at(r, BEGIN_PACKETS_AT))));
    }
    let list_dwords = u32_at(r, BEGIN_LIST_AT);
    let sidecar_bytes_used = u32_at(r, BEGIN_SIDECAR_AT);
    let parity = u32_at(r, BEGIN_PARITY_AT);
    let list_base = u32_at(r, BEGIN_LIST_BASE_AT);
    let side = begin + BEGIN_SIDECAR_BASE_IN_FUNCTION;
    let sb = bytes(side, 5).ok_or_else(|| bad(format!("the sidecar base store {side:#x} runs past .text")))?;
    if sb[0] != 0xa3 {
        return Err(bad(format!("the begin-frame's +{BEGIN_SIDECAR_BASE_IN_FUNCTION:#x} is {:#04x}, not mov [moffs32], eax", sb[0])));
    }
    let sidecar_base = u32_at(sb, 1);

    let (pp, ps, pa) = (&b[PACKET_PARITY_AT..], &b[PACKET_STRIDE_AT..], &b[PACKET_ARRAY_AT..]);
    if pp[..2] != [0x8b, 0x2d] || u32_at(pp, 2) != parity {
        return Err(bad(format!("the producer's +{PACKET_PARITY_AT:#x} is {:02x?}, not mov ebp, [{parity:#x}]", &pp[..6])));
    }
    if ps[..2] != [0x6b, 0xc0] {
        return Err(bad(format!("the producer's +{PACKET_STRIDE_AT:#x} is {:02x?}, not imul eax, eax, imm8", &ps[..3])));
    }
    let packet_bytes = u32::from(ps[2]);
    if pa[0] != 0x05 {
        return Err(bad(format!("the producer's +{PACKET_ARRAY_AT:#x} is {:#04x}, not add eax, imm32", pa[0])));
    }
    let packet_array = u32_at(pa, 1);
    let draw_class = u32_at(b, PACKET_CLASS_AT);
    let (pr, cr) = (&b[PACKET_PROGRAM_READ_AT..], &b[PACKET_CLASS_READ_AT..]);
    if pr[..3] != [0x0f, 0xb7, 0x15] {
        return Err(bad(format!("the producer's +{PACKET_PROGRAM_READ_AT:#x} is {:02x?}, not movzx edx, word [moffs32]", &pr[..7])));
    }
    let draw_program = u32_at(pr, 3);
    if cr[..2] != [0x8a, 0x15] || u32_at(cr, 2) != draw_class {
        return Err(bad(format!("the producer's +{PACKET_CLASS_READ_AT:#x} is {:02x?}, not mov dl, [{draw_class:#x}]", &cr[..6])));
    }
    let reset = begin + BEGIN_MATRIX_RESET_IN_FUNCTION;
    let m = bytes(reset, 9).ok_or_else(|| bad(format!("the matrix reset {reset:#x} runs past .text")))?;
    if m[..3] != [0x66, 0xc7, 0x05] || m[7..] != [0x02, 0x00] {
        return Err(bad(format!("the begin-frame's +{BEGIN_MATRIX_RESET_IN_FUNCTION:#x} is {m:02x?}, not mov word [counter], 2")));
    }
    let matrices = u32_at(m, 3);

    let push = at(Site::MatrixIndex);
    let p = bytes(push, MATRIX_BASE_AT + 4).ok_or_else(|| bad(format!("the matrix push {push:#x} runs past .text")))?;
    for o in [MATRIX_COUNTER_AT, MATRIX_COUNTER_STORE_AT] {
        if u32_at(p, o) != matrices {
            return Err(bad(format!("the matrix push uses {:#x}, the begin-frame resets {matrices:#x}", u32_at(p, o))));
        }
    }
    if u32_at(p, MATRIX_PARITY_AT) != parity {
        return Err(bad(format!("the matrix push reads the parity at {:#x}, the begin-frame writes {parity:#x}", u32_at(p, MATRIX_PARITY_AT))));
    }
    let matrix_cap = u32_at(p, MATRIX_CAP_AT);
    let matrix_table = u32_at(p, MATRIX_BASE_AT);

    let alloc = at(Site::StoreAlloc);
    let a = bytes(alloc, ALLOC_SIDECAR_AT[1] + 4).ok_or_else(|| bad(format!("the allocator {alloc:#x} runs past .text")))?;
    let size = |offsets: [usize; 2], what: &str| {
        let (x, y) = (u32_at(a, offsets[0]), u32_at(a, offsets[1]));
        if x == y { Ok(x) } else { Err(bad(format!("the allocator's two {what} paths take {x:#x} and {y:#x}"))) }
    };
    let list_bytes = size(ALLOC_LIST_AT, "display-list")?;
    let sidecar_bytes = size(ALLOC_SIDECAR_AT, "sidecar")?;

    let packet_end = packet_array as u64 + 2 * u64::from(packet_cap) * u64::from(packet_bytes) - 4;
    let matrix_end = matrix_table as u64 + 2 * u64::from(matrix_cap) * 0x40 - 4;
    for (what, v) in [
        ("the packet count", packets as u64),
        ("the list cursor", list_dwords as u64),
        ("the sidecar cursor", sidecar_bytes_used as u64),
        ("the matrix counter", matrices as u64),
        ("the parity", parity as u64),
        ("the list base pointer", list_base as u64),
        ("the sidecar base pointer", sidecar_base as u64),
        ("the draw program", draw_program as u64),
        ("the draw class", draw_class as u64),
        ("the packet array", packet_array as u64),
        ("the packet array's end", packet_end),
        ("the matrix table", matrix_table as u64),
        ("the matrix table's end", matrix_end),
    ] {
        if u32::try_from(v).ok().and_then(|v| img.u32_at(v)).is_none() {
            return Err(bad(format!("{what} {v:#x} is not in the image")));
        }
    }
    Ok(FrameStores {
        begin,
        packets,
        packet_cap,
        list_dwords,
        list_bytes,
        sidecar_bytes_used,
        sidecar_bytes,
        matrices,
        matrix_cap,
        parity,
        packet_array,
        packet_bytes,
        matrix_table,
        list_base,
        sidecar_base,
        draw_program,
        draw_class,
    })
}

/// The counter and ring addresses out of [`Site::CellsChangeRing`] at `site` (`b` is its bytes).
fn cells_change_ring(img: &Image, site: u32, b: &[u8]) -> Result<CellsChangeRing, ProfilerSitesError> {
    let bad = |why: String| ProfilerSitesError::CellsChangeRing { site, why };
    let counter = u32_at(b, CELLS_COUNTER_AT);
    let ring = u32_at(b, CELLS_RING_AT);
    if u32_at(b, CELLS_COUNTER_STORE_AT) != counter {
        return Err(bad(format!(
            "reads the counter at {counter:#x} but writes it at {:#x}",
            u32_at(b, CELLS_COUNTER_STORE_AT)
        )));
    }
    if ring != counter.wrapping_add(8) {
        return Err(bad(format!("ring {ring:#x} is not 8 bytes after the counter {counter:#x}")));
    }
    for (what, va) in [("the counter global", counter), ("the ring base", ring)] {
        if img.u32_at(va).is_none() {
            return Err(bad(format!("{what} {va:#x} is not in the image")));
        }
    }
    Ok(CellsChangeRing { counter, ring })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exe_image::synth;

    #[test]
    fn every_built_in_pattern_parses() {
        for s in Site::ALL {
            Pattern::parse(s.pattern()).unwrap_or_else(|e| panic!("{}: {e:?}", s.name()));
        }
        let bind: Vec<&str> = Site::BindDevice.pattern().split_whitespace().collect();
        assert_eq!(bind[BIND_D3D_GLOBAL_AT - 1], "a1");
        assert_eq!(bind[BIND_RENDER_SYSTEM_AT - 1], "a1");
        assert_eq!(bind[BIND_DEVICE_FIELD_AT..BIND_DEVICE_FIELD_AT + 4], ["bc", "05", "00", "00"]);
        let reg: Vec<&str> = Site::JobRegister.pattern().split_whitespace().collect();
        assert_eq!(reg.len(), JOB_COUNT_STORE_AT + 4);
        assert_eq!(reg[JOB_COUNT_AT - 1], "a1");
        assert_eq!(reg[JOB_COUNT_STORE_AT - 1], "a3");
        assert_eq!(reg[JOB_HASH_AT..JOB_HASH_AT + 4], ["f0", "a7", "7a", "ec"]);
        for at in [JOB_CPU0_HANDLER_AT, JOB_CPU0_HASH_AT, JOB_CPU1_HANDLER_AT, JOB_CPU1_HASH_AT] {
            assert_eq!(reg[at - 1], "c5", "the SIB byte before the disp32 at {at}");
        }
        assert_eq!(JOB_SLOTS, 36);
        let wait: Vec<&str> = Site::JobWait.pattern().split_whitespace().collect();
        assert_eq!(wait[JOB_WAIT_BLOCK_AT - 2..JOB_WAIT_BLOCK_AT], ["c7", "80"]);
        assert_eq!(wait[JOB_WAIT_BLOCK_AT + 4], "01");
        let ring: Vec<&str> = Site::CellsChangeRing.pattern().split_whitespace().collect();
        assert_eq!(ring[CELLS_COUNTER_AT - 1], "a1");
        assert_eq!(ring[CELLS_RING_AT - 1], "05");
        assert_eq!(ring[CELLS_COUNTER_STORE_AT - 2..CELLS_COUNTER_STORE_AT], ["83", "05"]);
        let producer: Vec<&str> = Site::PacketProducer.pattern().split_whitespace().collect();
        assert_eq!(producer[PACKET_COUNT_AT - 2..PACKET_COUNT_AT], ["8b", "0d"]);
        assert_eq!(producer[PACKET_CAP_AT - 2..PACKET_CAP_AT + 4], ["81", "f9", "00", "20", "00", "00"]);
        let end: Vec<&str> = Site::PacketFrameEnd.pattern().split_whitespace().collect();
        assert_eq!(end[FRAME_END_COUNT_AT - 2..FRAME_END_COUNT_AT], ["8b", "1d"]);
        let begin: Vec<&str> = Site::FrameBegin.pattern().split_whitespace().collect();
        for at in [BEGIN_PACKETS_AT, BEGIN_LIST_AT, BEGIN_SIDECAR_AT] {
            assert_eq!(begin[at - 2..at], ["89", "35"], "mov [global], esi before {at}");
        }
        let alloc: Vec<&str> = Site::StoreAlloc.pattern().split_whitespace().collect();
        for (at, op) in [(ALLOC_LIST_AT[0], "b8"), (ALLOC_LIST_AT[1], "68"), (ALLOC_SIDECAR_AT[0], "b8"), (ALLOC_SIDECAR_AT[1], "68")] {
            assert_eq!(alloc[at - 1], op, "the size operand at {at}");
            assert_eq!(alloc[at], "??", "the size at {at} is read, not matched");
        }
        assert_eq!(alloc.len(), ALLOC_SIDECAR_AT[1] + 4);
        let push: Vec<&str> = Site::MatrixIndex.pattern().split_whitespace().collect();
        assert_eq!(push[MATRIX_COUNTER_AT - 2..MATRIX_COUNTER_AT], ["66", "a1"]);
        assert_eq!(push[MATRIX_COUNTER_STORE_AT - 2..MATRIX_COUNTER_STORE_AT], ["66", "a3"]);
        assert_eq!(push[MATRIX_CAP_AT - 2..MATRIX_CAP_AT], ["69", "c0"]);
    }

    /// Where the frame-store sites sit in the synthetic `.text`.
    const PRODUCER_AT: usize = 0x500;
    const END_AT: usize = 0x800;
    const BEGIN_FN_AT: usize = 0x1200;
    const ALLOC_AT: usize = 0x1900;
    const PUSH_AT: usize = 0x1A00;

    /// What the five frame-store sites name: each counter's address in `.data`, the sizes, and the
    /// hand-off's call target (an offset in `.text`).
    #[derive(Clone)]
    struct Stores {
        producer_count: u32,
        producer_add: u32,
        end_count: u32,
        /// The hand-off's transfer opcode (`e9` tail jump, as retail, or `e8` call) and its target.
        transfer: u8,
        call_to: usize,
        begin_packets: u32,
        list: u32,
        sidecar: u32,
        reset_matrices: u32,
        push_matrices: u32,
        list_sizes: [u32; 2],
        sidecar_sizes: [u32; 2],
        matrix_cap: u32,
        /// The parity the begin-frame writes, the producer reads and the matrix push reads.
        begin_parity: u32,
        producer_parity: u32,
        push_parity: u32,
        packet_array: u32,
        matrix_table: u32,
        list_base: u32,
        sidecar_base: u32,
        /// The class global as the producer's first read (the pattern) and its second read name it.
        draw_class: u32,
        draw_class_read: u32,
        draw_program: u32,
    }

    fn stores() -> Stores {
        Stores {
            producer_count: 0x403030,
            producer_add: 0x403030,
            end_count: 0x403030,
            transfer: 0xe9,
            call_to: BEGIN_FN_AT,
            begin_packets: 0x403030,
            list: 0x403034,
            sidecar: 0x403038,
            reset_matrices: 0x40303c,
            push_matrices: 0x40303c,
            list_sizes: [0x50000; 2],
            sidecar_sizes: [0x20000; 2],
            matrix_cap: 0x1400,
            begin_parity: 0x403050,
            producer_parity: 0x403050,
            push_parity: 0x403050,
            packet_array: 0x1400000,
            matrix_table: 0x1570000,
            list_base: 0x403054,
            sidecar_base: 0x403058,
            draw_class: 0x403060,
            draw_class_read: 0x403060,
            draw_program: 0x40305c,
        }
    }

    fn put(b: &mut [u8], at: usize, v: u32) {
        b[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }

    /// The frame-store sites written into `text` (already at least `PUSH_AT` long).
    fn place_stores(text: &mut Vec<u8>, s: &Stores) {
        let mut producer = concrete(Site::PacketProducer, 0x66);
        put(&mut producer, PACKET_COUNT_AT, s.producer_count);
        producer.resize(PACKET_COUNT_ADD_AT, 0x90);
        producer[PACKET_PARITY_AT..PACKET_PARITY_AT + 2].copy_from_slice(&[0x8b, 0x2d]);
        put(&mut producer, PACKET_PARITY_AT + 2, s.producer_parity);
        producer[PACKET_STRIDE_AT..PACKET_STRIDE_AT + 3].copy_from_slice(&[0x6b, 0xc0, 0x58]);
        producer[PACKET_ARRAY_AT] = 0x05;
        put(&mut producer, PACKET_ARRAY_AT + 1, s.packet_array);
        put(&mut producer, PACKET_CLASS_AT, s.draw_class);
        producer[PACKET_PROGRAM_READ_AT..PACKET_PROGRAM_READ_AT + 3].copy_from_slice(&[0x0f, 0xb7, 0x15]);
        put(&mut producer, PACKET_PROGRAM_READ_AT + 3, s.draw_program);
        producer[PACKET_CLASS_READ_AT..PACKET_CLASS_READ_AT + 2].copy_from_slice(&[0x8a, 0x15]);
        put(&mut producer, PACKET_CLASS_READ_AT + 2, s.draw_class_read);
        producer.extend([0x01, 0x1d]);
        producer.extend(s.producer_add.to_le_bytes());
        let mut end = concrete(Site::PacketFrameEnd, 0x88);
        put(&mut end, FRAME_END_COUNT_AT, s.end_count);
        end.resize(FRAME_END_BEGIN_CALL_AT, 0x90);
        end.push(s.transfer);
        let rel = (s.call_to as i64 - (END_AT + FRAME_END_BEGIN_CALL_AT + 5) as i64) as i32;
        end.extend(rel.to_le_bytes());
        let mut begin = concrete(Site::FrameBegin, 0x99);
        put(&mut begin, BEGIN_PACKETS_AT, s.begin_packets);
        put(&mut begin, BEGIN_LIST_AT, s.list);
        put(&mut begin, BEGIN_SIDECAR_AT, s.sidecar);
        put(&mut begin, BEGIN_PARITY_AT, s.begin_parity);
        put(&mut begin, BEGIN_LIST_BASE_AT, s.list_base);
        let mut side = vec![0xa3];
        side.extend(s.sidecar_base.to_le_bytes());
        let mut reset = vec![0x66, 0xc7, 0x05];
        reset.extend(s.reset_matrices.to_le_bytes());
        reset.extend([0x02, 0x00]);
        let mut alloc = concrete(Site::StoreAlloc, 0x11);
        for (at, v) in ALLOC_LIST_AT.into_iter().zip(s.list_sizes).chain(ALLOC_SIDECAR_AT.into_iter().zip(s.sidecar_sizes)) {
            put(&mut alloc, at, v);
        }
        let mut push = concrete(Site::MatrixIndex, 0x22);
        put(&mut push, MATRIX_COUNTER_AT, s.push_matrices);
        put(&mut push, MATRIX_COUNTER_STORE_AT, s.push_matrices);
        put(&mut push, MATRIX_CAP_AT, s.matrix_cap);
        put(&mut push, MATRIX_PARITY_AT, s.push_parity);
        put(&mut push, MATRIX_BASE_AT, s.matrix_table);
        let site = BEGIN_FN_AT + BEGIN_SITE_IN_FUNCTION as usize;
        let reset_at = BEGIN_FN_AT + BEGIN_MATRIX_RESET_IN_FUNCTION as usize;
        let side_at = BEGIN_FN_AT + BEGIN_SIDECAR_BASE_IN_FUNCTION as usize;
        for (at, bytes) in [(PRODUCER_AT, producer), (END_AT, end), (side_at, side), (site, begin), (reset_at, reset), (ALLOC_AT, alloc), (PUSH_AT, push)] {
            text[at..at + bytes.len()].copy_from_slice(&bytes);
        }
    }

    /// The [`Site::CellsChangeRing`] bytes with counter at `counter` and ring at `counter + 8`.
    fn cells_ring(counter: u32) -> Vec<u8> {
        let mut b = concrete(Site::CellsChangeRing, 0x55);
        b[CELLS_COUNTER_AT..CELLS_COUNTER_AT + 4].copy_from_slice(&counter.to_le_bytes());
        b[CELLS_RING_AT..CELLS_RING_AT + 4].copy_from_slice(&(counter + 8).to_le_bytes());
        b[CELLS_COUNTER_STORE_AT..CELLS_COUNTER_STORE_AT + 4].copy_from_slice(&counter.to_le_bytes());
        b
    }

    /// The [`Site::JobWait`] bytes with control blocks at `blocks`.
    fn job_wait(blocks: u32) -> Vec<u8> {
        let mut b = concrete(Site::JobWait, 0x44);
        b[JOB_WAIT_BLOCK_AT..JOB_WAIT_BLOCK_AT + 4].copy_from_slice(&blocks.to_le_bytes());
        b
    }

    /// The pattern's fixed bytes, with every wildcard filled by `fill`.
    fn concrete(s: Site, fill: u8) -> Vec<u8> {
        s.pattern().split_ascii_whitespace().map(|t| if t == "??" { fill } else { u8::from_str_radix(t, 16).unwrap() }).collect()
    }

    /// The [`Site::JobRegister`] bytes: count global `count`, handler tables `cpu0` and `cpu1`.
    fn job_register(count: u32, cpu0: u32, cpu1: u32) -> Vec<u8> {
        let mut b = concrete(Site::JobRegister, 0x33);
        for (at, v) in [
            (JOB_COUNT_AT, count),
            (JOB_CPU0_HANDLER_AT, cpu0),
            (JOB_CPU0_HASH_AT, cpu0 + 4),
            (JOB_CPU1_HANDLER_AT, cpu1),
            (JOB_CPU1_HASH_AT, cpu1 + 4),
            (JOB_COUNT_STORE_AT, count),
        ] {
            b[at..at + 4].copy_from_slice(&v.to_le_bytes());
        }
        b
    }

    /// `.text` holding RunFrame at +0x40, BindDevice at +0x100 reading the globals `d3d` and `rs`,
    /// JobRegister at +0x200 (`reg`), JobWait at +0x300 (`wait`), CellsChangeRing at +0x400 (`ring`) and
    /// the frame-store sites of [`stores`]; a `.data` at 0x403000 of 0x400 bytes.
    fn image_with(d3d: u32, rs: u32, reg: Vec<u8>, wait: Vec<u8>, ring: Vec<u8>) -> Vec<u8> {
        image_with_stores(d3d, rs, reg, wait, ring, &stores())
    }

    fn image_with_stores(d3d: u32, rs: u32, reg: Vec<u8>, wait: Vec<u8>, ring: Vec<u8>, s: &Stores) -> Vec<u8> {
        let mut text = vec![0xcc; 0x40];
        text.extend(concrete(Site::RunFrame, 0x5a));
        text.resize(0x100, 0xcc);
        let mut bind = concrete(Site::BindDevice, 0x77);
        bind[BIND_D3D_GLOBAL_AT..BIND_D3D_GLOBAL_AT + 4].copy_from_slice(&d3d.to_le_bytes());
        bind[BIND_RENDER_SYSTEM_AT..BIND_RENDER_SYSTEM_AT + 4].copy_from_slice(&rs.to_le_bytes());
        text.extend(bind);
        text.resize(0x200, 0xcc);
        text.extend(reg);
        text.resize(0x300, 0xcc);
        text.extend(wait);
        text.resize(0x400, 0xcc);
        text.extend(ring);
        text.resize(0x1B00, 0xcc);
        place_stores(&mut text, s);
        // `.frame` holds both parities of the packet array (0x1400000) and of the matrix table (0x1570000), clear
        // of the addresses the other tests use as outside the image.
        synth::pe(0x400000, &[(".text", 0x1000, text), (".data", 0x3000, vec![0; 0x400]), (".frame", 0x1000000, vec![0; 0x300000])])
    }

    fn image(d3d: u32, rs: u32) -> Vec<u8> {
        image_with(
            d3d,
            rs,
            job_register(0x403010, 0x40304c, 0x40304c + JOB_BLOCK_STRIDE),
            job_wait(0x403040),
            cells_ring(0x403020),
        )
    }

    #[test]
    fn the_sites_resolve_and_name_the_render_system_global_and_the_job_tables() {
        let file = image(0x403004, 0x403008);
        let s = resolve(&Image::from_file(&file).unwrap()).unwrap();
        assert_eq!(s.va(Site::RunFrame), 0x401040);
        assert_eq!(s.va(Site::BindDevice), 0x401100);
        assert_eq!(s.va(Site::JobRegister), 0x401200);
        assert_eq!(s.va(Site::JobWait), 0x401300);
        assert_eq!(s.va(Site::CellsChangeRing), 0x401400);
        assert_eq!(s.render_system(), 0x403008);
        assert_eq!(s.jobs(), JobTables { count: 0x403010, handlers: [0x40304c, 0x403178] });
        assert_eq!(s.cells_change(), CellsChangeRing { counter: 0x403020, ring: 0x403028 });
        assert_eq!(s.cells_change().critical_section(), 0x403008);
        assert_eq!(s.va(Site::PacketProducer), 0x401500);
        assert_eq!(s.va(Site::PacketFrameEnd), 0x401800);
        assert_eq!(s.va(Site::FrameBegin), 0x402200 + BEGIN_SITE_IN_FUNCTION);
        assert_eq!(s.va(Site::StoreAlloc), 0x402900);
        assert_eq!(s.va(Site::MatrixIndex), 0x402a00);
        assert_eq!(
            s.frame_stores(),
            FrameStores {
                begin: 0x402200,
                packets: 0x403030,
                packet_cap: 0x2000,
                list_dwords: 0x403034,
                list_bytes: 0x50000,
                sidecar_bytes_used: 0x403038,
                sidecar_bytes: 0x20000,
                matrices: 0x40303c,
                matrix_cap: 0x1400,
                parity: 0x403050,
                packet_array: 0x1400000,
                packet_bytes: 0x58,
                matrix_table: 0x1570000,
                list_base: 0x403054,
                sidecar_base: 0x403058,
                draw_program: 0x40305c,
                draw_class: 0x403060,
            }
        );
    }

    #[test]
    fn the_frame_store_sites_must_name_one_set_of_counters() {
        let check = |s: Stores| {
            let file = image_with_stores(0x403004, 0x403008, job_register(0x403010, 0x40304c, 0x403178), job_wait(0x403040), cells_ring(0x403020), &s);
            resolve(&Image::from_file(&file).unwrap()).unwrap_err().to_string()
        };
        let e = check(Stores { producer_add: 0x403038, ..stores() });
        assert!(e.contains("not add [0x403030], ebx"), "{e}");
        let e = check(Stores { end_count: 0x40303c, ..stores() });
        assert!(e.contains("the frame hand-off reads 0x40303c"), "{e}");
        let e = check(Stores { call_to: BEGIN_FN_AT + 0x10, ..stores() });
        assert!(e.contains("the hand-off calls 0x402210"), "{e}");
        let e = check(Stores { transfer: 0x90, ..stores() });
        assert!(e.contains("is 0x90, not call or jmp rel32"), "{e}");
        let as_call = image_with_stores(
            0x403004,
            0x403008,
            job_register(0x403010, 0x40304c, 0x403178),
            job_wait(0x403040),
            cells_ring(0x403020),
            &Stores { transfer: 0xe8, ..stores() },
        );
        assert_eq!(resolve(&Image::from_file(&as_call).unwrap()).unwrap().frame_stores().begin, 0x402200, "a call reaches the begin-frame too");
        let e = check(Stores { begin_packets: 0x403034, ..stores() });
        assert!(e.contains("the begin-frame zeroes 0x403034"), "{e}");
        let e = check(Stores { push_matrices: 0x403038, ..stores() });
        assert!(e.contains("the matrix push uses 0x403038"), "{e}");
        let e = check(Stores { sidecar_sizes: [0x20000, 0x40000], ..stores() });
        assert!(e.contains("two sidecar paths take 0x20000 and 0x40000"), "{e}");
        let e = check(Stores { list: 0xc00000, ..stores() });
        assert!(e.contains("the list cursor 0xc00000 is not in the image"), "{e}");
        let e = check(Stores { producer_parity: 0x403054, ..stores() });
        assert!(e.contains("not mov ebp, [0x403050]"), "{e}");
        let e = check(Stores { push_parity: 0x403054, ..stores() });
        assert!(e.contains("the matrix push reads the parity at 0x403054"), "{e}");
        let e = check(Stores { matrix_table: 0x1670000, ..stores() });
        assert!(e.contains("the matrix table's end"), "{e}");
        let e = check(Stores { draw_class_read: 0x403064, ..stores() });
        assert!(e.contains("not mov dl, [0x403060]"), "{e}");
        let e = check(Stores { draw_program: 0x900000, ..stores() });
        assert!(e.contains("the draw program 0x900000 is not in the image"), "{e}");
    }

    #[test]
    fn job_wait_must_use_the_control_blocks_of_the_handler_tables() {
        let file = image_with(
            0x403004,
            0x403008,
            job_register(0x403010, 0x40304c, 0x403178),
            job_wait(0x403044),
            cells_ring(0x403020),
        );
        let e = resolve(&Image::from_file(&file).unwrap()).unwrap_err().to_string();
        assert!(e.contains("control blocks are at 0x403044") && e.contains("puts them at 0x403040"), "{e}");
    }

    #[test]
    fn cells_change_ring_names_a_counter_and_a_ring_8_bytes_apart() {
        let check = |ring: Vec<u8>| {
            resolve(
                &Image::from_file(&image_with(0x403004, 0x403008, job_register(0x403010, 0x40304c, 0x403178), job_wait(0x403040), ring))
                    .unwrap(),
            )
            .unwrap_err()
            .to_string()
        };
        // Reads counter at one address, writes it to another.
        let mut mismatched = cells_ring(0x403020);
        mismatched[CELLS_COUNTER_STORE_AT..CELLS_COUNTER_STORE_AT + 4].copy_from_slice(&0x403024u32.to_le_bytes());
        assert!(check(mismatched).contains("writes it at 0x403024"));
        // Ring is not 8 bytes after the counter.
        let mut split = cells_ring(0x403020);
        split[CELLS_RING_AT..CELLS_RING_AT + 4].copy_from_slice(&0x403030u32.to_le_bytes());
        assert!(check(split).contains("is not 8 bytes after"));
        // Counter is outside the image.
        assert!(check(cells_ring(0x900000)).contains("not in the image"));
    }

    #[test]
    fn job_register_must_name_one_count_and_two_tables_a_block_apart() {
        let check = |reg: Vec<u8>| {
            resolve(
                &Image::from_file(&image_with(0x403004, 0x403008, reg, job_wait(0x403040), cells_ring(0x403020))).unwrap(),
            )
            .unwrap_err()
            .to_string()
        };
        let e = check(job_register(0x403010, 0x40304c, 0x40304c + 0x100));
        assert!(e.contains("not one 300-byte control block apart"), "{e}");
        let mut other_count = job_register(0x403010, 0x40304c, 0x403178);
        other_count[JOB_COUNT_STORE_AT..JOB_COUNT_STORE_AT + 4].copy_from_slice(&0x403014u32.to_le_bytes());
        assert!(check(other_count).contains("writes it to 0x403014"));
        let mut skewed = job_register(0x403010, 0x40304c, 0x403178);
        skewed[JOB_CPU1_HASH_AT..JOB_CPU1_HASH_AT + 4].copy_from_slice(&0x403180u32.to_le_bytes());
        assert!(check(skewed).contains("cpu 1's hash store"));
        let e = check(job_register(0x403010, 0x40334c, 0x40334c + JOB_BLOCK_STRIDE));
        assert!(e.contains("cpu 1's last handler") && e.contains("is not in the image"), "{e}");
    }

    #[test]
    fn bind_device_must_read_adjacent_globals_inside_the_image() {
        let e = resolve(&Image::from_file(&image(0x403004, 0x40300c)).unwrap()).unwrap_err();
        assert!(e.to_string().contains("does not follow the IDirect3D9 global"), "{e}");
        let e = resolve(&Image::from_file(&image(0x500000, 0x500004)).unwrap()).unwrap_err();
        assert!(e.to_string().contains("is not in the image"), "{e}");
    }

    #[test]
    fn a_missing_or_doubled_site_reports_its_count() {
        let empty = synth::pe(0x400000, &[(".text", 0x1000, vec![0xcc; 64])]);
        let e = resolve(&Image::from_file(&empty).unwrap()).unwrap_err();
        assert_eq!(
            e,
            ProfilerSitesError::Matches(vec![
                (Site::RunFrame, 0),
                (Site::BindDevice, 0),
                (Site::JobRegister, 0),
                (Site::JobWait, 0),
                (Site::CellsChangeRing, 0),
                (Site::PacketProducer, 0),
                (Site::PacketFrameEnd, 0),
                (Site::FrameBegin, 0),
                (Site::StoreAlloc, 0),
                (Site::MatrixIndex, 0)
            ])
        );
        assert!(e.to_string().contains("RunFrame=0 match(es)"), "{e}");

        let mut text = concrete(Site::RunFrame, 0);
        text.extend(concrete(Site::RunFrame, 0xff));
        for s in &Site::ALL[1..] {
            text.extend(concrete(*s, 0));
        }
        let doubled = synth::pe(0x400000, &[(".text", 0x1000, text)]);
        let e = resolve(&Image::from_file(&doubled).unwrap()).unwrap_err();
        assert_eq!(
            e,
            ProfilerSitesError::Matches(vec![
                (Site::RunFrame, 2),
                (Site::BindDevice, 1),
                (Site::JobRegister, 1),
                (Site::JobWait, 1),
                (Site::CellsChangeRing, 1),
                (Site::PacketProducer, 1),
                (Site::PacketFrameEnd, 1),
                (Site::FrameBegin, 1),
                (Site::StoreAlloc, 1),
                (Site::MatrixIndex, 1)
            ])
        );
    }
}
