//! The front end: the scripts `shell.wad` ships, over the Flash widgets.
//!
//! Retail boots the `shell` level into `ShellBootstrap`, which plays the intro movies and enters the
//! shell state; `MrxGuiShell` puts `shell.gfx` on a Flash widget and from there on the movie and the
//! scripts talk to each other. This is the engine's half of that conversation: the game states, the
//! controller events the GUI reads, and the frame order. Starting a game ends the session: retail
//! tears the shell's Lua VM down and boots the level on a new one.

use std::cell::RefCell;
use std::rc::Rc;

use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_gfx::avm::Value;
use mercs2_script::{ScriptHost, SharedHost};

use crate::flash::{FlashDraw, FlashStage};
use crate::gfx_pass::WadImages;
use crate::script_host::GameScriptHost;

/// How a controller event reaches the GUI: `FUN_004BD670` builds this table and adds it to the list
/// `MrxGuiBase.Run` works through, which hands it to the widget holding the control focus.
const GLUE: &str = r#"
function __shell_input(sKey, nButton)
  local tList = MrxGuiBase.GetEventListTable()
  tList[#tList + 1] = {EventType = "ControllerInput", [sKey] = nButton}
end
function __shell_character()
  return MrxGuiShellBootstrap.GetSelectedCharacter()
end
"#;

/// What the session asks of the game.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShellExit {
    /// `Sys.StartSingleplayer`, and the hero picked for it.
    NewGame { character: Option<String> },
    /// The `LoadSaveGame` FSCommand.
    Load(std::path::PathBuf),
    /// `Sys.RequestGameState("Exiting")`.
    Quit,
}

/// A row of the load-game list, in the order of `addSaveGame`'s arguments (`FUN_00617390`).
#[derive(Clone, Debug)]
pub struct ShellSave {
    pub path: std::path::PathBuf,
    /// `hh:mm:ss | mm/dd/yyyy`.
    pub date: String,
    pub character: u8,
    pub cash: u32,
    pub fuel: u32,
    /// `hh:mm:ss`.
    pub play_time: String,
    pub contract: String,
}

/// How far the movie's startup has gone. The order is retail's: `FUN_004B8D60` starts the movie on
/// the shell state's first frame and ignores the controller for its first four seconds; pressing
/// start runs the profile manager (`FUN_006154B0`, then `FUN_00635CF0`), whose last state, after two
/// seconds, lists the saves and calls `startShell` (`FUN_00616CC0`).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Startup {
    WaitingForMovie,
    FirstLoad(f32),
    PressStart,
    SigningIn(f32),
    Running,
}

/// `[0x00B97744]`.
const FIRST_LOAD_SECONDS: f32 = 4.0;
/// `pm+0xA888 >= 2.0` at `0x00635F4F`.
const SIGNIN_SECONDS: f32 = 2.0;

/// The WADs the shell level mounts beside `vz_wad`, the last mounted first: retail mounts `Loading`,
/// `loading-patch`, `shell`, `shell-patch`, `extra`, `English`, `English-patch`, and the stack is
/// last-mounted-wins. A patch is optional; the three the shell cannot run without are not.
pub fn shell_wads(vz_wad: &str) -> Result<Vec<String>, String> {
    let dir = std::path::Path::new(vz_wad).parent().ok_or_else(|| format!("{vz_wad} has no folder"))?;
    let mut out = Vec::new();
    for (name, required) in [
        ("English-patch.wad", false),
        ("English.wad", true),
        ("extra.wad", false),
        ("shell-patch.wad", false),
        ("shell.wad", true),
        ("loading-patch.wad", false),
        ("Loading.wad", true),
    ] {
        let path = dir.join(name);
        if path.is_file() {
            out.push(path.to_string_lossy().into_owned());
        } else if required {
            return Err(format!("the shell needs {}", path.display()));
        }
    }
    Ok(out)
}

pub struct Shell {
    script: ScriptHost,
    host: Rc<RefCell<GameScriptHost>>,
    flash: FlashStage,
    state: Option<String>,
    exit: Option<ShellExit>,
    startup: Startup,
    saves: Vec<ShellSave>,
    /// What the running `preoperation` is for: `saveCompleteContinue` hands it back.
    operation: String,
}

