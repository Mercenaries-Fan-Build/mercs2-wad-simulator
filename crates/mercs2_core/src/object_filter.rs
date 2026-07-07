//! `ObjectFilter` — the script-side object query predicate (`ObjectFilter.*` Lua namespace).
//!
//! A filter is a **label boolean-expression** (`"Hero||(China&&Vehicle)"`, `"human"`) combined with an
//! explicit **include / exclude** object set and an optional `UsePlayers` flag. Mission Lua creates
//! filters, configures them, then evaluates them against candidate objects (targeting, triggers,
//! "everyone in this faction driving a vehicle"). The predicate grammar is recovered verbatim from the
//! shipped scripts:
//!
//! ```text
//! expr    := or
//! or      := and ( "||" and )*
//! and     := unary ( "&&" unary )*
//! unary   := "!" unary | primary
//! primary := "(" expr ")" | label
//! label   := [A-Za-z0-9_]+        (case-insensitive; tested via the caller's has_label predicate)
//! ```
//!
//! The engine owns the mechanism (the set + the evaluator); the label vocabulary is authored content.

use std::collections::HashMap;

/// A single object-query filter: a label predicate + explicit include/exclude sets + a players flag.
#[derive(Clone, Debug, Default)]
pub struct ObjectFilter {
    /// The label boolean-expression (`SetFilter`); empty = match nothing by predicate (only the
    /// explicit include set matches).
    pub expr: String,
    /// Explicitly included object GUIDs (`AddObject(f, guid, true)`).
    pub include: Vec<u64>,
    /// Explicitly excluded object GUIDs (`AddObject(f, guid, false)`) — excluded even if the predicate
    /// or include set would otherwise match.
    pub exclude: Vec<u64>,
    /// `UsePlayers` — player-controlled characters count as matches.
    pub use_players: bool,
}

impl ObjectFilter {
    pub fn new() -> Self {
        Self::default()
    }

    /// `AddObject(f, guid, bInclude)` — add to the include set (`bInclude`) or the exclude set.
    pub fn add(&mut self, guid: u64, include: bool) {
        let (set, other) = if include {
            (&mut self.include, &mut self.exclude)
        } else {
            (&mut self.exclude, &mut self.include)
        };
        other.retain(|&g| g != guid);
        if !set.contains(&guid) {
            set.push(guid);
        }
    }

    /// `RemoveObject(f, guid)` — drop from both sets.
    pub fn remove(&mut self, guid: u64) {
        self.include.retain(|&g| g != guid);
        self.exclude.retain(|&g| g != guid);
    }

    /// `ClearObjects(f)` — empty both explicit sets (the predicate is kept).
    pub fn clear_objects(&mut self) {
        self.include.clear();
        self.exclude.clear();
    }

    /// Whether `guid` passes this filter given a label lookup. Explicit exclude wins; explicit include
    /// always matches; otherwise the label expression decides (an empty expression matches nothing).
    pub fn matches(&self, guid: u64, has_label: impl Fn(&str) -> bool) -> bool {
        if self.exclude.contains(&guid) {
            return false;
        }
        if self.include.contains(&guid) {
            return true;
        }
        if self.expr.trim().is_empty() {
            return false;
        }
        eval_label_expr(&self.expr, &has_label).unwrap_or(false)
    }
}

/// Evaluate a label boolean-expression against a `has_label` predicate. Returns `None` on a malformed
/// expression (the engine is tolerant — callers treat `None` as "no match").
pub fn eval_label_expr(expr: &str, has_label: &impl Fn(&str) -> bool) -> Option<bool> {
    let tokens = tokenize(expr)?;
    let mut p = Parser { tokens: &tokens, pos: 0, has_label };
    let v = p.parse_or()?;
    if p.pos != p.tokens.len() {
        return None; // trailing garbage
    }
    Some(v)
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Or,
    And,
    Not,
    LParen,
    RParen,
    Label(String),
}

fn tokenize(s: &str) -> Option<Vec<Tok>> {
    let mut out = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        match c {
            b' ' | b'\t' => i += 1,
            b'(' => {
                out.push(Tok::LParen);
                i += 1;
            }
            b')' => {
                out.push(Tok::RParen);
                i += 1;
            }
            b'!' => {
                out.push(Tok::Not);
                i += 1;
            }
            b'|' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'|' {
                    out.push(Tok::Or);
                    i += 2;
                } else {
                    return None;
                }
            }
            b'&' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'&' {
                    out.push(Tok::And);
                    i += 2;
                } else {
                    return None;
                }
            }
            _ if c.is_ascii_alphanumeric() || c == b'_' => {
                let start = i;
                while i < bytes.len()
                    && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_')
                {
                    i += 1;
                }
                out.push(Tok::Label(s[start..i].to_string()));
            }
            _ => return None,
        }
    }
    Some(out)
}

struct Parser<'a, F: Fn(&str) -> bool> {
    tokens: &'a [Tok],
    pos: usize,
    has_label: &'a F,
}

