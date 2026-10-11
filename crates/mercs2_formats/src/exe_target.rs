//! Addresses in the target Mercenaries 2 exe (image base 0x00400000) that m2-sdk.dll uses, as published
//! to C mods in `m2/m2_target.h`.

pub const RDATA_START_VA: u32 = 0x00B0_5000;
pub const RDATA_SIZE: u32 = 0x000F_1000;
pub const TEXT_START_VA: u32 = 0x0040_1000;
pub const TEXT_SIZE: u32 = 0x0070_3000;

/// The shared no-op log stub (`33 C0 C3`) that the stripped log functions, Lua `print` and
/// `Debug.Printf` among them, funnel through.
pub const LOG_STUB_VA: u32 = 0x006D_5640;

/// FUN_0084f130: builds the shader registry. `__thiscall` (ECX = renderer), one stack argument, `ret 4`.
/// Its only caller is FUN_007492d0 @0x0074957A, the renderer constructor.
pub const SHADER_REGISTER_ALL_VA: u32 = 0x0084_F130;
/// FUN_0085ac90: `__thiscall(record, name, sho, class)`, `ret 0xc`. Sets record+4 = hash(name),
/// record+0x8c = class, copies sho to record+0xb, then calls the load handler vtbl[+8].
pub const SHADER_REGISTER_VA: u32 = 0x0085_AC90;
/// FUN_0085ace0: pixel-record base constructor. `__thiscall` (ECX = record), `ret`.
pub const SHADER_PS_BASE_CTOR_VA: u32 = 0x0085_ACE0;
/// FUN_0085ade0: vertex-record base constructor. usercall, EAX = record, `ret`; clobbers ECX only.
pub const SHADER_VS_BASE_CTOR_VA: u32 = 0x0085_ADE0;

pub const SHADER_PROLOGUE_LEN: usize = 16;
pub const SHADER_REGISTER_ALL_PROLOGUE: [u8; SHADER_PROLOGUE_LEN] =
    [0x55, 0x8B, 0xEC, 0x83, 0xE4, 0xF8, 0x53, 0x56, 0x68, 0x38, 0x7A, 0x97, 0x01, 0xB8, 0x3C, 0x34];
pub const SHADER_REGISTER_PROLOGUE: [u8; SHADER_PROLOGUE_LEN] =
    [0x8B, 0x54, 0x24, 0x04, 0x56, 0x57, 0x8B, 0xF1, 0xE8, 0xD3, 0x95, 0xFC, 0xFF, 0x89, 0x46, 0x04];
pub const SHADER_PS_BASE_CTOR_PROLOGUE: [u8; SHADER_PROLOGUE_LEN] =
    [0x8B, 0xC1, 0x33, 0xC9, 0x66, 0xC7, 0x40, 0x08, 0xFF, 0xFF, 0xC6, 0x40, 0x0A, 0x01, 0x89, 0x88];
pub const SHADER_VS_BASE_CTOR_PROLOGUE: [u8; SHADER_PROLOGUE_LEN] =
    [0x33, 0xC9, 0x66, 0xC7, 0x40, 0x08, 0xFF, 0xFF, 0xC6, 0x40, 0x0A, 0x01, 0x89, 0x88, 0x90, 0x00];

/// The shader pool (DAT_01977a38), zeroed by its static initializer FUN_0085ab00. The counts are
/// incremented only by the inserts FUN_0085b7c0 (pixel) and FUN_00632250 (vertex).
pub const SHADER_POOL_VA: u32 = 0x0197_7A38;
pub const SHADER_PS_COUNT_VA: u32 = 0x0197_7A38;
pub const SHADER_PS_KEYS_VA: u32 = 0x0197_9A40;
pub const SHADER_VS_COUNT_VA: u32 = 0x0197_DA40;
pub const SHADER_VS_KEYS_VA: u32 = 0x0197_DE48;
/// Slot counts of the two key tables. The inserts probe `& 0x7ff` / `& 0xff` without giving up, so an
/// insert into a full table never terminates.
pub const SHADER_PS_CAPACITY: u32 = 0x800;
pub const SHADER_VS_CAPACITY: u32 = 0x100;

/// u8 in the render settings block 0x00dfc320: ShaderLevel after the caps check (FUN_00753280,
/// FUN_0074c7ac). The game registers pixel classes 1-3 only when it is non-zero.
pub const SHADER_LEVEL_VA: u32 = 0x00DF_C345;

/// FUN_0085ac90 copies the sho name to record+0xb and the class lives at +0x8c, so a sho name holds at
/// most 0x8c - 0xb - 1 = 128 characters.
pub const SHADER_SHO_MAX_LEN: usize = 128;