impl Shell {
    /// Boot the shell from `wads`, the last mounted first, with `saves` newest first.
    pub fn open(wads: &[String], saves: Vec<ShellSave>) -> Result<Shell, String> {
        let mut flash = FlashStage::open(wads, "english")?;
        let scripts = flash.images().take_scripts();
        let script = ScriptHost::with_module_source(move |name| {
            let hash = pandemic_hash_m2(&name.to_ascii_lowercase());
            scripts.get(&hash).cloned().ok_or_else(|| format!("no script {name} (0x{hash:08X}) in the shell's WADs"))
        })
        .map_err(|e| format!("shell script host: {e}"))?;
        let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
        let shared: SharedHost = host.clone();
        script.register_engine(shared).map_err(|e| format!("shell bindings: {e:?}"))?;
        script.exec(GLUE, "@shell_glue").map_err(|e| format!("shell glue: {e:?}"))?;
        script.exec("import(\"ShellBootstrap\")", "@shell_boot").map_err(|e| format!("ShellBootstrap: {e:?}"))?;
        Ok(Shell { script, host, flash, state: None, exit: None, startup: Startup::WaitingForMovie, saves, operation: String::new() })
    }

    fn call(&mut self, name: &str, args: &[Value]) -> Result<(), String> {
        self.flash.call_shell(self.host.borrow_mut().widgets(), name, args)
    }

    fn step_startup(&mut self, dt: f32) -> Result<(), String> {
        match self.startup {
            Startup::WaitingForMovie => {
                let up = self.state.as_deref() == Some("Shell") && self.flash.has_shell(self.host.borrow_mut().widgets());
                if up {
                    // `FUN_0061C060`. The region comes from the install's registry key, which a copied
                    // data folder does not have; the image's own default is US.
                    self.call("setPlatform", &[Value::Str("PC".into())])?;
                    self.call("setRegion", &[Value::Str("US".into())])?;
                    self.call("setTerritory", &[Value::Str("EN".into())])?;
                    self.call("firstTimeLoad", &[Value::Bool(true)])?;
                    self.startup = Startup::FirstLoad(0.0);
                }
            }
            Startup::FirstLoad(held) if held + dt >= FIRST_LOAD_SECONDS => {
                self.call("firstTimeLoad", &[Value::Bool(false)])?;
                self.startup = Startup::PressStart;
            }
            Startup::FirstLoad(held) => self.startup = Startup::FirstLoad(held + dt),
            Startup::PressStart => {
                if self.host.borrow_mut().widgets().take_signin() {
                    self.call("disableSave", &[Value::Bool(false)])?;
                    self.call("EnableUsingFakeProfile", &[Value::Bool(false)])?;
                    self.startup = Startup::SigningIn(0.0);
                }
            }
            Startup::SigningIn(held) if held + dt >= SIGNIN_SECONDS => {
                self.call("clearSaveGames", &[Value::Bool(true)])?;
                self.call("ProfilesComplete", &[Value::Number(1.0)])?;
                self.call("disableManageSaves", &[Value::Bool(false)])?;
                self.call("disableOnline", &[Value::Bool(false)])?;
                for (index, save) in self.saves.clone().into_iter().enumerate() {
                    self.call(
                        "addSaveGame",
                        &[
                            Value::Number(index as f64 + 1.0),
                            Value::Str(save.date.into()),
                            Value::Number(save.character as f64),
                            Value::Number(save.cash as f64),
                            Value::Number(save.fuel as f64),
                            Value::Str(save.play_time.into()),
                            Value::Str(save.contract.into()),
                            Value::Bool(false),
                            Value::Number(0.0),
                            Value::Bool(false),
                        ],
                    )?;
                }
                self.call("startShell", &[Value::Bool(true)])?;
                self.startup = Startup::Running;
            }
            Startup::SigningIn(held) => self.startup = Startup::SigningIn(held + dt),
            Startup::Running => {}
        }
        Ok(())
    }

