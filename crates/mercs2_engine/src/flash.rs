//! The Flash widgets: one [`Player`] per widget, driven by what the scripts queue on the widget tree.
//!
//! Retail's widget update (`FUN_0061C5B0`) flushes the queued button input, then the analog values,
//! then the held-key repeat, and the movie advances after it. The button table is `FUN_0061CAC0`, read
//! from its jump table at `0x0061CD50`: every button sends a key event, Press and Hold a key down and
//! Release a key up.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use mercs2_formats::gfx::GfxMovie;
use mercs2_formats::gfx_def::MovieDef;
use mercs2_gfx::avm::{FsCommandHook, Player, Value};
use mercs2_gfx::stage::{Item, Movie};
use mercs2_gfx::text::FontLib;
use mercs2_script::ScriptHost;
use mercs2_ui::{FlashAction, FlashArg, FlashRequest, WidgetTree};

use crate::gfx_pass::WadImages;
use crate::script_host::GameScriptHost;

/// `[0x00BBC7EC]`: how long an axis holds before its key repeats.
const REPEAT_DELAY: f32 = 0.25;
/// `[0x00B977F8]`: the analog magnitude that needs a zero sent after it.
const ANALOG_EPSILON: f32 = 0.0001;

/// The key of a button. Button 6 is the confirm button, `[0x00D1E50C]`, and button 8 the other face
/// button; the only write to that global is 6, so the swapped pair is not modelled.
fn key_of(button: i64) -> u32 {
    match button {
        1 => 0x26,
        2 => 0x28,
        3 => 0x25,
        4 => 0x27,
        5 => 0x24,
        6 => 0x0D,
        7 => 0x08,
        8 => 0x10,
        9 => 0x41,
        10 => 0x44,
        11 => 0x57,
        12 => 0x53,
        13 => 0x4C,
        14 => 0xDE,
        15 => 0x50,
        16 => 0xBA,
        17 => 0x51,
        19 => 0x45,
        20 => 0xDB,
        22 => 0x4F,
        _ => 0,
    }
}

/// The held axis of a button, and the way it pushes it.
fn axis_of(button: i64) -> Option<(usize, i32)> {
    match button {
        3 => Some((0, -1)),
        4 => Some((0, 1)),
        1 => Some((1, -1)),
        2 => Some((1, 1)),
        9 => Some((2, -1)),
        10 => Some((2, 1)),
        11 => Some((3, -1)),
        12 => Some((3, 1)),
        _ => None,
    }
}

/// The keys of an axis, by its direction: `FUN_0061C820`.
const AXIS_KEYS: [[u32; 2]; 4] = [[0x25, 0x27], [0x26, 0x28], [0x41, 0x44], [0x57, 0x53]];

#[derive(Default)]
struct Axis {
    value: i32,
    held: f32,
}

#[derive(Default)]
struct Analog {
    x: f32,
    y: f32,
    fresh: bool,
}

struct Live {
    def: Rc<MovieDef>,
    player: Player,
    axes: [Axis; 4],
    analog: [Analog; 2],
    /// The pointer the movie last saw, in stage pixels, and the left button.
    pointer: Option<([f32; 2], bool)>,
}

/// The window's pointer, as `FlashWidget::Update` (`0x0061B600`) hands every running movie each frame
/// through `NotifyMouseState`: where it is across the viewport, `0..1` each way, and the buttons, bit
/// 0 left. Only the left button acts unless a movie turns more on, which none of the shipped do.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pointer {
    pub at: [f32; 2],
    pub buttons: u32,
}

impl Live {
    fn notify_pointer(&mut self, pointer: Pointer) -> Result<(), String> {
        let stage = [(self.def.stage[1] - self.def.stage[0]) as f32 / 20.0, (self.def.stage[3] - self.def.stage[2]) as f32 / 20.0];
        let at = [pointer.at[0] * stage[0], pointer.at[1] * stage[1]];
        let left = pointer.buttons & 1 != 0;
        let (was_at, was_left) = self.pointer.unwrap_or(([f32::NAN; 2], false));
        if at != was_at {
            self.player.mouse_move(at[0], at[1])?;
        }
        match (was_left, left) {
            (false, true) => self.player.mouse_down()?,
            (true, false) => self.player.mouse_up()?,
            _ => {}
        }
        self.pointer = Some((at, left));
        Ok(())
    }

    fn button(&mut self, button: i64, action: FlashAction) -> Result<(), String> {
        if let Some((index, way)) = axis_of(button) {
            let axis = &mut self.axes[index];
            match action {
                FlashAction::Press => {
                    axis.value = way;
                    axis.held = 0.0;
                }
                FlashAction::Release if axis.value == way => {
                    axis.value = 0;
                    axis.held = 0.0;
                }
                _ => {}
            }
        }
        match action {
            FlashAction::Release => self.player.key_up(key_of(button)),
            _ => self.player.key_down(key_of(button), 0),
        }
    }

