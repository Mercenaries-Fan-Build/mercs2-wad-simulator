//! The front end's option pages: what retail's `LTILibName` bindings and its `PauseItemChanged`
//! `key=value` dispatcher (`FUN_005C37E0`) do with the options movie's commands.
//!
//! Retail keeps the options as one record in three copies: the committed one (`0x00DFC320`, read from
//! `Mercs2.ini` at boot by `FUN_00753280` and written back by `FUN_00753D40`), the live one the pages
//! edit (`0x00DF6700`), and an undo copy for the advanced video page (`0x00DF6748`). A page copies
//! committed into live on entering, edits live, and Apply copies live back and saves.
//!
//! A command answers with the values to push into the movie (the Invoke funnel `FUN_0061C550`) and
//! the changes the rest of the engine applies now.

use mercs2_ui::FlashArg;

/// One option record (the copies above). Field comments give the record offset and INI key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Record {
    /// `+0x00` `[Render] Defaults`: the detail preset last applied, 0..3.
    pub defaults: i32,
    /// `+0x04` `D3DAdapter`.
    pub adapter: i32,
    /// `+0x08` `ScrW`, `+0x0C` `ScrH`, `+0x10` `DispFmt`, `+0x14` `RefreshRate`.
    pub width: u32,
    pub height: u32,
    pub disp_fmt: i32,
    pub refresh: u32,
    /// `+0x19` `Widescreen`.
    pub widescreen: bool,
    /// `+0x1C` `FSAA`: an index into the device's anti-aliasing levels.
    pub fsaa: u32,
    /// `+0x20` `Gamma`, 0..100.
    pub gamma: i32,
    /// `+0x24` `EnableWaterEffects`.
    pub water_effects: bool,
    /// `+0x25` `ShaderLevel` as read, and `+0x26` the flag the pages edit and the writer saves.
    pub shader_level: u8,
    pub high_shaders: bool,
    /// `+0x28` `ViewDistance`.
    pub view_distance: i32,
    /// `+0x2C` `PresentImmediate`: vsync off.
    pub present_immediate: bool,
    /// `+0x2D` `[Joystick] Invert`, `+0x31` `Rumble`.
    pub joy_invert: bool,
    pub rumble: bool,
    /// `+0x2E` `[Game] Subtitles`, `+0x2F` `AutoSave`, `+0x32` `Tutorials`.
    pub subtitles: bool,
    pub autosave: bool,
    pub tutorials: bool,
    /// `+0x35` `[Render] Subtitles`, `+0x36` `DetailLevel`, `+0x37` `WaterDetail`, `+0x38` `SkyDetail`.
    pub render_subtitles: bool,
    pub detail: u8,
    pub water_detail: u8,
    pub sky_detail: u8,
    /// `+0x3A..0x3E` `[Audio] SFXVolume`, `MusicVolume`, `DialogVolume`, `InVoiceVolume`,
    /// `OutVoiceVolume`: 0..100.
    pub sfx: i32,
    pub music: i32,
    pub dialog: i32,
    pub in_voice: i32,
    pub out_voice: i32,
    /// `+0x3F` `EnableScrub`, `+0x40` `EnableShadows`, `+0x41` `ModelDetailLevel`,
    /// `+0x42` `ParticleDetailLevel`, `+0x43` `MotionBlur`.
    pub scrub: bool,
    pub shadows: bool,
    pub model_detail: u8,
    pub particle_detail: u8,
    pub motion_blur: bool,
    /// `+0x45` `[Network] FriendlyFire`.
    pub friendly_fire: bool,
}

/// What the audio page's Defaults sets every volume to (`audioDefaults`, `FUN_005C37E0`), and the
/// INI reader's default for each.
const DEFAULT_VOLUME: i32 = 75;

/// The anti-aliasing names `AntiAliasText` shows, by level (`0x00D14698`).
const AA_TEXT: [&str; 6] = ["[0x7aebcaa8]", "2x", "4x", "8x", "16x", "32x"];

/// One display mode as retail packs it (`0x00754850`): `lo` = id | refresh << 8 | height << 16,
/// `hi` = width | bits per pixel << 16 | widescreen << 24. The list is sorted by `(hi, lo)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mode {
    pub lo: u32,
    pub hi: u32,
}

impl Mode {
    const NONE: Mode = Mode { lo: u32::MAX, hi: u32::MAX };

    fn id(self) -> u8 {
        self.lo as u8
    }
    pub fn refresh(self) -> u32 {
        (self.lo >> 8) & 0xFF
    }
    pub fn height(self) -> u32 {
        self.lo >> 16
    }
    pub fn width(self) -> u32 {
        self.hi & 0xFFFF
    }
    pub fn bpp(self) -> u32 {
        (self.hi >> 16) & 0xFF
    }
    pub fn widescreen(self) -> bool {
        self.hi >> 24 != 0
    }
    fn valid(self) -> bool {
        self.lo & self.hi != u32::MAX
    }
}

/// The modes retail lists from the adapter's (`0x00754850`): enumerated at 32 bits per pixel; at
/// least 640×480; at most 85 Hz; an aspect within 0.001 of 4:3 or 16:9; widescreen above 1.3433334;
/// a duplicate width/height/refresh dropped; sorted. `modes` is `(width, height, refresh Hz)` in
/// the order the display reports them, which gives each its id.
pub fn mode_list(modes: &[(u32, u32, u32)]) -> Vec<Mode> {
    let mut out: Vec<Mode> = Vec::new();
    for (index, &(width, height, refresh)) in modes.iter().enumerate() {
        if width < 640 || height < 480 || refresh > 85 {
            continue;
        }
        let aspect = width as f32 / height as f32;
        if (aspect - 4.0 / 3.0).abs() > 0.001 && (aspect - 16.0 / 9.0).abs() > 0.001 {
            continue;
        }
        if out.iter().any(|m| m.width() == width && m.height() == height && m.refresh() == refresh) {
            continue;
        }
        let widescreen = (aspect > 1.343_333_4) as u32;
        out.push(Mode { lo: (index as u32 & 0xFF) | refresh << 8 | height << 16, hi: width | 0x20 << 16 | widescreen << 24 });
    }
    out.sort_by_key(|m| ((m.hi as u64) << 32) | m.lo as u64);
    out
}

