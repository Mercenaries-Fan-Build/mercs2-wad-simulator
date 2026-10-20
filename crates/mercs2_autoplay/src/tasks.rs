//! The live `MrxTask` tree, read out of the running Lua.
//!
//! Every mission the flow unlocks is a node under `MrxMissionFlow._oParent` (set by
//! `MrxMissionFlow.Init`, `mrxmissionflow.lua:108`): a container per mission, the mission task under
//! it, and briefing / objective children below that. Each node carries a state from `mrxtaskstate.lua`
//! (`_knLatent` 0, `_knActive` 1, `_knCompleted` 2, `_knCancelled` 3).
//!
//! The walk only calls the pure accessors `GetLineage`, `_GetState` and `GetChildren`
//! (`mrxtask.lua:360-386`, `:212`); it does not print, mutate, or fire anything.

use mercs2_engine::script::mercs2_luac::rt::Table;

use crate::boot::Session;

/// `mrxtaskstate.lua:1-4`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TaskState {
    Latent,
    Active,
    Completed,
    Cancelled,
}

impl TaskState {
    fn from_lua(n: i64) -> Result<TaskState, String> {
        match n {
            0 => Ok(TaskState::Latent),
            1 => Ok(TaskState::Active),
            2 => Ok(TaskState::Completed),
            3 => Ok(TaskState::Cancelled),
            other => Err(format!("task state {other} is not one of mrxtaskstate.lua's four")),
        }
    }
}

/// One node of the task tree: its dotted lineage (`Missions.VzaCon001.VzaCon001Mission.VZA001: …`, the
/// same string `_SetState` prints) and its state.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TaskNode {
    pub lineage: String,
    pub state: TaskState,
}

const WALK_LUA: &str = r#"
local root = MrxMissionFlow._oParent
if root == nil then
  error("MrxMissionFlow._oParent is nil: the mission flow has not been initialized")
end
local out = {}
local function walk(t)
  out[#out + 1] = { t:GetLineage(), t:_GetState() }
  local children = t:GetChildren()
  if children then
    for _, child in pairs(children) do walk(child) end
  end
end
walk(root)
return out
"#;

/// Snapshot the whole tree, sorted by lineage (`GetChildren` is a hash table, so the walk order is not
/// meaningful and is not kept). The walk is the harness reading state, not the game calling anything,
/// so the call recorder is suspended for it.
pub fn snapshot(session: &Session) -> Result<Vec<TaskNode>, String> {
    session.trace.suspended(|| walk(session))
}

fn walk(session: &Session) -> Result<Vec<TaskNode>, String> {
    let sh = &session.sh;
    // Re-binds the `MrxMissionFlow` global to the module the loader already holds, so the walk below
    // cannot read an auto-stub stand-in (which the stub layer hands out for any unloaded Capitalized name).
    sh.import_module("MrxMissionFlow").map_err(|e| format!("MrxMissionFlow: {e}"))?;
    let rows: Table = sh.eval(WALK_LUA).map_err(|e| format!("task-tree walk: {e}"))?;
    let mut nodes = Vec::new();
    for row in rows.sequence_values::<Table>() {
        let row = row.map_err(|e| format!("task-tree row: {e}"))?;
        let lineage: String = row.get(1).map_err(|e| format!("task lineage: {e}"))?;
        let state: i64 = row.get(2).map_err(|e| format!("task state of {lineage}: {e}"))?;
        nodes.push(TaskNode { state: TaskState::from_lua(state)?, lineage });
    }
    nodes.sort();
    Ok(nodes)
}