    /// Call `name` if the movie has it: every engine `Invoke` is guarded by `IsAvailable`.
    fn invoke(&mut self, name: &str, args: &[Value]) -> Result<(), String> {
        if self.player.available(name)? {
            self.player.invoke(name, args)?;
        }
        Ok(())
    }

    fn analog(&mut self) -> Result<(), String> {
        for (index, name) in ["leftAnalog", "rightAnalog"].into_iter().enumerate() {
            let stick = &mut self.analog[index];
            if stick.fresh {
                stick.fresh = false;
            } else if stick.x.abs() > ANALOG_EPSILON || stick.y.abs() > ANALOG_EPSILON {
                stick.x = 0.0;
                stick.y = 0.0;
            } else {
                continue;
            }
            let args = [Value::Number(stick.x as f64), Value::Number(stick.y as f64)];
            self.invoke(name, &args)?;
        }
        Ok(())
    }

    fn repeat(&mut self, dt: f32) -> Result<(), String> {
        for (index, keys) in AXIS_KEYS.into_iter().enumerate() {
            let axis = &mut self.axes[index];
            if axis.value == 0 {
                continue;
            }
            if axis.held <= REPEAT_DELAY {
                axis.held += dt;
                continue;
            }
            let key = keys[(axis.value > 0) as usize];
            self.player.key_up(key)?;
            self.player.key_down(key, 0)?;
        }
        Ok(())
    }

    /// `FUN_0061C440`, after a load and after a restart.
    fn setup(&mut self) -> Result<(), String> {
        // Retail makes these before the first advance as well, while `SHELL.gfx` defines neither yet.
        self.invoke("confirmButtonReversed", &[Value::Bool(false)])?;
        self.invoke("setTerritory", &[Value::Str("EN".into())])
    }
}

/// The script host the movies' FSCommand handlers live in.
pub type Host = Rc<RefCell<GameScriptHost>>;

fn value_of(arg: FlashArg) -> Value {
    match arg {
        FlashArg::Nil => Value::Undefined,
        FlashArg::Bool(b) => Value::Bool(b),
        FlashArg::Number(n) => Value::Number(n),
        FlashArg::Text(t) => Value::Str(t.into()),
    }
}

/// Whether a movie file is the shell's, the one `SHELL_MOVIE` requests reach.
fn is_shell_movie(file: &str) -> bool {
    file.to_ascii_lowercase().starts_with("shell")
}

/// The FSCommand handler of `widget`'s movie: as GFx calls the engine's handler inside the action
/// (`FUN_0060DE80` → the Lua function `SetFlashEventHandler` registered), the handler runs at once,
/// and the functions it has the engine call in this movie (`LTIAudioSFXVolume` for `audioEnter`, …)
/// run before the movie's next action. The audio page needs that order: its first frame places the
/// sliders from the values `audioEnter` sets in the frame before it.
fn fscommand_hook(host: &Host, script: &Rc<ScriptHost>, widget: u64, is_shell: bool) -> FsCommandHook {
    let (host, script) = (host.clone(), script.clone());
    Rc::new(move |command: &str, argument: &str| {
        let callback = host.borrow().widgets_ref().get(widget).and_then(|w| w.flash.as_ref()?.callbacks.get(command).copied());
        if let Some(callback) = callback {
            script.fire_flash_now(callback, Some(argument.to_string())).map_err(|e| format!("the {command} handler: {e:?}"))?;
        }
        let invokes = host.borrow_mut().widgets().take_flash_invokes(widget, is_shell);
        Ok(invokes.into_iter().map(|(name, args)| (name, args.into_iter().map(value_of).collect())).collect())
    })
}

/// One movie to draw this frame.
pub struct FlashDraw {
    pub def: Rc<MovieDef>,
    pub items: Vec<Item>,
}

/// Every live Flash widget, and what their movies read.
pub struct FlashStage {
    assets: WadImages,
    fonts: Rc<FontLib>,
    strings: Rc<HashMap<u32, String>>,
    defs: HashMap<String, Rc<MovieDef>>,
    movies: BTreeMap<u64, Live>,
    /// The FSCommands the engine answers itself, by `FUN_0060DE80` and `FUN_00614540`.
    commands: Vec<(String, String)>,
}

/// The commands [`FlashStage::take_commands`] hands over.
const ENGINE_COMMANDS: [&str; 8] = [
    "onlineLTIIsConnected",
    "preoperation",
    "onlineSaveProfile",
    "LoadSaveGame",
    "DeleteSaveGame",
    "Sound.Play",
    "Sound.Loop",
    "Sound.Stop",
];