/// `FUN_00755500`: the mode with this width, height, bits per pixel and refresh.
fn find_mode(modes: &[Mode], width: u32, height: u32, bpp: u32, refresh: u32) -> Option<Mode> {
    modes.iter().copied().find(|m| m.width() == width && m.height() == height && m.bpp() == bpp && m.refresh() == refresh)
}

/// `FUN_00755590` (`forward`) and `FUN_00755770`: from `current` (found by its id), the next mode
/// either way, wrapping, that keeps the `keep` bits and changes at least one `change` bit; `current`
/// when none does.
fn step_mode(modes: &[Mode], current: Mode, keep: (u32, u32), change: (u32, u32), forward: bool) -> Mode {
    if modes.is_empty() || !current.valid() {
        return Mode::NONE;
    }
    let at = modes.iter().position(|m| m.id() == current.id()).unwrap_or(modes.len() - 1);
    let n = modes.len();
    for step in 1..n {
        let i = if forward { (at + step) % n } else { (at + n - step) % n };
        let m = modes[i];
        if m.lo & keep.0 == current.lo & keep.0
            && m.hi & keep.1 == current.hi & keep.1
            && (m.lo & change.0 != current.lo & change.0 || m.hi & change.1 != current.hi & change.1)
        {
            return m;
        }
    }
    current
}

/// The adapter, as the options read it.
#[derive(Clone, Debug)]
pub struct Device {
    /// PCI vendor and device (`+0x428`, `+0x42C` of the adapter record).
    pub vendor: u32,
    pub device: u32,
    /// The anti-aliasing levels it has (`[+0x5E0]+0x34`).
    pub aa_levels: u32,
    /// The capability bit `0x40` of `+0x5E4`, without which the high shaders are off.
    pub high_shaders: bool,
}

/// A change for the rest of the engine to make now.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// `FUN_005FAEE0(category, volume / 100)` → `Sound.SetCategoryVolume`.
    CategoryVolume { category: &'static str, volume: f32 },
    /// The gamma ramp `FUN_0074AE20(1.5 − gamma / 100)` builds, live as the slider moves.
    Gamma(i32),
    /// Apply's display mode and present interval (`FUN_0074C7A0`).
    Display { width: u32, height: u32, refresh: u32, present_immediate: bool },
    /// The keyboard binds were saved: the game reads `[Actions1]`/`[Actions2]` again.
    Bindings,
}

/// What one command asks of the engine.
#[derive(Debug, Default, PartialEq)]
pub struct Reply {
    /// Movie functions to call, in order, with their arguments.
    pub invokes: Vec<(&'static str, Vec<FlashArg>)>,
    pub effects: Vec<Effect>,
}

impl Reply {
    pub(crate) fn invoke(&mut self, name: &'static str, args: Vec<FlashArg>) {
        self.invokes.push((name, args));
    }
}

fn num(v: impl Into<f64>) -> FlashArg {
    FlashArg::Number(v.into())
}

pub struct Options {
    /// The `vz.wad` whose install holds `Mercs2.ini`.
    vz_wad: String,
    committed: Record,
    live: Record,
    /// The advanced page's undo copy, and whether it was entered (`0x01175F2A`) and accepted
    /// (`0x01175F2B`) since the video page last was.
    undo: Record,
    advanced_entered: bool,
    advanced_accepted: bool,
    /// The display: its modes, the mode in use (`[0x01176288]+0x5E0`), the pair the steppers move
    /// (`0x00EDAF80`), the refresh last picked (`0x00ED278F`), and the adapter.
    modes: Vec<Mode>,
    in_use: Mode,
    current: Mode,
    preferred_refresh: u32,
    device: Option<Device>,
    /// The control pages.
    controls: crate::controls::Controls,
}

impl Options {
    /// The committed record from the install's `Mercs2.ini`, a missing key taking the default
    /// retail's reader passes `GetPrivateProfileIntA` (`FUN_00753280`, `FUN_00753324`..), and the
    /// controls. `strings` is the front end's string table.
    pub fn load(vz_wad: &str, strings: std::rc::Rc<std::collections::HashMap<u32, String>>) -> Result<Options, String> {
        let int = |section: &str, key: &str, default: i64| -> Result<i64, String> { Ok(crate::ini::int(vz_wad, section, key)?.unwrap_or(default)) };
        let flag = |section: &str, key: &str, default: i64| -> Result<bool, String> { Ok(int(section, key, default)? != 0) };
        let level = |key: &str, default: i64| -> Result<u8, String> { Ok(int("Render", key, default)?.clamp(0, 2) as u8) };
        let volume = |key: &str| -> Result<i32, String> { Ok(int("Audio", key, DEFAULT_VOLUME as i64)? as i32) };
        let shader_level = int("Render", "ShaderLevel", 1)? as u8;
        let committed = Record {
            defaults: int("Render", "Defaults", 0)?.clamp(0, 3) as i32,
            adapter: int("Render", "D3DAdapter", 0)? as i32,
            width: int("Render", "ScrW", 0x400)? as u32,
            height: int("Render", "ScrH", 0x300)? as u32,
            disp_fmt: int("Render", "DispFmt", 1)? as i32,
            refresh: int("Render", "RefreshRate", 0x3C)? as u32,
            widescreen: flag("Render", "Widescreen", 0)?,
            fsaa: int("Render", "FSAA", 0)? as u32,
            gamma: int("Render", "Gamma", 0x32)? as i32,
            water_effects: flag("Render", "EnableWaterEffects", 0)?,
            shader_level,
            high_shaders: shader_level != 0,
            view_distance: int("Render", "ViewDistance", 100)? as i32,
            present_immediate: flag("Render", "PresentImmediate", 0)?,
            joy_invert: flag("Joystick", "Invert", 0)?,
            rumble: flag("Joystick", "Rumble", 1)?,
            subtitles: flag("Game", "Subtitles", 1)?,
            autosave: flag("Game", "AutoSave", 1)?,
            tutorials: flag("Game", "Tutorials", 1)?,
            render_subtitles: flag("Render", "Subtitles", 1)?,
            detail: int("Render", "DetailLevel", 2)? as u8,
            water_detail: level("WaterDetail", 2)?,
            sky_detail: level("SkyDetail", 2)?,
            sfx: volume("SFXVolume")?,
            music: volume("MusicVolume")?,
            dialog: volume("DialogVolume")?,
            in_voice: volume("InVoiceVolume")?,
            out_voice: volume("OutVoiceVolume")?,
            scrub: flag("Render", "EnableScrub", 1)?,
            shadows: flag("Render", "EnableShadows", 1)?,
            model_detail: level("ModelDetailLevel", 1)?,
            particle_detail: level("ParticleDetailLevel", 1)?,
            motion_blur: flag("Render", "MotionBlur", 1)?,
            friendly_fire: flag("Network", "FriendlyFire", 1)?,
        };
        Ok(Options {
            vz_wad: vz_wad.to_string(),
            committed,
            live: committed,
            undo: committed,
            advanced_entered: false,
            advanced_accepted: false,
            modes: Vec::new(),
            in_use: Mode::NONE,
            current: Mode::NONE,
            preferred_refresh: committed.refresh,
            device: None,
            controls: crate::controls::Controls::load(vz_wad, strings)?,
        })
    }

