//! The `states:` schema for `edit_state_machine` — the destruction state machine as editable YAML.
//!
//! Hand-writing a state machine from scratch would be punishing: it is nodes of named states, each
//! with Enter/Exit command scripts that are raw `u32` token streams. So the workflow is **extract,
//! then edit**: [`extract`] dumps the machine a model already carries into this format (names where a
//! hash reverses, opcodes as plain integers), the author changes what they want, and [`parse`] reads
//! it back. `serialize_state_machine` then applies it — which, being same-shape only, means the doc
//! is a faithful edit of the extracted baseline rather than a free-form authoring surface.
//!
//! A token is written as a plain integer (the `1`/`2`/`3` script opcodes, or any small value) or as
//! a string — a registry name we hash, or a bare `0xHHHHHHHH`. Extraction prefers the name so the
//! script reads as `[1, wreck_body, 2, SHOW, 3]`; round-trips exactly either way.

use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::orchestrator::{StateDef, StateMachine, SwitchNodeDef};
use serde::{Deserialize, Serialize};

/// One command-script / name token: an integer (opcode or raw value), or a string we resolve.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Token {
    Num(u32),
    Name(String),
}

impl Token {
    /// Resolve to the `u32` the format stores. A string is a bare `0xHHHHHHHH` if it parses as one,
    /// otherwise a name to hash — the same rule the manifest uses for any asset reference.
    fn resolve(&self) -> u32 {
        match self {
            Token::Num(n) => *n,
            Token::Name(s) => crate::manifest::bare_hash(s).unwrap_or_else(|| pandemic_hash_m2(s)),
        }
    }