impl FlashStage {
    /// `wads` in lookup order, the last mounted first. Every language but Russian reads `fonts_enext`
    /// (`FUN_0060E9C0`).
    pub fn open(wads: &[String], language: &str) -> Result<FlashStage, String> {
        let mut assets = WadImages::open(wads)?;
        let strings = Rc::new(assets.strings(language)?);
        let font = if language == "russian" { "fonts_ru" } else { "fonts_enext" };
        let fonts = Rc::new(FontLib::from_movie(&read_movie(&mut assets, font)?)?);
        Ok(FlashStage { assets, fonts, strings, defs: HashMap::new(), movies: BTreeMap::new(), commands: Vec::new() })
    }

    /// The widget showing the shell's movie.
    fn shell_widget(&self, tree: &WidgetTree) -> Option<u64> {
        self.movies.keys().copied().find(|id| tree.get(*id).and_then(|w| w.flash.as_ref()).is_some_and(|f| is_shell_movie(&f.swf)))
    }

    /// Whether the shell's movie is up.
    pub fn has_shell(&self, tree: &WidgetTree) -> bool {
        self.shell_widget(tree).is_some()
    }

    /// Call a function of the shell's movie, if it has one, as the engine's `Invoke` does.
    pub fn call_shell(&mut self, host: &Host, name: &str, args: &[Value]) -> Result<(), String> {
        let id = self.shell_widget(host.borrow().widgets_ref()).ok_or_else(|| format!("{name}: the shell's movie is not up"))?;
        self.movies.get_mut(&id).ok_or("the shell's movie vanished")?.invoke(name, args)
    }

    /// The FSCommands this frame that are the engine's to answer.
    pub fn take_commands(&mut self) -> Vec<(String, String)> {
        std::mem::take(&mut self.commands)
    }

    pub fn images(&mut self) -> &mut WadImages {
        &mut self.assets
    }

    /// The string table the movies' `[0x…]` keys read from.
    pub fn strings(&self) -> Rc<HashMap<u32, String>> {
        self.strings.clone()
    }