impl<F: Fn(&str) -> bool> Parser<'_, F> {
    fn peek(&self) -> Option<&Tok> {
        self.tokens.get(self.pos)
    }

    fn parse_or(&mut self) -> Option<bool> {
        let mut v = self.parse_and()?;
        while self.peek() == Some(&Tok::Or) {
            self.pos += 1;
            let rhs = self.parse_and()?;
            v = v || rhs;
        }
        Some(v)
    }

    fn parse_and(&mut self) -> Option<bool> {
        let mut v = self.parse_unary()?;
        while self.peek() == Some(&Tok::And) {
            self.pos += 1;
            let rhs = self.parse_unary()?;
            v = v && rhs;
        }
        Some(v)
    }

    fn parse_unary(&mut self) -> Option<bool> {
        if self.peek() == Some(&Tok::Not) {
            self.pos += 1;
            return Some(!self.parse_unary()?);
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Option<bool> {
        match self.peek()? {
            Tok::LParen => {
                self.pos += 1;
                let v = self.parse_or()?;
                if self.peek() != Some(&Tok::RParen) {
                    return None;
                }
                self.pos += 1;
                Some(v)
            }
            Tok::Label(name) => {
                let name = name.clone();
                self.pos += 1;
                Some((self.has_label)(&name))
            }
            _ => None,
        }
    }
}

/// The script host's registry of live filters (`ObjectFilter.Create`/`Copy`/`_GC` mint/free handles).
#[derive(Default)]
pub struct ObjectFilterRegistry {
    filters: HashMap<u64, ObjectFilter>,
    next: u64,
}

impl ObjectFilterRegistry {
    pub fn new() -> Self {
        ObjectFilterRegistry { filters: HashMap::new(), next: 1 }
    }

    /// `ObjectFilter.Create()` — mint a fresh empty filter, returning its handle.
    pub fn create(&mut self) -> u64 {
        let id = self.next;
        self.next += 1;
        self.filters.insert(id, ObjectFilter::new());
        id
    }

    /// `ObjectFilter.Copy(src)` — duplicate an existing filter (or a fresh one if `src` is unknown).
    pub fn copy(&mut self, src: u64) -> u64 {
        let clone = self.filters.get(&src).cloned().unwrap_or_default();
        let id = self.next;
        self.next += 1;
        self.filters.insert(id, clone);
        id
    }

    pub fn get(&self, handle: u64) -> Option<&ObjectFilter> {
        self.filters.get(&handle)
    }

    pub fn get_mut(&mut self, handle: u64) -> Option<&mut ObjectFilter> {
        self.filters.get_mut(&handle)
    }

    /// `ObjectFilter._GC(f)` — free a filter handle.
    pub fn remove(&mut self, handle: u64) {
        self.filters.remove(&handle);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn labels(set: &[&str]) -> impl Fn(&str) -> bool {
        let owned: HashSet<String> = set.iter().map(|s| s.to_string()).collect();
        move |l: &str| owned.contains(l)
    }

    #[test]
    fn evaluates_the_recovered_grammar() {
        let hero_china_veh = labels(&["Hero", "China", "Vehicle"]);
        // "Hero||(China&&Vehicle)" — hero present ⇒ true.
        assert_eq!(eval_label_expr("Hero||(China&&Vehicle)", &hero_china_veh), Some(true));

        let china_veh = labels(&["China", "Vehicle"]);
        assert_eq!(eval_label_expr("Hero||(China&&Vehicle)", &china_veh), Some(true));

        let china_only = labels(&["China"]);
        assert_eq!(eval_label_expr("Hero||(China&&Vehicle)", &china_only), Some(false));

        // simple single-label
        assert_eq!(eval_label_expr("human", &labels(&["human"])), Some(true));
        assert_eq!(eval_label_expr("human", &labels(&["vehicle"])), Some(false));

        // negation + precedence: !A && B
        assert_eq!(eval_label_expr("!Hero&&China", &china_only), Some(true));
        assert_eq!(eval_label_expr("!Hero&&China", &hero_china_veh), Some(false));

        // malformed ⇒ None
        assert_eq!(eval_label_expr("Hero||", &china_only), None);
        assert_eq!(eval_label_expr("Hero &", &china_only), None);
    }

    #[test]
    fn filter_include_exclude_and_predicate() {
        let mut f = ObjectFilter::new();
        f.expr = "Vehicle".to_string();
        let is_vehicle = labels(&["Vehicle"]);
        let not_vehicle = labels(&["human"]);

        // predicate match
        assert!(f.matches(10, &is_vehicle));
        assert!(!f.matches(10, &not_vehicle));

        // explicit include beats a failing predicate
        f.add(20, true);
        assert!(f.matches(20, &not_vehicle));

        // explicit exclude beats a passing predicate
        f.add(10, false);
        assert!(!f.matches(10, &is_vehicle));

        // remove clears both; adding include then exclude moves it
        f.remove(10);
        assert!(f.matches(10, &is_vehicle));
    }

    #[test]
    fn registry_mints_and_copies() {
        let mut reg = ObjectFilterRegistry::new();
        let a = reg.create();
        reg.get_mut(a).unwrap().expr = "human".into();
        reg.get_mut(a).unwrap().add(5, true);
        let b = reg.copy(a);
        assert_ne!(a, b);
        assert_eq!(reg.get(b).unwrap().expr, "human");
        assert_eq!(reg.get(b).unwrap().include, vec![5]);
        reg.remove(a);
        assert!(reg.get(a).is_none());
        assert!(reg.get(b).is_some()); // copy is independent
    }
}