    /// The most readable faithful spelling of `v`: its name if one reverses, a small opcode as a
    /// plain integer, otherwise a bare hash. Every branch round-trips through [`Self::resolve`].
    fn of(v: u32, name_of: &impl Fn(u32) -> Option<String>) -> Token {
        if let Some(n) = name_of(v) {
            // Guard the round-trip: only use the name if it actually re-hashes to v.
            if pandemic_hash_m2(&n) == v {
                return Token::Name(n);
            }
        }
        if v < 256 {
            Token::Num(v)
        } else {
            Token::Name(format!("0x{v:08X}"))
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StateDoc {
    name: Token,
    #[serde(default)]
    enter: Vec<Token>,
    #[serde(default)]
    exit: Vec<Token>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NodeDoc {
    name: Token,
    states: Vec<StateDoc>,
}

/// The whole machine, as the `states:` file carries it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatesDoc {
    nodes: Vec<NodeDoc>,
    #[serde(default)]
    switch_slots: Vec<Token>,
}

/// Dump a parsed machine to the editable YAML baseline. `name_of` reverses a hash to a name where it
/// can, so the extracted file reads in vocabulary rather than hex.
pub fn extract(sm: &StateMachine, name_of: impl Fn(u32) -> Option<String>) -> String {
    let doc = StatesDoc {
        nodes: sm
            .nodes
            .iter()
            .map(|n| NodeDoc {
                name: Token::of(n.name_hash, &name_of),
                states: n
                    .states
                    .iter()
                    .map(|s| StateDoc {
                        name: Token::of(s.name_hash, &name_of),
                        enter: s.enter.iter().map(|&t| Token::of(t, &name_of)).collect(),
                        exit: s.exit.iter().map(|&t| Token::of(t, &name_of)).collect(),
                    })
                    .collect(),
            })
            .collect(),
        switch_slots: sm.switch_slots.iter().map(|&t| Token::of(t, &name_of)).collect(),
    };
    serde_norway::to_string(&doc).unwrap_or_else(|e| format!("# extract failed: {e}\n"))
}

/// Read an edited `states:` file back into a machine. Names and bare hashes both resolve; the
/// shape (node and state counts) is whatever the file declares — `serialize_state_machine` is what
/// then rejects a shape the target container cannot take.
pub fn parse(yaml: &str) -> Result<StateMachine, String> {
    let doc: StatesDoc =
        serde_norway::from_str(yaml).map_err(|e| format!("states file is not valid YAML: {e}"))?;
    Ok(StateMachine {
        switch_slots: doc.switch_slots.iter().map(Token::resolve).collect(),
        nodes: doc
            .nodes
            .iter()
            .map(|n| SwitchNodeDef {
                name_hash: n.name.resolve(),
                states: n
                    .states
                    .iter()
                    .map(|s| StateDef {
                        name_hash: s.name.resolve(),
                        enter: s.enter.iter().map(Token::resolve).collect(),
                        exit: s.exit.iter().map(Token::resolve).collect(),
                    })
                    .collect(),
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn machine() -> StateMachine {
        StateMachine {
            switch_slots: vec![0xAABB_CCDD],
            nodes: vec![SwitchNodeDef {
                name_hash: pandemic_hash_m2("DamageNode"),
                states: vec![
                    StateDef { name_hash: pandemic_hash_m2("PristineState"), enter: vec![], exit: vec![] },
                    StateDef {
                        name_hash: pandemic_hash_m2("DestroyedState"),
                        // 1 <arg> 2 <cmd> 3 — a real command script shape.
                        enter: vec![1, pandemic_hash_m2("wreck_body"), 2, pandemic_hash_m2("SHOW"), 3],
                        exit: vec![],
                    },
                ],
            }],
        }
    }

    /// Extract → parse is the identity on the machine — the whole "edit the extracted baseline"
    /// workflow rests on this: what you dumped is exactly what you get back if you change nothing.
    #[test]
    fn extract_then_parse_round_trips() {
        let sm = machine();
        // A resolver that knows the names we hashed (mimics the name table).
        let known = [
            "DamageNode", "PristineState", "DestroyedState", "wreck_body", "SHOW",
        ];
        let name_of = move |h: u32| known.iter().find(|n| pandemic_hash_m2(n) == h).map(|s| s.to_string());
        let yaml = extract(&sm, name_of);
        // The extracted file should read in names, not hex, and keep the opcodes as integers.
        assert!(yaml.contains("DestroyedState"), "{yaml}");
        assert!(yaml.contains("SHOW"), "{yaml}");

        let back = parse(&yaml).expect("re-parse");
        assert_eq!(back.switch_slots, sm.switch_slots);
        assert_eq!(back.nodes.len(), sm.nodes.len());
        assert_eq!(back.nodes[0].name_hash, sm.nodes[0].name_hash);
        assert_eq!(back.nodes[0].states[1].enter, sm.nodes[0].states[1].enter);
    }

    /// An unnamed hash survives as a bare `0x…`, and re-parses to the same value.
    #[test]
    fn an_unnamed_hash_survives_as_hex() {
        let sm = StateMachine {
            switch_slots: vec![],
            nodes: vec![SwitchNodeDef {
                name_hash: 0x1234_5678,
                states: vec![StateDef { name_hash: 0x9ABC_DEF0, enter: vec![], exit: vec![] }],
            }],
        };
        let yaml = extract(&sm, |_| None);
        assert!(yaml.contains("0x12345678"), "{yaml}");
        let back = parse(&yaml).unwrap();
        assert_eq!(back.nodes[0].name_hash, 0x1234_5678);
        assert_eq!(back.nodes[0].states[0].name_hash, 0x9ABC_DEF0);
    }

    /// A rename is expressed by editing a state's `name:` — the common edit, and it lands.
    #[test]
    fn editing_a_state_name_changes_the_hash() {
        let yaml = "nodes:\n  - name: DamageNode\n    states:\n      - name: MyCustomState\n        enter: []\n        exit: []\n";
        let sm = parse(yaml).unwrap();
        assert_eq!(sm.nodes[0].states[0].name_hash, pandemic_hash_m2("MyCustomState"));
    }
}
