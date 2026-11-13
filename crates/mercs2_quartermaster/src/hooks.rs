//! The lifecycle hooks Quartermaster owns: `data/hooks.tsv`, one row per hook. Each row gives the
//! retail module the generated code is spliced onto (`host`), the function it wraps or the event or
//! game state it subscribes to (`target`), and whether it dispatches before or after the original
//! (`call`), and when a wrap dispatches at all (`filter`). Every row cites the retail Lua it rests on
//! (`evidence`). An `add_script`'s `load: { when }` names one of these ids, and every hook dispatches
//! through the retail-Lua `_G._QM` (never through the m2-sdk, never through `import`).

use std::sync::OnceLock;

use mercs2_formats::shader_registrations::tsv_rows;

use crate::manifest::LoadSession;

const HOOKS_TSV: &str = include_str!("../data/hooks.tsv");

const HEADER: [&str; 9] = ["id", "session", "kind", "host", "target", "call", "filter", "fires", "evidence"];

/// The host name the catalogue gives the master script. A custom level runs its own master instead,
/// and qm puts the master's hooks onto that one.
pub const MASTER_HOST: &str = "vz";

/// When a wrap dispatches: a Lua expression over `...`, evaluated in the host's env.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Filter {
    /// Over the call's arguments, before the original runs.
    Args(String),
    /// Over the original's return values, after it returns. An `after` wrap only.
    Results(String),
}

/// How a hook fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The run of qm's own code spliced onto the end of `host`: it fires when the engine runs that
    /// chunk. The loaders' entry points.
    Splice,
    /// A wrapper replaces the global function `target` of `host`.
    Wrap,
    /// A subscription to the retail script event posted as `target`.
    Event,
    /// A subscription to the game-state broadcast for state `target`.
    State,
}

/// When a wrapper dispatches, relative to the original it calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Call {
    Before,
    After,
}

/// One row of `data/hooks.tsv`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hook {
    pub id: String,
    /// The sessions it fires in: one, or both for `both`.
    pub sessions: Vec<LoadSession>,
    pub kind: Kind,
    /// The INFO name of the retail module the generated code is spliced onto.
    pub host: String,
    /// The wrapped function, event or state; `None` for a splice.
    pub target: Option<String>,
    /// `Some` for a wrap only.
    pub call: Option<Call>,
    /// `None` when it always dispatches; a wrap only.
    pub filter: Option<Filter>,
    pub fires: String,
    pub evidence: String,
}

/// Every hook, in `data/hooks.tsv` row order. A malformed row is a build defect of qm itself and
/// panics naming it.
pub fn hooks() -> &'static [Hook] {
    static H: OnceLock<Vec<Hook>> = OnceLock::new();
    H.get_or_init(|| parse(HOOKS_TSV).unwrap_or_else(|e| panic!("data/hooks.tsv: {e}")))
}

/// The hook named `id`.
pub fn hook(id: &str) -> Option<&'static Hook> {
    hooks().iter().find(|h| h.id == id)
}