    pub fn committed(&self) -> &Record {
        &self.committed
    }

    /// Whether a key down now answers a rebind (`FUN_004FA570`).
    pub fn capturing_key(&self) -> bool {
        self.controls.capturing()
    }

    /// The key that answers a rebind.
    pub fn key_down(&mut self, key: u16) -> Result<Reply, String> {
        self.controls.key_down(key)
    }

    /// The pads, the first the one the control pages edit.
    pub fn attach_pads(&mut self, pads: &[crate::controls::Pad]) -> Result<(), String> {
        self.controls.attach_pads(pads)
    }

    /// One frame of the first pad's inputs held, by joystick vocabulary index.
    pub fn pad_frame(&mut self, held: &[u8]) -> Result<Reply, String> {
        self.controls.pad_frame(held)
    }

    /// The display's modes and the adapter, and the mode the committed record names in use: the one
    /// with its size and refresh at `(DispFmt + 1) << 4` bits, else 640×480 at 60 Hz with `DispFmt`
    /// 1, as `FUN_0074C7A0` falls back (`0x0074CB1E`).
    pub fn attach_display(&mut self, modes: &[(u32, u32, u32)], device: Device) -> Result<(), String> {
        self.modes = mode_list(modes);
        self.device = Some(device);
        self.in_use = self.mode_for_committed()?;
        Ok(())
    }

    fn mode_for_committed(&mut self) -> Result<Mode, String> {
        let r = self.committed;
        let bpp = ((r.disp_fmt + 1) << 4) as u32;
        if let Some(mode) = find_mode(&self.modes, r.width, r.height, bpp, r.refresh) {
            return Ok(mode);
        }
        self.committed.width = 640;
        self.committed.height = 480;
        self.committed.disp_fmt = 1;
        self.committed.refresh = 60;
        find_mode(&self.modes, 640, 480, 0x20, 60).ok_or_else(|| "the display has neither the INI's mode nor 640x480 at 60 Hz".to_string())
    }

    fn device(&self) -> Result<&Device, String> {
        self.device.as_ref().ok_or_else(|| "the video page needs the display attached".to_string())
    }

    /// One `LTILibName` call: `arg` is its single argument as text when it takes one.
    pub fn call(&mut self, name: &str, arg: Option<&str>) -> Result<Reply, String> {
        let mut reply = Reply::default();
        let number = || -> Result<f64, String> { arg.ok_or_else(|| format!("{name} takes an argument"))?.trim().parse::<f64>().map_err(|e| format!("{name}({arg:?}): {e}")) };
        match name {
            // `FUN_005C0A20`.
            "LTIVideoEnter" => {
                self.device()?;
                if !self.advanced_entered {
                    self.live = self.committed;
                    self.current = self.in_use;
                } else if !self.advanced_accepted {
                    self.live = self.undo;
                }
                self.advanced_entered = false;
                self.advanced_accepted = false;
                self.live.widescreen = self.current.widescreen();
                self.preferred_refresh = self.current.refresh();
                self.push_video(&mut reply)?;
            }
            // `FUN_005C0AF0`: the other aspect at the same depth.
            "LTIVideoSwitchMode" => {
                let was = self.current.widescreen();
                let next = step_mode(&self.modes, self.current, (0, 0x00FF_0000), (0, 0xFF00_0000), true);
                self.take_mode(next);
                if next.widescreen() != was {
                    self.live.widescreen = next.widescreen();
                    reply.invoke("LTIVideoSetMode", vec![num(self.live.widescreen as u8)]);
                    self.live.view_distance = if self.live.widescreen {
                        self.live.view_distance * 4 / 3
                    } else {
                        (self.live.view_distance as f64 * 0.75).round_ties_even() as i32
                    };
                    reply.invoke("LTIVideoSetVideoViewDistance", vec![num(self.live.view_distance)]);
                }
                reply.invoke("LTIVideoSetRes", vec![self.res_text()]);
                reply.invoke("LTIVideoSetRefresh", vec![num(self.live.refresh)]);
            }
            // `FUN_005C0CE0`/`FUN_005C0E10`: another size at the same aspect and depth, keeping the
            // refresh last picked where the size has it (`0x004F5DE0`).
            "LTIVideoNextRes" | "LTIVideoPrevRes" => {
                let mut next = step_mode(&self.modes, self.current, (0, 0xFFFF_0000), (0xFFFF_0000, 0xFFFF), name == "LTIVideoNextRes");
                if next.valid() && next.refresh() != self.preferred_refresh {
                    if let Some(kept) = find_mode(&self.modes, next.width(), next.height(), next.bpp(), self.preferred_refresh) {
                        next = kept;
                    }
                }
                self.take_mode(next);
                let res = self.res_text();
                reply.invoke("LTIVideoSetRes", vec![res.clone()]);
                reply.invoke("LTIVideoSetResolution", vec![res]);
                reply.invoke("LTIVideoSetRefresh", vec![num(self.live.refresh)]);
            }
            // `FUN_005C0F40`/`FUN_005C1000`: another refresh at this size.
            "LTIVideoNextRefresh" | "LTIVideoPrevRefresh" => {
                let next = step_mode(&self.modes, self.current, (0xFFFF_0000, 0xFFFF_FFFF), (0xFF00, 0), name == "LTIVideoNextRefresh");
                self.take_mode(next);
                self.preferred_refresh = next.refresh();
                reply.invoke("LTIVideoSetRefresh", vec![num(self.live.refresh)]);
            }
            "LTIVideoSetGamma" => {
                self.live.gamma = number()? as i32;
                reply.effects.push(Effect::Gamma(self.live.gamma));
            }
            // `FUN_005C1140`: `FUN_0074C7A0` then the writer.
            "LTIVideoApplyChanges" => {
                self.committed = self.live;
                self.in_use = self.mode_for_committed()?;
                self.save()?;
                let r = self.committed;
                reply.effects.push(Effect::Display { width: r.width, height: r.height, refresh: r.refresh, present_immediate: r.present_immediate });
            }
            // `FUN_005C1160`.
            "LTIVideoDefault" => {
                let preset = default_preset(self.device()?);
                self.apply_preset(preset);
                self.advanced_defaults(&mut reply)?;
                self.push_video(&mut reply)?;
            }
            // `FUN_005C11D0`: only the gamma goes back.
            "LTIVideoCancel" => reply.effects.push(Effect::Gamma(self.committed.gamma)),
            // `FUN_005C1210`.
            "LTIVideoAdvanceEnter" => {
                self.undo = self.live;
                self.advanced_entered = true;
                self.push_advanced(&mut reply)?;
            }
            // `FUN_005C13A0`.
            "LTIVideoAdvanceDefault" => self.advanced_defaults(&mut reply)?,
            // `FUN_005C1240`: the tens pick the option, live and committed both, saved at once.
            "LTIVideoSwitchOpt1" => {
                let v = number()? as i32;
                self.switch_option(v)?;
            }
            // `FUN_0063EF20`: `return 1`.
            "LTIVideoGetViewDistance" => {}
            other => {
                return match self.controls.call(other, arg, &mut self.committed)? {
                    Some(reply) => Ok(reply),
                    None => Err(format!("LTILibName.{other} has no handler")),
                }
            }
        }
        Ok(reply)
    }