    fn open_movie(&mut self, name: &str) -> Result<Live, String> {
        let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem).to_ascii_lowercase();
        let def = match self.defs.get(&stem) {
            Some(def) => def.clone(),
            None => {
                let def = Rc::new(read_movie(&mut self.assets, &stem)?);
                self.defs.insert(stem, def.clone());
                def
            }
        };
        let mut movie = Movie::new(def.clone())?;
        movie.font_lib = Some(self.fonts.clone());
        movie.strings = Some(self.strings.clone());
        Ok(Live { def, player: Player::new(movie)?, axes: Default::default(), analog: Default::default(), pointer: None })
    }

    /// A movie opened for `widget`, with the scripts' FSCommand handlers wired into its player.
    fn open_live(&mut self, host: &Host, script: &Rc<ScriptHost>, widget: u64, file: &str) -> Result<Live, String> {
        let mut live = self.open_movie(file)?;
        live.player.fscommand_hook = Some(fscommand_hook(host, script, widget, is_shell_movie(file)));
        live.setup()?;
        Ok(live)
    }

    /// Serve one script request. The host is only borrowed between movie calls: a movie's FSCommand
    /// runs its Lua handler from inside the movie, and that handler reaches the host itself.
    fn serve(&mut self, host: &Host, script: &Rc<ScriptHost>, widget: u64, request: FlashRequest) -> Result<(), String> {
        let widget = match widget {
            mercs2_ui::SHELL_MOVIE => self.shell_widget(host.borrow().widgets_ref()).ok_or_else(|| format!("no shell movie for {request:?}"))?,
            other => other,
        };
        match request {
            FlashRequest::Open { file: None, .. } => {
                self.movies.remove(&widget);
            }
            FlashRequest::Open { file: Some(file), done } => {
                let live = self.open_live(host, script, widget, &file)?;
                self.movies.insert(widget, live);
                if let Some(done) = done {
                    host.borrow_mut().widgets().fire_flash(done, None);
                }
            }
            FlashRequest::Restart => {
                let name = host.borrow().widgets_ref().get(widget).and_then(|w| w.flash.as_ref()).map(|f| f.swf.clone()).unwrap_or_default();
                if self.movies.contains_key(&widget) {
                    let live = self.open_live(host, script, widget, &name)?;
                    self.movies.insert(widget, live);
                }
            }
            request => {
                let live = self.movies.get_mut(&widget).ok_or_else(|| format!("a script drove flash widget {widget}, which holds no movie: {request:?}"))?;
                match request {
                    FlashRequest::Invoke { name, args } => {
                        let args: Vec<Value> = args.into_iter().map(value_of).collect();
                        live.invoke(&name, &args)?;
                    }
                    FlashRequest::Input { button, action } => live.button(button, action)?,
                    FlashRequest::Analog { right, x, y } => live.analog[right as usize] = Analog { x, y, fresh: true },
                    FlashRequest::Open { .. } | FlashRequest::Restart => unreachable!("matched above"),
                }
            }
        }
        Ok(())
    }

    /// Serve the scripts' requests and step the movies. A movie's FSCommand reaches the handler a
    /// script registered for it while the movie runs (see [`fscommand_hook`]); the ones the engine
    /// answers itself are kept for [`Self::take_commands`].
    pub fn advance(&mut self, host: &Host, script: &Rc<ScriptHost>, dt: f32, pointer: Pointer) -> Result<(), String> {
        let requests = host.borrow_mut().widgets().take_flash_requests();
        for (widget, request) in requests {
            self.serve(host, script, widget, request)?;
        }
        let ids: Vec<u64> = self.movies.keys().copied().collect();
        for id in ids {
            let state = host.borrow().widgets_ref().get(id).and_then(|w| Some((w.visible, w.flash.as_ref()?.playing, w.flash.as_ref()?.play_speed)));
            let Some((visible, playing, speed)) = state else {
                self.movies.remove(&id);
                continue;
            };
            let live = self.movies.get_mut(&id).ok_or("a movie vanished mid-frame")?;
            live.notify_pointer(pointer)?;
            live.analog()?;
            live.repeat(dt)?;
            if playing && visible {
                live.player.advance((dt * speed) as f64 * 1000.0)?;
            }
            for line in std::mem::take(&mut live.player.traces) {
                println!("[flash] {line}");
            }
            for (command, argument) in std::mem::take(&mut live.player.fscommands) {
                println!("[flash] fscommand {command}({argument})");
                if ENGINE_COMMANDS.contains(&command.as_str()) {
                    self.commands.push((command, argument));
                }
            }
        }
        Ok(())
    }

    /// Call `name` in every live movie; says which had it.
    pub fn call_all(&mut self, name: &str, args: &[f64]) -> Result<String, String> {
        let args: Vec<Value> = args.iter().map(|n| Value::Number(*n)).collect();
        let mut out = String::new();
        for (id, live) in &mut self.movies {
            let found = live.player.available(name)?;
            out.push_str(&format!("{id}:{found} "));
            live.invoke(name, &args)?;
        }
        Ok(out)
    }

    /// Read `path` in every live movie, for a probe.
    pub fn get_all(&mut self, path: &str) -> Result<String, String> {
        let mut out = String::new();
        for (id, live) in &mut self.movies {
            out.push_str(&format!("{id}:{:?} ", live.player.get_variable(path)?));
        }
        Ok(out)
    }

    /// A line per live movie, for a probe.
    pub fn describe(&self, tree: &WidgetTree) -> Vec<String> {
        self.movies
            .iter()
            .map(|(id, live)| {
                let frame = live.player.movie.clip(live.player.movie.root).map(|c| c.frame);
                format!("widget {id}: frame {frame:?} shown {} playing {:?}", tree.shown(*id), tree.get(*id).and_then(|w| w.flash.as_ref()).map(|f| f.playing))
            })
            .collect()
    }

    /// The movies on screen, each with the z its widget draws at.
    pub fn draws(&mut self, tree: &WidgetTree) -> Result<Vec<(i32, FlashDraw)>, String> {
        let mut out = Vec::new();
        for (id, live) in self.movies.iter_mut() {
            let Some(w) = tree.get(*id) else { continue };
            if tree.shown(*id) {
                out.push((w.z, FlashDraw { def: live.def.clone(), items: live.player.movie.draw()? }));
            }
        }
        Ok(out)
    }
}

fn read_movie(assets: &mut WadImages, stem: &str) -> Result<MovieDef, String> {
    let bytes = assets.movie(stem)?;
    MovieDef::decode(&GfxMovie::parse(&bytes).map_err(|e| format!("{stem}: {e}"))?).map_err(|e| format!("{stem}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_button_table_is_the_jump_table() {
        assert_eq!(key_of(2), 0x28, "pad down is the down arrow");
        assert_eq!(key_of(6), 0x0D, "the confirm button is return");
        assert_eq!(key_of(18), 0, "a button with no key sends key 0");
        assert_eq!(axis_of(5), None, "only the pad and the stick hold an axis");
        assert_eq!(AXIS_KEYS[axis_of(2).unwrap().0][1], key_of(2));
        assert_eq!(AXIS_KEYS[axis_of(9).unwrap().0][0], key_of(9));
    }
}