    /// Answer the commands that are the engine's own.
    fn answer(&mut self, command: &str, argument: &str) -> Result<(), String> {
        match command {
            // `FUN_00614540`. There is no service to reach, so the answer is the movie's own offline
            // branch: `LTIChoseOnline(2)`, then on to the game.
            "onlineLTIIsConnected" => self.call("LTIProfileOnlinePlay", &[Value::Number(2.0)]),
            // `FUN_00616A90`: `"<operation>,<name>"`, and `preopComplete(true, name)` lets it go on. Before
            // the player has pressed start it does nothing: the movie sends one on its way there.
            "preoperation" if matches!(self.startup, Startup::WaitingForMovie | Startup::FirstLoad(_) | Startup::PressStart) => Ok(()),
            "preoperation" => {
                let name = argument.split_once(',').map_or(argument, |(_, name)| name).to_string();
                self.operation = name.clone();
                self.call("preopComplete", &[Value::Bool(true), Value::Str(name.into())])
            }
            // The profile holds nothing here that a later session reads, so it is saved at once.
            "onlineSaveProfile" => {
                let name = self.operation.clone();
                self.call("saveCompleteContinue", &[Value::Str(name.into())])
            }
            // `0x0060DE80`: a positive number is a row of the list; anything else is the newest.
            "LoadSaveGame" => {
                let row: i64 = argument.trim().parse().map_err(|e| format!("LoadSaveGame({argument}): {e}"))?;
                let index = if row > 0 { row as usize - 1 } else { 0 };
                let save = self.saves.get(index).ok_or_else(|| format!("LoadSaveGame({argument}): {} saves are listed", self.saves.len()))?;
                self.exit = Some(ShellExit::Load(save.path.clone()));
                Ok(())
            }
            "DeleteSaveGame" => Err(format!("DeleteSaveGame({argument}): deleting a save from the menu is not implemented")),
            other => Err(format!("{other} has no engine answer")),
        }
    }

    /// One frame: the game states the scripts asked for, their timers and callbacks, the GUI's events,
    /// then the movies.
    pub fn tick(&mut self, dt: f32) -> Result<Option<ShellExit>, String> {
        let shared: SharedHost = self.host.clone();
        let states = self.host.borrow_mut().take_pending_game_states();
        for state in states {
            if state == "Exiting" {
                self.exit = Some(ShellExit::Quit);
            }
            if let Some(previous) = self.state.replace(state.clone()) {
                self.change(&previous, "Exit")?;
            }
            self.change(&state, "Enter")?;
        }
        self.script.pump_events(dt).map_err(|e| format!("event pump: {e:?}"))?;
        self.script.pump_callbacks(&shared, dt).map_err(|e| format!("callback pump: {e:?}"))?;
        self.script.pump_widget_updates(&shared, dt).map_err(|e| format!("widget update: {e:?}"))?;
        self.script.exec("MrxGuiBase.Run()", "@shell_run").map_err(|e| format!("MrxGuiBase.Run: {e:?}"))?;
        self.flash.advance(self.host.borrow_mut().widgets(), dt)?;
        self.step_startup(dt)?;
        for (command, argument) in self.flash.take_commands() {
            self.answer(&command, &argument)?;
        }
        if self.exit.is_none() && self.host.borrow().singleplayer_started() {
            let character = self.script.eval::<Option<String>>("return __shell_character()").map_err(|e| format!("the picked hero: {e:?}"))?;
            self.exit = Some(ShellExit::NewGame { character });
        }
        Ok(self.exit.clone())
    }

    fn change(&self, state: &str, action: &str) -> Result<(), String> {
        self.script.fire_state_change(state, action).map_err(|e| format!("GameStateChange({state}, {action}): {e:?}"))?;
        self.script.fire_gui_state_change(state, action).map_err(|e| format!("GuiGameStateChange({state}, {action}): {e:?}"))
    }

    /// A controller button went down or up.
    pub fn button(&mut self, button: i64, down: bool) -> Result<(), String> {
        if matches!(self.startup, Startup::WaitingForMovie | Startup::FirstLoad(_)) {
            return Ok(());
        }
        let key = if down { "ButtonPress" } else { "ButtonReleased" };
        self.script.exec(&format!("__shell_input(\"{key}\", {button})"), "@shell_input").map_err(|e| format!("controller input: {e:?}"))
    }

    /// Call a function of every live movie, as the engine's `Invoke` does, for a probe.
    pub fn call_movies(&mut self, name: &str, args: &[f64]) -> Result<String, String> {
        self.flash.call_all(name, args)
    }

    /// The game state and the live movies, for a probe.
    pub fn describe(&mut self) -> Vec<String> {
        let mut out = vec![format!("state {:?}", self.state)];
        out.extend(self.flash.describe(self.host.borrow_mut().widgets()));
        out
    }

    /// The movies to draw this frame, back to front, and the images they read.
    pub fn draws(&mut self) -> Result<(Vec<FlashDraw>, &mut WadImages), String> {
        let draws = self.flash.draws(self.host.borrow_mut().widgets())?;
        Ok((draws, self.flash.images()))
    }

}