    fn take_mode(&mut self, mode: Mode) {
        self.current = mode;
        self.live.width = mode.width();
        self.live.height = mode.height();
        self.live.refresh = mode.refresh();
    }

    fn res_text(&self) -> FlashArg {
        FlashArg::Text(format!("{} x {}", self.live.width, self.live.height))
    }

    /// `FUN_005C0870`.
    fn push_video(&self, reply: &mut Reply) -> Result<(), String> {
        let r = self.live;
        reply.invoke("LTIVideoSetMode", vec![num(r.widescreen as u8)]);
        reply.invoke("LTIVideoSetRes", vec![self.res_text()]);
        reply.invoke("LTIVideoSetRefresh", vec![num(r.refresh)]);
        reply.invoke("LTIVideoSetGamma", vec![num(r.gamma)]);
        reply.invoke("LTIVideoSetVideoViewDistance", vec![num(r.view_distance)]);
        reply.invoke("LTISetGraphicDetail", vec![num(r.detail)]);
        let aa = AA_TEXT.get(r.fsaa as usize).ok_or_else(|| format!("FSAA {} has no name", r.fsaa))?;
        reply.invoke("AntiAliasText", vec![FlashArg::Text((*aa).into())]);
        reply.invoke("LTIVideoSetVSync", vec![num(!r.present_immediate as u8)]);
        Ok(())
    }

    /// `FUN_005C06C0`.
    fn push_advanced(&self, reply: &mut Reply) -> Result<(), String> {
        let r = self.live;
        reply.invoke("LTIVideoSetSwitchOpt1", vec![FlashArg::Bool(r.water_effects)]);
        reply.invoke("LTIVideoSetSwitchOpt2", vec![num(r.water_detail)]);
        reply.invoke("LTIVideoSetSwitchOpt3", vec![num(r.sky_detail)]);
        reply.invoke("LTIVideoSetSwitchOpt4", vec![FlashArg::Bool(r.shadows)]);
        reply.invoke("LTIVideoSetSwitchOpt5", vec![num(r.model_detail)]);
        reply.invoke("LTIVideoSetSwitchOpt6", vec![num(r.particle_detail)]);
        reply.invoke("LTIVideoSetSwitchOpt7", vec![FlashArg::Bool(r.high_shaders)]);
        reply.invoke("LTIVideoSetSwitchOpt8", vec![FlashArg::Bool(r.motion_blur)]);
        if !self.device()?.high_shaders {
            reply.invoke("LTIVideoDisableHighShaders", vec![FlashArg::Bool(false)]);
        }
        Ok(())
    }

    /// `FUN_007546E0` on the live record: preset 1, 2 or 3 sets the mode and every detail level.
    fn apply_preset(&mut self, preset: i32) {
        let ati = self.device.as_ref().is_some_and(|d| d.vendor == 0x1002);
        let r = &mut self.live;
        r.defaults = preset;
        let level = match preset {
            1 => 0,
            2 => 1,
            3 => 2,
            _ => return,
        };
        (r.width, r.height) = [(640, 480), (800, 600), (1024, 768)][level as usize];
        r.sky_detail = level;
        r.shadows = level > 0;
        r.particle_detail = level;
        r.model_detail = level;
        r.shader_level = (level > 0) as u8;
        r.detail = level;
        r.water_detail = level;
        r.disp_fmt = 1;
        r.refresh = 60;
        r.widescreen = false;
        r.fsaa = 0;
        r.gamma = 50;
        r.render_subtitles = true;
        r.present_immediate = false;
        if preset == 3 {
            r.water_effects = !ati;
            r.motion_blur = true;
            r.scrub = true;
            r.view_distance = 75;
        } else {
            r.water_effects = false;
            r.motion_blur = false;
            r.scrub = false;
            r.view_distance = 20;
        }
        self.current = Mode { hi: self.current.hi & 0x00FF_FFFF, ..self.current };
    }

