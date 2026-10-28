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
    pub const ALL: [Site; 5] =
        [Site::RunFrame, Site::BindDevice, Site::JobRegister, Site::JobWait, Site::CellsChangeRing];

    pub fn name(self) -> &'static str {
        match self {
            Site::RunFrame => "RunFrame",
            Site::BindDevice => "BindDevice",
            Site::JobRegister => "JobRegister",
            Site::JobWait => "JobWait",
            Site::CellsChangeRing => "CellsChangeRing",
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
        }
    }
}

/// Where every site was found, and the globals read out of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProfilerSites {
    va: [u32; 5],
    render_system: u32,
    jobs: JobTables,
    cells_change: CellsChangeRing,
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
    let mut va = [0u32; 5];
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
    Ok(ProfilerSites { va, render_system, jobs, cells_change })
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
    /// JobRegister at +0x200 (`reg`), JobWait at +0x300 (`wait`) and CellsChangeRing at +0x400 (`ring`);
    /// a `.data` at 0x403000 of 0x400 bytes.
    fn image_with(d3d: u32, rs: u32, reg: Vec<u8>, wait: Vec<u8>, ring: Vec<u8>) -> Vec<u8> {
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
        text.resize(0x500, 0xcc);
        synth::pe(0x400000, &[(".text", 0x1000, text), (".data", 0x3000, vec![0; 0x400])])
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
                (Site::CellsChangeRing, 0)
            ])
        );
        assert!(e.to_string().contains("RunFrame=0 match(es)"), "{e}");

        let mut text = concrete(Site::RunFrame, 0);
        text.extend(concrete(Site::RunFrame, 0xff));
        text.extend(concrete(Site::BindDevice, 0));
        text.extend(concrete(Site::JobRegister, 0));
        text.extend(concrete(Site::JobWait, 0));
        text.extend(concrete(Site::CellsChangeRing, 0));
        let doubled = synth::pe(0x400000, &[(".text", 0x1000, text)]);
        let e = resolve(&Image::from_file(&doubled).unwrap()).unwrap_err();
        assert_eq!(
            e,
            ProfilerSitesError::Matches(vec![
                (Site::RunFrame, 2),
                (Site::BindDevice, 1),
                (Site::JobRegister, 1),
                (Site::JobWait, 1),
                (Site::CellsChangeRing, 1)
            ])
        );
    }
}