fn parse(text: &str) -> Result<Vec<Hook>, String> {
    let body: String = text.lines().filter(|l| !l.starts_with('#')).map(|l| format!("{l}\n")).collect();
    let mut out: Vec<Hook> = Vec::new();
    for f in tsv_rows(&body, &HEADER) {
        let row = f.join("\t");
        if let Some(empty) = HEADER.iter().zip(&f).find(|(_, v)| v.is_empty()) {
            return Err(format!("row {row:?}: field {} is empty (write `-` for not applicable)", empty.0));
        }
        let sessions = match f[1] {
            "gameplay" => vec![LoadSession::Gameplay],
            "front_end" => vec![LoadSession::FrontEnd],
            "both" => LoadSession::ALL.to_vec(),
            s => return Err(format!("row {row:?}: session {s:?} is not gameplay, front_end or both")),
        };
        let kind = match f[2] {
            "splice" => Kind::Splice,
            "wrap" => Kind::Wrap,
            "event" => Kind::Event,
            "state" => Kind::State,
            k => return Err(format!("row {row:?}: kind {k:?} is not splice, wrap, event or state")),
        };
        let target = (f[4] != "-").then(|| f[4].to_string());
        let call = match f[5] {
            "before" => Some(Call::Before),
            "after" => Some(Call::After),
            "-" => None,
            c => return Err(format!("row {row:?}: call {c:?} is not before, after or -")),
        };
        match (kind, &target, call) {
            (Kind::Splice, None, None) | (Kind::Wrap, Some(_), Some(_)) | (Kind::Event | Kind::State, Some(_), None) => {}
            _ => {
                return Err(format!(
                    "row {row:?}: a {} hook takes {}",
                    f[2],
                    match kind {
                        Kind::Splice => "no target and no call",
                        Kind::Wrap => "a target and a call",
                        Kind::Event | Kind::State => "a target and no call",
                    }
                ));
            }
        }
        let filter = match f[6] {
            "-" => None,
            v => match (v.split_once(": "), kind, call) {
                (Some(("args", e)), Kind::Wrap, _) => Some(Filter::Args(e.to_string())),
                (Some(("results", e)), Kind::Wrap, Some(Call::After)) => Some(Filter::Results(e.to_string())),
                (Some(("results", _)), Kind::Wrap, _) => return Err(format!("row {row:?}: a results filter needs an after wrap")),
                (_, Kind::Wrap, _) => return Err(format!("row {row:?}: filter {v:?} is not \"args: <expr>\" or \"results: <expr>\"")),
                _ => return Err(format!("row {row:?}: only a wrap takes a filter")),
            },
        };
        if out.iter().any(|h| h.id == f[0]) {
            return Err(format!("hook {:?} appears twice", f[0]));
        }
        // Rows of one (host, target) are one wrapper, so they dispatch on one side of the original.
        if let Some(other) = out.iter().find(|h| h.kind == Kind::Wrap && h.host == f[3] && h.target == target && h.call != call) {
            return Err(format!("hooks {:?} and {:?} wrap {}.{} on different sides of the original", other.id, f[0], f[3], f[4]));
        }
        out.push(Hook {
            id: f[0].to_string(),
            sessions,
            kind,
            host: f[3].to_string(),
            target,
            call,
            filter,
            fires: f[7].to_string(),
            evidence: f[8].to_string(),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalogue_parses_and_each_session_has_its_loader_entry() {
        let all = hooks();
        assert!(all.len() >= 80, "the catalogue has {} hooks", all.len());
        let world = hook("world.scripts_loaded").expect("the world loader's entry");
        assert_eq!((world.kind, world.host.as_str(), world.sessions.as_slice()), (Kind::Splice, "vz", &[LoadSession::Gameplay][..]));
        let shell = hook("shell.boot").expect("the shell loader's entry");
        assert_eq!((shell.kind, shell.host.as_str(), shell.sessions.as_slice()), (Kind::Splice, "shellbootstrap", &[LoadSession::FrontEnd][..]));
        let hq = hook("hq.enter").expect("hq.enter");
        assert_eq!((hq.kind, hq.host.as_str(), hq.target.as_deref(), hq.call, &hq.filter), (Kind::Wrap, "mrxhq", Some("GlobalEnter"), Some(Call::After), &None));
        let begin = hook("world.begin").expect("world.begin");
        assert_eq!(begin.filter, Some(Filter::Args("not Pg.LoadIsRetry()".into())));
        let shop = hook("shop.purchase").expect("shop.purchase");
        assert_eq!(shop.filter, Some(Filter::Results("(...) == true".into())));
    }

    #[test]
    fn a_malformed_row_is_refused_naming_it() {
        let head = HEADER.join("\t");
        let bad_kind = format!("{head}\nx\tgameplay\thook\tvz\t-\t-\t-\tf\te\n");
        assert!(parse(&bad_kind).unwrap_err().contains("kind \"hook\""));
        let wrap_without_call = format!("{head}\nx\tgameplay\twrap\tvz\tF\t-\t-\tf\te\n");
        assert!(parse(&wrap_without_call).unwrap_err().contains("a target and a call"));
        let twice = format!("{head}\nx\tgameplay\tsplice\tvz\t-\t-\t-\tf\te\nx\tgameplay\tsplice\tvz\t-\t-\t-\tf\te\n");
        assert!(parse(&twice).unwrap_err().contains("appears twice"));
        let empty = format!("{head}\nx\tgameplay\tsplice\t\t-\t-\t-\tf\te\n");
        assert!(parse(&empty).unwrap_err().contains("field host is empty"));
        let results_before = format!("{head}\nx\tgameplay\twrap\tvz\tF\tbefore\tresults: (...)\tf\te\n");
        assert!(parse(&results_before).unwrap_err().contains("needs an after wrap"));
        let event_filter = format!("{head}\nx\tgameplay\tevent\tvz\tE\t-\targs: true\tf\te\n");
        assert!(parse(&event_filter).unwrap_err().contains("only a wrap takes a filter"));
        let bad_filter = format!("{head}\nx\tgameplay\twrap\tvz\tF\tafter\twhen: true\tf\te\n");
        assert!(parse(&bad_filter).unwrap_err().contains("is not \"args: <expr>\""));
        let split = format!("{head}\nx\tgameplay\twrap\tvz\tF\tafter\t-\tf\te\ny\tgameplay\twrap\tvz\tF\tbefore\t-\tf\te\n");
        assert!(parse(&split).unwrap_err().contains("on different sides"));
    }
}