    /// `FUN_005C13A0`: the advanced options for the live detail level, the high shaders off on a
    /// device without them, then the advanced page pushed (`0x005C06C0`).
    fn advanced_defaults(&mut self, reply: &mut Reply) -> Result<(), String> {
        self.set_advanced_defaults()?;
        self.push_advanced(reply)
    }

    fn set_advanced_defaults(&mut self) -> Result<(), String> {
        let high = self.device()?.high_shaders;
        let r = &mut self.live;
        let level = r.detail;
        match level {
            2 => {
                (r.water_detail, r.water_effects, r.high_shaders, r.shadows) = (2, true, true, true);
                (r.model_detail, r.particle_detail, r.motion_blur, r.sky_detail, r.scrub) = (2, 2, true, 2, true);
            }
            1 => {
                (r.water_detail, r.high_shaders, r.shadows, r.model_detail, r.particle_detail, r.sky_detail) = (1, true, true, 1, 1, 1);
                (r.scrub, r.motion_blur, r.water_effects) = (false, false, false);
            }
            _ => {
                (r.water_detail, r.high_shaders, r.shadows, r.model_detail, r.particle_detail, r.sky_detail) = (0, false, false, 0, 0, 0);
                (r.scrub, r.motion_blur, r.water_effects) = (false, false, false);
            }
        }
        if !high {
            r.high_shaders = false;
        }
        Ok(())
    }

    fn switch_option(&mut self, v: i32) -> Result<(), String> {
        let (live, committed) = (&mut self.live, &mut self.committed);
        let key = match v {
            ..=19 => {
                live.water_effects = v == 11;
                committed.water_effects = live.water_effects;
                ("EnableWaterEffects", live.water_effects as i64)
            }
            20..=29 => {
                live.water_detail = (v - 20).clamp(0, 2) as u8;
                committed.water_detail = live.water_detail;
                ("WaterDetail", live.water_detail as i64)
            }
            30..=39 => {
                live.sky_detail = (v - 30) as u8;
                committed.sky_detail = live.sky_detail;
                ("SkyDetail", live.sky_detail as i64)
            }
            40..=49 => {
                live.shadows = v == 41;
                committed.shadows = live.shadows;
                ("EnableShadows", live.shadows as i64)
            }
            50..=59 => {
                live.model_detail = (v - 50) as u8;
                committed.model_detail = live.model_detail;
                ("ModelDetailLevel", live.model_detail as i64)
            }
            60..=69 => {
                live.particle_detail = (v - 60) as u8;
                committed.particle_detail = live.particle_detail;
                ("ParticleDetailLevel", live.particle_detail as i64)
            }
            70..=79 => {
                live.motion_blur = v == 71;
                committed.motion_blur = live.motion_blur;
                ("MotionBlur", live.motion_blur as i64)
            }
            _ => return Ok(()),
        };
        crate::ini::write(&self.vz_wad, &[("Render", key.0, key.1.to_string())])
    }

    /// `LTILibName.LTIPauseItemChanged(s)`: `s` is a control word or `key=value`, split at the first
    /// `=`; booleans are `_stricmp(value, "true") == 0`.
    pub fn pause_item_changed(&mut self, s: &str) -> Result<Reply, String> {
        let mut reply = Reply::default();
        let (key, value) = match s.split_once('=') {
            Some((k, v)) => (k, Some(v)),
            None => (s, None),
        };
        let truth = || value.is_some_and(|v| v.eq_ignore_ascii_case("true"));
        let number = || atoi(value.unwrap_or(""));
        match (key, value.is_some()) {
            // Game page.
            ("gameEnter", false) => {
                self.live = self.committed;
                self.push_game(&mut reply, |b| FlashArg::Number(b as i32 as f64));
            }
            ("gameTutorialsVar", true) => self.live.tutorials = truth(),
            ("gameSubtitleVar", true) => self.live.subtitles = truth(),
            ("gameAutoSaveVar", true) => self.live.autosave = truth(),
            ("gameApply" | "gameAccept", false) => {
                self.committed = self.live;
                self.save()?;
            }
            ("gameCancel", false) => {}
            ("gameDefaults", false) => {
                self.committed.tutorials = true;
                self.committed.subtitles = true;
                self.committed.autosave = true;
                self.live = self.committed;
                self.push_game(&mut reply, FlashArg::Bool);
            }
            // Audio page.
            ("audioEnter", false) => {
                self.live = self.committed;
                self.push_audio(&mut reply);
            }
            ("audioSFXVolumeVar", true) => {
                self.live.sfx = number();
                reply.effects.push(volume("sfx", self.live.sfx));
            }
            ("audioMusicVolumeVar", true) => {
                self.live.music = number();
                reply.effects.push(volume("music", self.live.music));
            }
            ("audioDialogVolumeVar", true) => {
                self.live.dialog = number();
                reply.effects.push(volume("vo", self.live.dialog));
            }
            ("audioInVoiceVolumeVar", true) => self.live.in_voice = number(),
            ("audioOutVoiceVolumeVar", true) => self.live.out_voice = number(),
            ("audioApply", false) => {
                self.committed = self.live;
                self.save()?;
            }
            ("audioDefaults", false) => {
                for v in [&mut self.live.sfx, &mut self.live.music, &mut self.live.dialog, &mut self.live.in_voice, &mut self.live.out_voice] {
                    *v = DEFAULT_VOLUME;
                }
                self.push_audio(&mut reply);
            }
            ("audioCancel", false) => {
                let (live, committed) = (self.live, self.committed);
                for (category, now, saved) in [("sfx", live.sfx, committed.sfx), ("music", live.music, committed.music), ("vo", live.dialog, committed.dialog)] {
                    if now != saved {
                        reply.effects.push(volume(category, saved));
                    }
                }
            }
            // Video page (`0x005C4FE3`..`0x005C5620`).
            ("videoNextAntiAliasLevel", false) => {
                let levels = self.device()?.aa_levels;
                self.live.fsaa += 1;
                if self.live.fsaa >= levels {
                    self.live.fsaa = 0;
                }
                self.push_aa(&mut reply)?;
            }
            ("videoPrevAntiAliasLevel", false) => {
                if self.live.fsaa == 0 {
                    self.live.fsaa = self.device()?.aa_levels;
                }
                self.live.fsaa = self.live.fsaa.saturating_sub(1);
                self.push_aa(&mut reply)?;
            }
            ("VSyncOn", false) => self.live.present_immediate = false,
            ("VSyncOff", false) => self.live.present_immediate = true,
            ("videoVsyncVar", true) => self.live.present_immediate = !truth(),
            ("videoVideoDetailVar", true) => {
                self.live.detail = number() as u8;
                self.advanced_defaults(&mut reply)?;
                self.apply_preset(self.live.detail as i32 + 1);
                self.push_video(&mut reply)?;
            }
            ("videoViewDistance", true) => self.live.view_distance = number() + 10,
            ("videoShaderSetting", true) => {
                self.committed.high_shaders = truth();
                self.save()?;
            }
            ("AdvAccept", false) => self.advanced_accepted = true,
            // Server page (`0x005C4D7C`..`0x005C4E16`): the committed record, saved by the next Apply.
            ("serverFriendlyFireVar", true) => self.committed.friendly_fire = truth(),
            ("serverEnter", false) => reply.invoke("serverFriendlyFire", vec![num(self.committed.friendly_fire as u8)]),
            _ => return Err(format!("PauseItemChanged {s:?} has no handler")),
        }
        Ok(reply)
    }

    fn push_aa(&self, reply: &mut Reply) -> Result<(), String> {
        let aa = AA_TEXT.get(self.live.fsaa as usize).ok_or_else(|| format!("FSAA {} has no name", self.live.fsaa))?;
        reply.invoke("AntiAliasText", vec![FlashArg::Text((*aa).into())]);
        reply.invoke("setAntiAliasVar", vec![num(self.live.fsaa)]);
        Ok(())
    }

    /// `gameEnter` pushes the three as numbers, `gameDefaults` as booleans.
    fn push_game(&self, reply: &mut Reply, arg: impl Fn(bool) -> FlashArg) {
        reply.invoke("gameTutorials", vec![arg(self.live.tutorials)]);
        reply.invoke("videoSubtitles", vec![arg(self.live.subtitles)]);
        reply.invoke("gameAutosave", vec![arg(self.live.autosave)]);
    }

    fn push_audio(&self, reply: &mut Reply) {
        let r = self.live;
        for (name, v) in [
            ("LTIAudioSFXVolume", r.sfx),
            ("LTIAudioMusicVolume", r.music),
            ("LTIAudioDialogVolume", r.dialog),
            ("LTIAudioInVoiceVolume", r.in_voice),
            ("LTIAudioOutVoiceVolume", r.out_voice),
        ] {
            reply.invoke(name, vec![num(v)]);
        }
    }

    /// `FUN_00753D40`: every key of the committed record, as `%i`; `FirstRun` written 0 and
    /// `ShaderLevel` as the edited flag (`0x0075428C`).
    fn save(&self) -> Result<(), String> {
        let r = self.committed;
        let i = |v: i64| v.to_string();
        let b = |v: bool| (v as i64).to_string();
        crate::ini::write(
            &self.vz_wad,
            &[
                ("Render", "Defaults", i(r.defaults as i64)),
                ("Render", "FirstRun", i(0)),
                ("Render", "D3DAdapter", i(r.adapter as i64)),
                ("Render", "ScrW", i(r.width as i64)),
                ("Render", "ScrH", i(r.height as i64)),
                ("Render", "DispFmt", i(r.disp_fmt as i64)),
                ("Render", "RefreshRate", i(r.refresh as i64)),
                ("Render", "Widescreen", b(r.widescreen)),
                ("Render", "FSAA", i(r.fsaa as i64)),
                ("Render", "Gamma", i(r.gamma as i64)),
                ("Render", "EnableWaterEffects", b(r.water_effects)),
                ("Render", "PresentImmediate", b(r.present_immediate)),
                ("Render", "WaterDetail", i(r.water_detail as i64)),
                ("Render", "SkyDetail", i(r.sky_detail as i64)),
                ("Render", "EnableScrub", b(r.scrub)),
                ("Render", "EnableShadows", b(r.shadows)),
                ("Render", "ParticleDetailLevel", i(r.particle_detail as i64)),
                ("Render", "ModelDetailLevel", i(r.model_detail as i64)),
                ("Render", "ViewDistance", i(r.view_distance as i64)),
                ("Render", "ShaderLevel", b(r.high_shaders)),
                ("Render", "MotionBlur", b(r.motion_blur)),
                ("Render", "Subtitles", b(r.render_subtitles)),
                ("Render", "DetailLevel", i(r.detail as i64)),
                ("Joystick", "Invert", b(r.joy_invert)),
                ("Joystick", "Rumble", b(r.rumble)),
                ("Game", "Subtitles", b(r.subtitles)),
                ("Game", "AutoSave", b(r.autosave)),
                ("Game", "Tutorials", b(r.tutorials)),
                ("Audio", "SFXVolume", i(r.sfx as i64)),
                ("Audio", "MusicVolume", i(r.music as i64)),
                ("Audio", "DialogVolume", i(r.dialog as i64)),
                ("Audio", "InVoiceVolume", i(r.in_voice as i64)),
                ("Audio", "OutVoiceVolume", i(r.out_voice as i64)),
                ("Network", "FriendlyFire", b(r.friendly_fire)),
            ],
        )
    }
}

/// `FUN_005C1160`'s choice: preset 1 on a low-end card (`0x005E6820`, `0x005E69C0`, `0x0074BC70`,
/// `0x0074C380`), else 2 on one of the ATI cards `0x0074C520` lists, else 3.
pub fn default_preset(device: &Device) -> i32 {
    let (vendor, id) = (device.vendor, device.device);
    let nvidia = vendor == 0x10DE;
    let ati = vendor == 0x1002;
    let low = (nvidia && (NV_LOW_A.contains(&id) || NV_LOW_B.contains(&id) || (0x290..=0x299).contains(&id)))
        || ANY_LOW.contains(&id)
        || (ati && (ATI_LOW.contains(&id) || (0x9596..=0x9599).contains(&id)));
    if low {
        1
    } else if ati && ATI_MID.contains(&id) {
        2
    } else {
        3
    }
}

/// `0x005E6820` (`0x02487D20`, `0x004131A0`, `0x00471BF0`, `0x006BCDB0`).
const NV_LOW_A: &[u32] = &[
    0x3D0, 0x3D1, 0x3D2, 0x242, 0x3D5, 0x241, 0x245, 0x240, 0x14F, 0xF3, 0x221, 0x163, 0x162, 0x161, 0x222, 0x160, 0x141, 0x142, 0x143, 0xF1, 0xF4, 0x140, 0xF2, 0x147, 0x41,
    0xC1, 0x47, 0xF6, 0xC0, 0x45, 0xF9, 0xC2, 0x40, 0x43, 0x48, 0x218, 0xC3, 0x42, 0x44, 0x46, 0x211, 0x212, 0x215,
];
/// `0x005E69C0` (`0x004EF9B0`, `0x00464740`, `0x004D7690`), with the range `0x290..=0x299`.
const NV_LOW_B: &[u32] = &[0x1D1, 0x1D3, 0x1DF, 0x393, 0x395, 0x2E2, 0x1DD, 0x391, 0x392, 0x2E1, 0x394, 0x2E0, 0x90, 0x92, 0x91, 0x93, 0x95, 0xF5];
/// `0x0074BC70`: whatever the vendor.
const ANY_LOW: &[u32] = &[0x6E2, 0x420, 0x84F, 0x849, 0x84B, 0x423, 0x848, 0x422, 0x404, 0x424, 0x6E4, 0x421, 0x402, 0x401, 0x400, 0x403];
/// `0x0074C380` and the helpers it and `0x0074C150` call, with the range `0x9596..=0x9599`.
const ATI_LOW: &[u32] = &[
    0x714E, 0x715E, 0x714D, 0x71C3, 0x718F, 0x716E, 0x717E, 0x716D, 0x71E3, 0x71AF, 0x7146, 0x7166, 0x7142, 0x7180, 0x7183, 0x7187, 0x7162, 0x71A0, 0x71A3, 0x71A7, 0x718B, 0x718C,
    0x7196, 0x7149, 0x714A, 0x714B, 0x714C, 0x7186, 0x718D, 0x7145, 0x7193, 0x7143, 0x71B3, 0x7163, 0x719F, 0x7147, 0x715F, 0x7167, 0x717F, 0x71CE, 0x71EE, 0x71C6, 0x7291, 0x7293,
    0x71C1, 0x71C7, 0x71E6, 0x72B1, 0x72B3, 0x71E1, 0x71E7, 0x71C0, 0x71C2, 0x7140, 0x71CD, 0x7181, 0x71E0, 0x71E2, 0x7160, 0x71ED, 0x71A1, 0x71C5, 0x71D5, 0x71DE, 0x71D6, 0x71DA,
    0x71FA, 0x71D4, 0x71C4, 0x94CB, 0x94C5, 0x94C9, 0x94C8, 0x94C4, 0x94C3, 0x94C1, 0x71D2, 0x71F2, 0x7153, 0x7173, 0x7152, 0x7172, 0x7280, 0x7240, 0x7244, 0x72A0, 0x7260, 0x7264,
    0x7248, 0x7249, 0x724A, 0x724B, 0x724C, 0x724D, 0x724F, 0x7243, 0x7245, 0x7246, 0x7247, 0x7268, 0x7269, 0x726A, 0x726B, 0x726C, 0x726D, 0x726F, 0x7263, 0x7265, 0x7266, 0x7267,
    0x7105, 0x7108, 0x7109, 0x710A, 0x710B, 0x710C, 0x7100, 0x7120, 0x7125, 0x7128, 0x7129, 0x712A, 0x712B, 0x712C, 0x7288, 0x72A8, 0x7284, 0x7102, 0x7101, 0x710F, 0x712F, 0x710E,
    0x712E, 0x7104, 0x7124, 0x7103, 0x9590, 0x95C0, 0x95C5, 0x95C7, 0x9614, 0x9612, 0x9610,
];
/// `0x0074C520`.
const ATI_MID: &[u32] = &[0x958C, 0x958B, 0x9583, 0x9588, 0x9586, 0x9589, 0x9587, 0x958D, 0x9581, 0x958E];

fn volume(category: &'static str, value: i32) -> Effect {
    Effect::CategoryVolume { category, volume: value as f32 * 0.01 }
}

/// C `atoi`: leading blanks, an optional sign, then digits; anything else ends it. The audio sliders
/// send their position divided by 2.5, so the text can carry a fraction (`"28.4"` → 28).
pub(crate) fn atoi(s: &str) -> i32 {
    let s = s.trim_start();
    let (sign, digits) = match s.as_bytes().first() {
        Some(b'-') => (-1, &s[1..]),
        Some(b'+') => (1, &s[1..]),
        _ => (1, s),
    };
    let value = digits.bytes().take_while(u8::is_ascii_digit).fold(0i64, |v, d| (v * 10 + (d - b'0') as i64).min(i32::MAX as i64));
    (sign * value) as i32
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn record() -> Record {
        Record {
            defaults: 3,
            adapter: 0,
            width: 1600,
            height: 900,
            disp_fmt: 1,
            refresh: 60,
            widescreen: true,
            fsaa: 0,
            gamma: 50,
            water_effects: true,
            shader_level: 0,
            high_shaders: false,
            view_distance: 100,
            present_immediate: false,
            joy_invert: false,
            rumble: true,
            subtitles: false,
            autosave: true,
            tutorials: true,
            render_subtitles: true,
            detail: 2,
            water_detail: 2,
            sky_detail: 2,
            sfx: 28,
            music: 29,
            dialog: 33,
            in_voice: 23,
            out_voice: 18,
            scrub: true,
            shadows: true,
            model_detail: 2,
            particle_detail: 2,
            motion_blur: false,
            friendly_fire: false,
        }
    }

    fn options() -> Options {
        let r = record();
        Options {
            vz_wad: String::new(),
            committed: r,
            live: r,
            undo: r,
            advanced_entered: false,
            advanced_accepted: false,
            modes: Vec::new(),
            in_use: Mode::NONE,
            current: Mode::NONE,
            preferred_refresh: 60,
            device: None,
            controls: crate::controls::Controls::detached(Default::default()),
        }
    }

    fn attached() -> Options {
        let mut o = options();
        let modes = [(640, 480, 60), (1600, 900, 60), (1600, 900, 75), (1280, 720, 60), (1024, 768, 60), (1280, 1024, 60), (1920, 1080, 120)];
        o.modes = mode_list(&modes);
        o.device = Some(Device { vendor: 0x106B, device: 0, aa_levels: 3, high_shaders: true });
        o.in_use = find_mode(&o.modes, 1600, 900, 0x20, 60).expect("mode");
        o
    }

    #[test]
    fn atoi_reads_the_sliders_text() {
        assert_eq!((atoi("28.4"), atoi(" 7"), atoi("-3x"), atoi("")), (28, 7, -3, 0));
    }

    #[test]
    fn entering_the_audio_page_pushes_the_five_volumes() {
        let reply = options().pause_item_changed("audioEnter").unwrap();
        assert_eq!(reply.invokes[0], ("LTIAudioSFXVolume", vec![FlashArg::Number(28.0)]));
        assert_eq!(reply.invokes[4], ("LTIAudioOutVoiceVolume", vec![FlashArg::Number(18.0)]));
    }

    #[test]
    fn a_slider_moves_its_category_and_cancel_puts_it_back() {
        let mut o = options();
        let moved = o.pause_item_changed("audioMusicVolumeVar=80.4").unwrap();
        assert_eq!(moved.effects, vec![volume("music", 80)]);
        let undone = o.pause_item_changed("audioCancel").unwrap();
        assert_eq!(undone.effects, vec![volume("music", 29)]);
    }

    #[test]
    fn game_toggles_are_live_until_entered_again() {
        let mut o = options();
        o.pause_item_changed("gameSubtitleVar=TRUE").unwrap();
        assert!(o.live.subtitles && !o.committed.subtitles);
        let entered = o.pause_item_changed("gameEnter").unwrap();
        assert!(!o.live.subtitles);
        assert_eq!(entered.invokes[1], ("videoSubtitles", vec![FlashArg::Number(0.0)]));
    }

    #[test]
    fn the_mode_list_keeps_4_3_and_16_9_up_to_85_hz_sorted() {
        let list = mode_list(&[(1920, 1080, 120), (1280, 1024, 60), (1280, 720, 60), (800, 600, 60), (1280, 720, 60), (512, 384, 60)]);
        let sizes: Vec<_> = list.iter().map(|m| (m.width(), m.height(), m.refresh(), m.widescreen())).collect();
        assert_eq!(sizes, [(800, 600, 60, false), (1280, 720, 60, true)]);
    }

    #[test]
    fn entering_the_video_page_pushes_its_eight_values() {
        let mut o = attached();
        let reply = o.call("LTIVideoEnter", None).unwrap();
        let names: Vec<_> = reply.invokes.iter().map(|(n, _)| *n).collect();
        assert_eq!(names, ["LTIVideoSetMode", "LTIVideoSetRes", "LTIVideoSetRefresh", "LTIVideoSetGamma", "LTIVideoSetVideoViewDistance", "LTISetGraphicDetail", "AntiAliasText", "LTIVideoSetVSync"]);
        assert_eq!(reply.invokes[1].1, vec![FlashArg::Text("1600 x 900".into())]);
    }

    #[test]
    fn stepping_resolution_keeps_the_refresh_picked() {
        let mut o = attached();
        o.call("LTIVideoEnter", None).unwrap();
        o.call("LTIVideoNextRefresh", None).unwrap();
        assert_eq!(o.live.refresh, 75);
        o.call("LTIVideoNextRes", None).unwrap();
        assert_eq!((o.live.width, o.live.height, o.live.refresh), (1280, 720, 60));
        o.call("LTIVideoNextRes", None).unwrap();
        assert_eq!((o.live.width, o.live.height, o.live.refresh), (1600, 900, 75));
    }

    #[test]
    fn switching_the_mode_rescales_the_view_distance() {
        let mut o = attached();
        o.call("LTIVideoEnter", None).unwrap();
        let reply = o.call("LTIVideoSwitchMode", None).unwrap();
        assert!(!o.live.widescreen);
        assert_eq!(o.live.view_distance, 75);
        assert_eq!(reply.invokes[0], ("LTIVideoSetMode", vec![FlashArg::Number(0.0)]));
    }

    #[test]
    fn anti_aliasing_wraps_over_the_device_levels() {
        let mut o = attached();
        o.pause_item_changed("videoPrevAntiAliasLevel").unwrap();
        assert_eq!(o.live.fsaa, 2);
        o.pause_item_changed("videoNextAntiAliasLevel").unwrap();
        assert_eq!(o.live.fsaa, 0);
    }

    #[test]
    fn defaults_pick_the_preset_by_card() {
        let card = |vendor, device| Device { vendor, device, aa_levels: 1, high_shaders: true };
        assert_eq!(default_preset(&card(0x10DE, 0x3D0)), 1);
        assert_eq!(default_preset(&card(0x10DE, 0x295)), 1);
        assert_eq!(default_preset(&card(0x1002, 0x958C)), 2);
        assert_eq!(default_preset(&card(0x1002, 0x9597)), 1);
        assert_eq!(default_preset(&card(0x106B, 0x420)), 1);
        assert_eq!(default_preset(&card(0x106B, 0x1234)), 3);
    }

    #[test]
    fn advanced_options_take_the_tens_digit() {
        let mut o = attached();
        o.live.detail = 1;
        o.set_advanced_defaults().unwrap();
        assert_eq!((o.live.water_detail, o.live.shadows, o.live.scrub, o.live.motion_blur), (1, true, false, false));
    }

    #[test]
    fn an_unknown_key_is_an_error() {
        assert!(options().pause_item_changed("videoGammaVar=3").is_err());
    }
}
