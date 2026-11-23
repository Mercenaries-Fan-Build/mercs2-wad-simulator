//! Writing an edited value into a file a person wrote, keeping the rest of their text.
//!
//! [`rewrite`] reads the file, writes what it read and the edited value with the kind's own writer,
//! and changes in the file only what differs between those two texts:
//!
//! * a changed value takes the writer's spelling; the comment on its line stays;
//! * an added key or list item takes the writer's text, indented as the file indents it;
//! * a list item that is removed or moved takes the comment lines directly above it with it;
//! * an added list item is set off by a blank line when the list's items already are.
//!
//! The result has to read back as the edited value, or nothing is returned. YAML is edited through
//! `yaml-edit`'s syntax tree, TOML through `toml_edit`'s document, JSON through the spans of its own
//! small reader: a JSON object or list that gains, loses or reorders members is laid out as the file
//! lays it out (its line breaks, its indent step, or all on one line).

use std::collections::{HashMap, VecDeque};

use crate::Format;

/// `text`, which reads as some value, changed to read as `new`. `read` and `write` are the kind's
/// own reader and writer. Refused when `text` does not read, when the edit cannot be placed in
/// `text`, or when the result does not read back as `new`.
pub fn rewrite<T>(
    text: &str,
    format: Format,
    new: &T,
    read: impl Fn(&str) -> Result<T, String>,
    write: impl Fn(&T) -> Result<String, String>,
) -> Result<String, String> {
    let old = read(text).map_err(|e| format!("the file does not read: {e}"))?;
    let (was, will) = (write(&old)?, write(new)?);
    if was == will {
        return Ok(text.to_string());
    }
    let out = match format {
        Format::Yaml => yaml_text::rewrite(text, &was, &will)?,
        Format::Toml => toml_text::rewrite(text, &was, &will)?,
        Format::Json => json_text::rewrite(text, &was, &will)?,
    };
    let back = read(&out).map_err(|e| format!("the edited file does not read back: {e}"))?;
    let again = write(&back)?;
    if again != will {
        return Err(format!("the edited file reads back otherwise: {}", first_difference(&again, &will)));
    }
    Ok(out)
}

/// The first line two texts differ on.
pub fn first_difference(a: &str, b: &str) -> String {
    let (x, y): (Vec<&str>, Vec<&str>) = (a.split('\n').collect(), b.split('\n').collect());
    let n = x.iter().zip(&y).position(|(p, q)| p != q).unwrap_or(x.len().min(y.len()));
    let shown = |l: Option<&&str>| l.map_or("(no line)".to_string(), |s| format!("`{}`", s.trim_end_matches('\r')));
    format!("line {} reads {} where qm writes {}", n + 1, shown(x.get(n)), shown(y.get(n)))
}

/// Where an item of a list that was edited comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    /// Item `i` of the list as read, unchanged and in order.
    Stays(usize),
    /// Item `i` of the list as read, edited in place.
    Changed(usize),
    /// Item `i` of the list as read, unchanged, from elsewhere in the list.
    Moved(usize),
    /// Not in the list as read.
    New,
}

/// How the items of `new` line up with the items of `old` (each item as its text): a slot per new
/// item, and the old items that leave their place (removed, or moved).
///
/// Equal texts pair up in order; the longest run of pairs in order stays, and the other pairs are
/// moves. Between two items that stay, the unpaired old and new items pair up in order as edits in
/// place; what is left over is removed or added.
fn align<K: AsRef<str>>(old: &[K], new: &[K]) -> (Vec<Slot>, Vec<usize>) {
    let mut by_text: HashMap<&str, VecDeque<usize>> = HashMap::new();
    for (i, k) in old.iter().enumerate() {
        by_text.entry(k.as_ref()).or_default().push_back(i);
    }
    let paired: Vec<Option<usize>> =
        new.iter().map(|k| by_text.get_mut(k.as_ref()).and_then(|q| q.pop_front())).collect();

    // The longest increasing run of paired old indices, in new order (patience sorting).
    let pairs: Vec<(usize, usize)> = paired.iter().enumerate().filter_map(|(j, m)| m.map(|i| (j, i))).collect();
    let mut tails: Vec<usize> = Vec::new();
    let mut back: Vec<Option<usize>> = vec![None; pairs.len()];
    for (p, &(_, i)) in pairs.iter().enumerate() {
        let at = tails.partition_point(|&t| pairs[t].1 < i);
        back[p] = at.checked_sub(1).map(|a| tails[a]);
        if at == tails.len() {
            tails.push(p);
        } else {
            tails[at] = p;
        }
    }
    let mut stays = vec![false; pairs.len()];
    let mut p = tails.last().copied();
    while let Some(q) = p {
        stays[q] = true;
        p = back[q];
    }

    let mut slots = vec![Slot::New; new.len()];
    let mut used = vec![false; old.len()];
    let mut anchors: Vec<(usize, usize)> = Vec::new();
    for (p, &(j, i)) in pairs.iter().enumerate() {
        used[i] = true;
        if stays[p] {
            slots[j] = Slot::Stays(i);
            anchors.push((i, j));
        } else {
            slots[j] = Slot::Moved(i);
        }
    }
    let mut gone = Vec::new();
    let (mut oi, mut nj) = (0, 0);
    for (ai, aj) in anchors.into_iter().chain(std::iter::once((old.len(), new.len()))) {
        let olds: Vec<usize> = (oi..ai).filter(|&i| !used[i]).collect();
        let news: Vec<usize> = (nj..aj).filter(|&j| slots[j] == Slot::New).collect();
        for (k, &i) in olds.iter().enumerate() {
            match news.get(k) {
                Some(&j) => slots[j] = Slot::Changed(i),
                None => gone.push(i),
            }
        }
        (oi, nj) = (ai + 1, aj + 1);
    }
    for s in &slots {
        if let Slot::Moved(i) = s {
            gone.push(*i);
        }
    }
    gone.sort_unstable();
    (slots, gone)
}

/// One change to a text: `[at, end)` replaced by `text`.
#[derive(Debug)]
struct Edit {
    at: usize,
    end: usize,
    text: String,
}

/// `src` with `edits` made. Deletions may overlap one another; any other overlap is refused.
fn apply(src: &str, mut edits: Vec<Edit>) -> Result<String, String> {
    edits.sort_by_key(|e| (e.at, e.end));
    let mut out = String::with_capacity(src.len());
    let mut cur = 0;
    for e in edits {
        if e.at < cur {
            if e.text.is_empty() {
                cur = cur.max(e.end);
                continue;
            }
            return Err(format!("two changes meet at byte {}; the file is left as it is", e.at));
        }
        out.push_str(&src[cur..e.at]);
        out.push_str(&e.text);
        cur = e.end;
    }
    out.push_str(&src[cur..]);
    Ok(out)
}

fn line_start(src: &str, at: usize) -> usize {
    src[..at].rfind('\n').map_or(0, |i| i + 1)
}

/// The offset just past the newline ending the line `at` is on (the end of `src` on the last line).
fn line_end(src: &str, at: usize) -> usize {
    src[at..].find('\n').map_or(src.len(), |i| at + i + 1)
}

fn col(src: &str, at: usize) -> usize {
    at - line_start(src, at)
}

fn only_space_before(src: &str, at: usize) -> bool {
    src[line_start(src, at)..at].chars().all(|c| c == ' ' || c == '\t')
}

fn is_blank(line: &str) -> bool {
    line.trim().is_empty()
}

/// The start of the comment lines directly above the line starting at `ls`, each indented no
/// deeper than `max_col`.
fn comments_above(src: &str, ls: usize, max_col: usize) -> usize {
    let mut s = ls;
    while s > 0 {
        let prev = line_start(src, s - 1);
        let line = &src[prev..s];
        let t = line.trim_start();
        if t.starts_with('#') && line.len() - t.len() <= max_col {
            s = prev;
        } else {
            break;
        }
    }
    s
}

/// `text` with every line after the first shifted by `shift` spaces (`all`: the first too).
fn reindent(text: &str, shift: isize, all: bool) -> Result<String, String> {
    if shift == 0 {
        return Ok(text.to_string());
    }
    let mut out = String::with_capacity(text.len());
    for (k, line) in text.split_inclusive('\n').enumerate() {
        if (k == 0 && !all) || is_blank(line) {
            out.push_str(line);
        } else if shift > 0 {
            out.push_str(&" ".repeat(shift as usize));
            out.push_str(line);
        } else {
            let cut = (-shift) as usize;
            if line.len() < cut || !line[..cut].chars().all(|c| c == ' ') {
                return Err(format!("`{}` cannot be indented to the file's depth", line.trim_end()));
            }
            out.push_str(&line[cut..]);
        }
    }
    Ok(out)
}

mod yaml_text {
    use std::str::FromStr;

    use rowan::ast::AstNode;
    use yaml_edit::{Lang, SyntaxKind as K, YamlFile};

    use super::{
        align, apply, col, comments_above, is_blank, line_end, line_start, only_space_before, reindent, Edit,
        Slot,
    };

    type Node = rowan::SyntaxNode<Lang>;

    pub(super) fn rewrite(a: &str, o: &str, n: &str) -> Result<String, String> {
        let ar = root(a, "the file")?;
        let or = root(o, "qm's text of the file")?;
        let nr = root(n, "qm's text of the edit")?;
        let mut e = Editor { a, o, n, edits: Vec::new() };
        match (ar, or, nr) {
            (Some(ac), Some(oc), Some(nc)) => e.node(Place::Root, &ac, &oc, &nc, "")?,
            (None, _, Some(nc)) => {
                let lead = if a.is_empty() || a.ends_with('\n') { "" } else { "\n" };
                e.edits.push(Edit { at: a.len(), end: a.len(), text: format!("{lead}{}\n", e.tn(&nc)) });
            }
            (_, _, None) => return Err("the edit writes an empty file".into()),
            (Some(_), None, Some(_)) => return Err("qm writes the file as holding no value".into()),
        }
        apply(a, e.edits)
    }

    /// The one document's value; `None` for a file holding none.
    fn root(src: &str, what: &str) -> Result<Option<Node>, String> {
        let file = YamlFile::from_str(src).map_err(|e| format!("{what} does not parse: {e}"))?;
        let docs: Vec<Node> = file.syntax().children().filter(|c| c.kind() == K::DOCUMENT).collect();
        match docs.as_slice() {
            [] => Ok(None),
            [d] => Ok(content(d)),
            _ => Err(format!("{what} holds {} documents", docs.len())),
        }
    }

    fn is_content(k: K) -> bool {
        matches!(k, K::MAPPING | K::SEQUENCE | K::SCALAR | K::TAGGED_NODE | K::ALIAS)
    }

    fn content(n: &Node) -> Option<Node> {
        n.children().find(|c| is_content(c.kind()))
    }

    fn trivia(k: K) -> bool {
        matches!(k, K::WHITESPACE | K::NEWLINE | K::INDENT | K::COMMENT)
    }

    /// Where the node's own text starts.
    fn first(n: &Node) -> usize {
        n.descendants_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| !trivia(t.kind()))
            .map_or(n.text_range().start().into(), |t| t.text_range().start().into())
    }

    /// Where the node's own text ends: before any comment or line break after it.
    fn end(n: &Node) -> usize {
        n.descendants_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|t| !trivia(t.kind()))
            .last()
            .map_or(first(n), |t| t.text_range().end().into())
    }

    fn is_flow(n: &Node) -> bool {
        n.children_with_tokens().any(|c| matches!(c.kind(), K::LEFT_BRACKET | K::LEFT_BRACE))
    }

    fn tag(n: &Node) -> Option<String> {
        n.children_with_tokens().find(|c| c.kind() == K::TAG).and_then(|c| c.into_token()).map(|t| t.text().to_string())
    }

    struct Entry {
        key: String,
        node: Node,
        key_at: usize,
        colon: usize,
        value: Option<Node>,
    }

    fn entries(map: &Node, src: &str) -> Result<Vec<Entry>, String> {
        let mut out = Vec::new();
        for e in map.children().filter(|c| c.kind() == K::MAPPING_ENTRY) {
            let key = e
                .children()
                .find(|c| c.kind() == K::KEY)
                .and_then(|k| content(&k))
                .filter(|k| k.kind() == K::SCALAR)
                .ok_or_else(|| format!("line {}: a key that is not plain text", line_of(src, first(&e))))?;
            let colon = e
                .children_with_tokens()
                .find(|c| c.kind() == K::COLON)
                .map(|c| usize::from(c.text_range().end()))
                .ok_or_else(|| format!("line {}: a key with no colon", line_of(src, first(&e))))?;
            let value = e.children().find(|c| c.kind() == K::VALUE).and_then(|v| content(&v));
            out.push(Entry { key: unquote(&src[first(&key)..end(&key)]), key_at: first(&key), node: e, colon, value });
        }
        Ok(out)
    }

    fn unquote(s: &str) -> String {
        let b = s.as_bytes();
        if b.len() >= 2 && (b[0] == b'"' || b[0] == b'\'') && b[b.len() - 1] == b[0] {
            s[1..s.len() - 1].to_string()
        } else {
            s.to_string()
        }
    }

    fn items(seq: &Node) -> Vec<Node> {
        seq.children().filter(|c| c.kind() == K::SEQUENCE_ENTRY).collect()
    }

    fn line_of(src: &str, at: usize) -> usize {
        src[..at].matches('\n').count() + 1
    }

    /// A node as flow YAML (inside `[…]` or `{…}`).
    fn flow(n: &Node, src: &str) -> Result<String, String> {
        Ok(match n.kind() {
            K::SEQUENCE => {
                let parts: Result<Vec<String>, String> =
                    items(n).iter().map(|i| content(i).map_or(Ok("null".into()), |c| flow(&c, src))).collect();
                format!("[{}]", parts?.join(", "))
            }
            K::MAPPING => {
                let parts: Result<Vec<String>, String> = entries(n, src)?
                    .iter()
                    .map(|e| {
                        let v = e.value.as_ref().map_or(Ok("null".into()), |v| flow(v, src))?;
                        Ok(format!("{}: {v}", &src[e.key_at..end(&e.node.children().find(|c| c.kind() == K::KEY).expect("an entry has a key"))]))
                    })
                    .collect();
                format!("{{{}}}", parts?.join(", "))
            }
            K::TAGGED_NODE => {
                let inner = content(n).map_or(Ok(String::new()), |c| flow(&c, src))?;
                format!("{} {inner}", tag(n).unwrap_or_default())
            }
            _ => {
                let t = &src[first(n)..end(n)];
                if t.contains('\n') {
                    return Err(format!("line {}: text over several lines inside [ ] or {{ }}", line_of(src, first(n))));
                }
                t.to_string()
            }
        })
    }

    #[derive(Clone, Copy)]
    enum Place {
        Root,
        /// The value of a block mapping entry: the file's colon and key column, qm's key column.
        Value { colon: usize, key: usize, n_key: usize },
        /// The value of a block list item.
        Item,
        /// Inside `[…]` or `{…}`.
        Flow,
    }

    struct Editor<'s> {
        a: &'s str,
        o: &'s str,
        n: &'s str,
        edits: Vec<Edit>,
    }

    impl Editor<'_> {
        fn to(&self, n: &Node) -> &str {
            &self.o[first(n)..end(n)]
        }

        fn tn(&self, n: &Node) -> &str {
            &self.n[first(n)..end(n)]
        }

        fn edit(&mut self, at: usize, end: usize, text: String) {
            self.edits.push(Edit { at, end, text });
        }

        fn node(&mut self, place: Place, a: &Node, o: &Node, n: &Node, path: &str) -> Result<(), String> {
            if self.to(o) == self.tn(n) {
                return Ok(());
            }
            match (a.kind(), o.kind(), n.kind()) {
                (K::MAPPING, K::MAPPING, K::MAPPING) => self.map(place, a, o, n, path),
                (K::SEQUENCE, K::SEQUENCE, K::SEQUENCE) => self.seq(place, a, o, n, path),
                (K::TAGGED_NODE, K::TAGGED_NODE, K::TAGGED_NODE) if tag(a) == tag(o) && tag(o) == tag(n) => {
                    match (content(a), content(o), content(n)) {
                        (Some(ac), Some(oc), Some(nc)) => self.node(place, &ac, &oc, &nc, path),
                        _ => self.replace(place, a, n),
                    }
                }
                _ => self.replace(place, a, n),
            }
        }

        /// The file's node `a` replaced with qm's text of `n`.
        fn replace(&mut self, place: Place, a: &Node, n: &Node) -> Result<(), String> {
            let (a0, a1) = (first(a), end(a));
            let n0 = first(n);
            let text = self.tn(n).to_string();
            match place {
                Place::Flow => {
                    let t = flow(n, self.n)?;
                    self.edit(a0, a1, t);
                }
                Place::Value { colon, key, n_key } => {
                    // A block list or mapping starts on its own line, however short.
                    let block = |x: &Node| matches!(x.kind(), K::SEQUENCE | K::MAPPING) && !is_flow(x);
                    let block = block(n) || (n.kind() == K::TAGGED_NODE && content(n).is_some_and(|c| block(&c)));
                    if !block && !text.contains('\n') {
                        self.edit(colon, a1, format!(" {text}"));
                    } else if only_space_before(self.n, n0) {
                        let depth = key + (col(self.n, n0) - n_key.min(col(self.n, n0)));
                        let body = reindent(&text, depth as isize - col(self.n, n0) as isize, false)?;
                        self.edit(colon, a1, format!("\n{}{body}", " ".repeat(depth)));
                    } else {
                        let body = reindent(&text, key as isize - n_key as isize, false)?;
                        self.edit(colon, a1, format!(" {body}"));
                    }
                }
                Place::Item | Place::Root => {
                    let body = reindent(&text, col(self.a, a0) as isize - col(self.n, n0) as isize, false)?;
                    self.edit(a0, a1, body);
                }
            }
            Ok(())
        }

        fn map(&mut self, place: Place, a: &Node, o: &Node, n: &Node, path: &str) -> Result<(), String> {
            let (ae, oe, ne) = (entries(a, self.a)?, entries(o, self.o)?, entries(n, self.n)?);
            let has = |es: &[Entry], k: &str| es.iter().any(|e| e.key == k);
            let flowing = is_flow(a) || is_flow(o) || is_flow(n);
            let same_keys = oe.len() == ne.len() && oe.iter().zip(&ne).all(|(x, y)| x.key == y.key);
            let all_gone = ae.iter().all(|e| !has(&ne, &e.key));
            if ne.is_empty() || ae.is_empty() || all_gone || (flowing && !(same_keys && ne.iter().all(|e| has(&ae, &e.key)))) {
                return self.replace(place, a, n);
            }
            let at = |es: &[Entry], k: &str| es.iter().position(|e| e.key == k);
            if flowing {
                for x in &ne {
                    let (ai, oi) = (at(&ae, &x.key).expect("checked above"), at(&oe, &x.key).expect("checked above"));
                    match (&ae[ai].value, &oe[oi].value, &x.value) {
                        (Some(av), Some(ov), Some(nv)) => self.node(Place::Flow, av, ov, nv, &format!("{path}.{}", x.key))?,
                        _ => return self.replace(place, a, n),
                    }
                }
                return Ok(());
            }
            let a_col = col(self.a, ae[0].key_at);
            for (j, x) in ne.iter().enumerate() {
                let child = if path.is_empty() { x.key.clone() } else { format!("{path}.{}", x.key) };
                let was = at(&oe, &x.key).map(|i| &oe[i]);
                let unchanged = was.is_some_and(|w| match (&w.value, &x.value) {
                    (Some(p), Some(q)) => self.to(p) == self.tn(q),
                    (None, None) => true,
                    _ => false,
                });
                match at(&ae, &x.key) {
                    Some(ai) if !unchanged => {
                        let ent = &ae[ai];
                        let place = Place::Value { colon: ent.colon, key: col(self.a, ent.key_at), n_key: col(self.n, x.key_at) };
                        match (&ent.value, was.and_then(|w| w.value.as_ref()), &x.value) {
                            (Some(av), Some(ov), Some(nv)) => self.node(place, av, ov, nv, &child)?,
                            (Some(av), None, Some(nv)) => self.replace(place, av, nv)?,
                            (None, _, Some(nv)) => {
                                let t = self.tn(nv).to_string();
                                let t = reindent(&t, place_shift(place), false)?;
                                self.edit(ent.colon, ent.colon, format!(" {t}"));
                            }
                            (_, _, None) => return Err(format!("{child}: qm writes the key with no value")),
                        }
                    }
                    Some(_) => {}
                    None if unchanged => {}
                    None => {
                        let prev = ne[..j].iter().rev().find_map(|p| at(&ae, &p.key));
                        self.insert_entry(&ae, prev, x, a_col)?;
                    }
                }
            }
            for (i, e) in ae.iter().enumerate() {
                if has(&oe, &e.key) && !has(&ne, &e.key) {
                    self.remove_entry(&ae, i);
                }
            }
            Ok(())
        }

        /// qm's entry `x` added to the file's block mapping, after its entry `prev`.
        fn insert_entry(&mut self, ae: &[Entry], prev: Option<usize>, x: &Entry, a_col: usize) -> Result<(), String> {
            let text = &self.n[x.key_at..end(&x.node)];
            let body = reindent(text, a_col as isize - col(self.n, x.key_at) as isize, false)?;
            let pad = " ".repeat(a_col);
            let first = &ae[0];
            let at = match prev {
                Some(p) => line_end(self.a, end(&ae[p].node)),
                None if only_space_before(self.a, first.key_at) => line_start(self.a, first.key_at),
                None => line_end(self.a, end(&first.node)),
            };
            let lead = if at > 0 && !self.a[..at].ends_with('\n') { "\n" } else { "" };
            self.edit(at, at, format!("{lead}{pad}{body}\n"));
            Ok(())
        }

        fn remove_entry(&mut self, ae: &[Entry], i: usize) {
            let e = &ae[i];
            if only_space_before(self.a, e.key_at) {
                let s = comments_above(self.a, line_start(self.a, e.key_at), col(self.a, e.key_at));
                let t = line_end(self.a, end(&e.node));
                self.edit(s, t, String::new());
            } else if let Some(next) = ae.get(i + 1) {
                self.edit(e.key_at, next.key_at, String::new());
            }
        }

        fn seq(&mut self, place: Place, a: &Node, o: &Node, n: &Node, path: &str) -> Result<(), String> {
            let (ai, oi, ni) = (items(a), items(o), items(n));
            let item_path = |j: usize| format!("{path}[{j}]");
            if is_flow(a) || is_flow(o) || is_flow(n) || ai.is_empty() || ni.is_empty() {
                if is_flow(a) && ai.len() == oi.len() && oi.len() == ni.len() {
                    let mut pairs = Vec::new();
                    for k in 0..ai.len() {
                        match (content(&ai[k]), content(&oi[k]), content(&ni[k])) {
                            (Some(x), Some(y), Some(z)) => pairs.push((x, y, z)),
                            _ => return self.replace(place, a, n),
                        }
                    }
                    for (k, (x, y, z)) in pairs.iter().enumerate() {
                        self.node(Place::Flow, x, y, z, &item_path(k))?;
                    }
                    return Ok(());
                }
                return self.replace(place, a, n);
            }
            if ai.len() != oi.len() {
                return Err(format!(
                    "{}: the file lists {} items where it reads as {}; the file is left as it is",
                    if path.is_empty() { "the list" } else { path },
                    ai.len(),
                    oi.len()
                ));
            }
            let scalars = |xs: &[Node]| xs.iter().all(|x| content(x).is_some_and(|c| c.kind() == K::SCALAR));
            let (slots, gone) = if ai.len() == ni.len() && scalars(&oi) && scalars(&ni) {
                // A list of plain values the same length: each value is edited where it stands.
                let slots = (0..ni.len()).map(|k| if self.to(&oi[k]) == self.tn(&ni[k]) { Slot::Stays(k) } else { Slot::Changed(k) });
                (slots.collect(), Vec::new())
            } else {
                let keys_o: Vec<&str> = oi.iter().map(|x| self.to(x)).collect();
                let keys_n: Vec<&str> = ni.iter().map(|x| self.tn(x)).collect();
                align(&keys_o, &keys_n)
            };
            let reshaped = !gone.is_empty() || slots.iter().any(|s| matches!(s, Slot::Moved(_) | Slot::New));
            if reshaped
                && (ai.iter().any(|x| !only_space_before(self.a, first(x))) || ni.iter().any(|x| !only_space_before(self.n, first(x))))
            {
                return self.replace(place, a, n);
            }
            for (j, s) in slots.iter().enumerate() {
                if let Slot::Changed(i) = *s {
                    match (content(&ai[i]), content(&oi[i]), content(&ni[j])) {
                        (Some(x), Some(y), Some(z)) => self.node(Place::Item, &x, &y, &z, &item_path(j))?,
                        _ => return Err(format!("{}: an item with no value", item_path(j))),
                    }
                }
            }
            if !reshaped {
                return Ok(());
            }
            let block = |src: &str, x: &Node| {
                let d = first(x);
                (comments_above(src, line_start(src, d), col(src, d)), line_end(src, end(x)))
            };
            let a_blocks: Vec<(usize, usize)> = ai.iter().map(|x| block(self.a, x)).collect();
            let spaced = a_blocks.windows(2).any(|w| self.a[w[0].1..w[1].0].split_inclusive('\n').any(is_blank));
            let sep = if spaced { "\n" } else { "" };
            for &i in &gone {
                let (s, e) = a_blocks[i];
                if i + 1 < ai.len() {
                    let mut t = e;
                    while t < self.a.len() && is_blank(&self.a[t..line_end(self.a, t)]) {
                        t = line_end(self.a, t);
                    }
                    self.edit(s, t, String::new());
                } else if i > 0 {
                    let mut t = s;
                    while t > 0 && is_blank(&self.a[line_start(self.a, t - 1)..t]) {
                        t = line_start(self.a, t - 1);
                    }
                    self.edit(t, e, String::new());
                } else {
                    self.edit(s, e, String::new());
                }
            }
            let staying = |s: &Slot| match *s {
                Slot::Stays(i) | Slot::Changed(i) => Some(i),
                _ => None,
            };
            let last = slots.iter().rev().find_map(staying);
            let shift = col(self.a, first(&ai[0])) as isize - col(self.n, first(&ni[0])) as isize;
            for (j, s) in slots.iter().enumerate() {
                let text = match *s {
                    Slot::Moved(i) => self.a[a_blocks[i].0..a_blocks[i].1].to_string(),
                    Slot::New => {
                        let (s0, e0) = block(self.n, &ni[j]);
                        reindent(&self.n[s0..e0], shift, true)?
                    }
                    _ => continue,
                };
                let text = if text.ends_with('\n') { text } else { format!("{text}\n") };
                match slots[j + 1..].iter().find_map(staying) {
                    Some(i) => self.edit(a_blocks[i].0, a_blocks[i].0, format!("{text}{sep}")),
                    None => match last {
                        Some(i) => {
                            let at = a_blocks[i].1;
                            let lead = if self.a[..at].ends_with('\n') { "" } else { "\n" };
                            self.edit(at, at, format!("{lead}{sep}{text}"));
                        }
                        None => self.edit(a_blocks[0].0, a_blocks[0].0, format!("{text}{sep}")),
                    },
                }
            }
            Ok(())
        }
    }

    /// The shift a value written after the colon takes for its continuation lines.
    fn place_shift(place: Place) -> isize {
        match place {
            Place::Value { key, n_key, .. } => key as isize - n_key as isize,
            _ => 0,
        }
    }
}

mod toml_text {
    use toml_edit::{Array, ArrayOfTables, DocumentMut, InlineTable, Item, Table, Value};

    use super::{align, Slot};

    pub(super) fn rewrite(a: &str, o: &str, n: &str) -> Result<String, String> {
        let mut ad: DocumentMut = a.parse().map_err(|e| format!("the file does not parse: {e}"))?;
        let od: DocumentMut = o.parse().map_err(|e| format!("qm's text of the file does not parse: {e}"))?;
        let nd: DocumentMut = n.parse().map_err(|e| format!("qm's text of the edit does not parse: {e}"))?;
        table(ad.as_table_mut(), od.as_table(), nd.as_table(), "")?;
        Ok(ad.to_string())
    }

    /// A value's identity apart from how it is laid out.
    fn canon_value(v: &Value) -> String {
        match v {
            Value::Array(a) => format!("[{}]", a.iter().map(canon_value).collect::<Vec<_>>().join(",")),
            Value::InlineTable(t) => {
                format!("{{{}}}", t.iter().map(|(k, v)| format!("{k:?}={}", canon_value(v))).collect::<Vec<_>>().join(","))
            }
            other => {
                let mut v = other.clone();
                v.decor_mut().clear();
                v.to_string()
            }
        }
    }

    fn canon(i: &Item) -> String {
        match i {
            Item::None => "none".into(),
            Item::Value(v) => canon_value(v),
            Item::Table(t) => {
                format!("{{{}}}", t.iter().map(|(k, v)| format!("{k:?}={}", canon(v))).collect::<Vec<_>>().join(","))
            }
            Item::ArrayOfTables(a) => format!(
                "[[{}]]",
                a.iter().map(|t| canon(&Item::Table(t.clone()))).collect::<Vec<_>>().join(",")
            ),
        }
    }

    /// `i` as a new item of the file: its tables placed by where they go, not where qm's text has them.
    fn fresh(i: &Item) -> Item {
        let mut i = i.clone();
        unplace(&mut i);
        i
    }

    fn unplace(i: &mut Item) {
        match i {
            Item::Table(t) => {
                t.set_position(None);
                for (_, v) in t.iter_mut() {
                    unplace(v);
                }
            }
            Item::ArrayOfTables(a) => {
                for t in a.iter_mut() {
                    t.set_position(None);
                    for (_, v) in t.iter_mut() {
                        unplace(v);
                    }
                }
            }
            _ => {}
        }
    }

    fn join(path: &str, key: &str) -> String {
        if path.is_empty() { key.to_string() } else { format!("{path}.{key}") }
    }

    fn table(a: &mut Table, o: &Table, n: &Table, path: &str) -> Result<(), String> {
        for (k, ni) in n.iter() {
            let oi = o.get(k);
            if oi.is_some_and(|oi| canon(oi) == canon(ni)) {
                continue;
            }
            let reshaped = a.get(k).is_some_and(|ai| ai.is_value() != ni.is_value());
            match (a.get_mut(k), oi) {
                (Some(ai), Some(oi)) => item(ai, oi, ni, &join(path, k))?,
                (Some(ai), None) => *ai = fresh(ni),
                (None, _) => {
                    let key = n.key(k).expect("the key is in the table").clone();
                    a.insert_formatted(&key, fresh(ni));
                }
            }
            if reshaped {
                // A header key (`[[items]]`) that now names a value (`items = []`), or the other way.
                if let Some(mut key) = a.key_mut(k) {
                    key.fmt();
                }
            }
        }
        for (k, _) in o.iter() {
            if !n.contains_key(k) {
                a.remove(k);
            }
        }
        Ok(())
    }

    fn item(a: &mut Item, o: &Item, n: &Item, path: &str) -> Result<(), String> {
        match (a, o, n) {
            (Item::Table(at), Item::Table(ot), Item::Table(nt)) => table(at, ot, nt, path),
            (Item::ArrayOfTables(aa), Item::ArrayOfTables(oa), Item::ArrayOfTables(na)) => tables(aa, oa, na, path),
            (Item::Value(av), Item::Value(ov), Item::Value(nv)) => value(av, ov, nv, path),
            (a, _, n) => {
                *a = fresh(n);
                Ok(())
            }
        }
    }

    fn value(a: &mut Value, o: &Value, n: &Value, path: &str) -> Result<(), String> {
        match (&mut *a, o, n) {
            (Value::Array(aa), Value::Array(oa), Value::Array(na)) => array(aa, oa, na, path),
            (Value::InlineTable(at), Value::InlineTable(ot), Value::InlineTable(nt)) => inline(at, ot, nt, path),
            _ => {
                let decor = a.decor().clone();
                *a = n.clone();
                *a.decor_mut() = decor;
                Ok(())
            }
        }
    }

    fn inline(a: &mut InlineTable, o: &InlineTable, n: &InlineTable, path: &str) -> Result<(), String> {
        for (k, nv) in n.iter() {
            let ov = o.get(k);
            if ov.is_some_and(|ov| canon_value(ov) == canon_value(nv)) {
                continue;
            }
            match (a.get_mut(k), ov) {
                (Some(av), Some(ov)) => value(av, ov, nv, &join(path, k))?,
                (Some(av), None) => *av = nv.clone(),
                (None, _) => {
                    a.insert(k, nv.clone());
                }
            }
        }
        for (k, _) in o.iter() {
            if !n.contains_key(k) {
                a.remove(k);
            }
        }
        Ok(())
    }

    fn array(a: &mut Array, o: &Array, n: &Array, path: &str) -> Result<(), String> {
        if a.len() != o.len() {
            return Err(format!("{path}: the file lists {} values where it reads as {}; the file is left as it is", a.len(), o.len()));
        }
        let (ov, nv): (Vec<&Value>, Vec<&Value>) = (o.iter().collect(), n.iter().collect());
        if o.len() == n.len() {
            // The same length: each value is edited where it stands.
            for (k, av) in a.iter_mut().enumerate() {
                if canon_value(ov[k]) != canon_value(nv[k]) {
                    value(av, ov[k], nv[k], &format!("{path}[{k}]"))?;
                }
            }
            return Ok(());
        }
        let (ko, kn): (Vec<String>, Vec<String>) = (o.iter().map(canon_value).collect(), n.iter().map(canon_value).collect());
        let (slots, _) = align(&ko, &kn);
        let old: Vec<Value> = a.iter().cloned().collect();
        let mut out = a.clone();
        out.clear();
        for (j, s) in slots.iter().enumerate() {
            let v = match *s {
                Slot::Stays(i) | Slot::Moved(i) => old[i].clone(),
                Slot::Changed(i) => {
                    let mut v = old[i].clone();
                    value(&mut v, ov[i], nv[j], &format!("{path}[{j}]"))?;
                    v
                }
                Slot::New => {
                    let mut v = nv[j].clone();
                    let like = if j == 0 { old.first() } else { out.get(out.len().wrapping_sub(1)).or(old.last()) };
                    match like {
                        Some(l) if !l.decor().prefix().and_then(|p| p.as_str()).unwrap_or("").contains('#') => {
                            let (p, s) = (
                                l.decor().prefix().and_then(|p| p.as_str()).unwrap_or(if j == 0 { "" } else { " " }).to_string(),
                                l.decor().suffix().and_then(|s| s.as_str()).unwrap_or("").to_string(),
                            );
                            v.decor_mut().set_prefix(p);
                            v.decor_mut().set_suffix(s);
                        }
                        _ => v.decor_mut().clear(),
                    }
                    v
                }
            };
            out.push_formatted(v);
        }
        *a = out;
        Ok(())
    }

    fn tables(a: &mut ArrayOfTables, o: &ArrayOfTables, n: &ArrayOfTables, path: &str) -> Result<(), String> {
        if a.len() != o.len() {
            return Err(format!("{path}: the file lists {} tables where it reads as {}; the file is left as it is", a.len(), o.len()));
        }
        let wrap = |t: &Table| canon(&Item::Table(t.clone()));
        let (ko, kn): (Vec<String>, Vec<String>) = (o.iter().map(wrap).collect(), n.iter().map(wrap).collect());
        let (slots, _) = align(&ko, &kn);
        let old: Vec<Table> = a.iter().cloned().collect();
        let (ot, nt): (Vec<&Table>, Vec<&Table>) = (o.iter().collect(), n.iter().collect());
        let reordered = slots.iter().any(|s| matches!(s, Slot::Moved(_) | Slot::New));
        let mut pool: Vec<isize> = Vec::new();
        if reordered {
            for t in &old {
                positions(t, &mut pool);
            }
            pool.sort_unstable();
        }
        let mut out = ArrayOfTables::new();
        for (j, s) in slots.iter().enumerate() {
            let t = match *s {
                Slot::Stays(i) | Slot::Moved(i) => old[i].clone(),
                Slot::Changed(i) => {
                    let mut t = old[i].clone();
                    table(&mut t, ot[i], nt[j], &format!("{path}[{j}]"))?;
                    t
                }
                Slot::New => match fresh(&Item::Table(nt[j].clone())) {
                    Item::Table(t) => t,
                    _ => unreachable!("fresh keeps the kind"),
                },
            };
            out.push(t);
        }
        if reordered {
            let mut next = pool.into_iter();
            for t in out.iter_mut() {
                place(t, &mut next);
            }
        }
        *a = out;
        Ok(())
    }

    /// The document positions of `t` and every table under it, in the order they are written.
    fn positions(t: &Table, out: &mut Vec<isize>) {
        if !t.is_dotted() {
            if let Some(p) = t.position() {
                out.push(p);
            }
        }
        for (_, v) in t.iter() {
            match v {
                Item::Table(s) => positions(s, out),
                Item::ArrayOfTables(a) => a.iter().for_each(|s| positions(s, out)),
                _ => {}
            }
        }
    }

    /// `t` and every table under it given the next positions, in the order they are written.
    fn place(t: &mut Table, next: &mut impl Iterator<Item = isize>) {
        if !t.is_dotted() {
            t.set_position(next.next());
        }
        for (_, v) in t.iter_mut() {
            match v {
                Item::Table(s) => place(s, next),
                Item::ArrayOfTables(a) => a.iter_mut().for_each(|s| place(s, next)),
                _ => {}
            }
        }
    }
}

mod json_text {
    use super::{align, line_start, only_space_before, Slot};

    /// A JSON value: where its text is, and what it holds.
    struct J {
        at: usize,
        end: usize,
        kind: Kind,
    }

    enum Kind {
        Obj(Vec<Member>),
        Arr(Vec<J>),
        Leaf,
    }

    struct Member {
        key: String,
        at: usize,
        key_end: usize,
        value: J,
    }

    fn line_of(src: &str, at: usize) -> usize {
        src[..at].matches('\n').count() + 1
    }

    struct Reader<'s> {
        src: &'s str,
        i: usize,
        what: &'s str,
    }

    impl Reader<'_> {
        fn err(&self, m: &str) -> String {
            format!("{}, line {}: {m}", self.what, line_of(self.src, self.i.min(self.src.len())))
        }

        fn peek(&self) -> Option<u8> {
            self.src.as_bytes().get(self.i).copied()
        }

        fn space(&mut self) {
            while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
                self.i += 1;
            }
        }

        fn string(&mut self) -> Result<(), String> {
            self.i += 1;
            while let Some(c) = self.peek() {
                self.i += 1;
                match c {
                    b'\\' => self.i += 1,
                    b'"' => return Ok(()),
                    _ => {}
                }
            }
            Err(self.err("a string with no closing quote"))
        }

        fn value(&mut self) -> Result<J, String> {
            self.space();
            let at = self.i;
            match self.peek() {
                Some(b'{') => {
                    self.i += 1;
                    let mut members: Vec<Member> = Vec::new();
                    self.space();
                    if self.peek() == Some(b'}') {
                        self.i += 1;
                    } else {
                        loop {
                            self.space();
                            if self.peek() != Some(b'"') {
                                return Err(self.err("a key that is not a string"));
                            }
                            let key_at = self.i;
                            self.string()?;
                            let key_end = self.i;
                            let key: String =
                                serde_json::from_str(&self.src[key_at..key_end]).map_err(|e| self.err(&e.to_string()))?;
                            if members.iter().any(|m| m.key == key) {
                                return Err(self.err(&format!("the key `{key}` twice in one object")));
                            }
                            self.space();
                            if self.peek() != Some(b':') {
                                return Err(self.err("a key with no colon"));
                            }
                            self.i += 1;
                            let value = self.value()?;
                            members.push(Member { key, at: key_at, key_end, value });
                            self.space();
                            match self.peek() {
                                Some(b',') => self.i += 1,
                                Some(b'}') => {
                                    self.i += 1;
                                    break;
                                }
                                _ => return Err(self.err("an object member followed by neither `,` nor `}`")),
                            }
                        }
                    }
                    Ok(J { at, end: self.i, kind: Kind::Obj(members) })
                }
                Some(b'[') => {
                    self.i += 1;
                    let mut items = Vec::new();
                    self.space();
                    if self.peek() == Some(b']') {
                        self.i += 1;
                    } else {
                        loop {
                            items.push(self.value()?);
                            self.space();
                            match self.peek() {
                                Some(b',') => self.i += 1,
                                Some(b']') => {
                                    self.i += 1;
                                    break;
                                }
                                _ => return Err(self.err("a list item followed by neither `,` nor `]`")),
                            }
                        }
                    }
                    Ok(J { at, end: self.i, kind: Kind::Arr(items) })
                }
                Some(b'"') => {
                    self.string()?;
                    Ok(J { at, end: self.i, kind: Kind::Leaf })
                }
                Some(_) => {
                    while !matches!(self.peek(), None | Some(b',' | b'}' | b']' | b' ' | b'\t' | b'\n' | b'\r')) {
                        self.i += 1;
                    }
                    if self.i == at {
                        return Err(self.err("a value expected"));
                    }
                    Ok(J { at, end: self.i, kind: Kind::Leaf })
                }
                None => Err(self.err("a value expected")),
            }
        }
    }

    fn read(src: &str, what: &str) -> Result<J, String> {
        let mut r = Reader { src, i: 0, what };
        let j = r.value()?;
        r.space();
        if r.i != src.len() {
            return Err(r.err("text after the value"));
        }
        Ok(j)
    }

    /// Where each member or item of a container starts and ends.
    fn spans(j: &J) -> Vec<(usize, usize)> {
        match &j.kind {
            Kind::Obj(ms) => ms.iter().map(|m| (m.at, m.value.end)).collect(),
            Kind::Arr(xs) => xs.iter().map(|x| (x.at, x.end)).collect(),
            Kind::Leaf => Vec::new(),
        }
    }

    fn children(j: &J) -> Vec<&J> {
        match &j.kind {
            Kind::Obj(ms) => ms.iter().map(|m| &m.value).collect(),
            Kind::Arr(xs) => xs.iter().collect(),
            Kind::Leaf => Vec::new(),
        }
    }

    /// The spaces and tabs the line `at` is on starts with.
    fn indent_of(src: &str, at: usize) -> &str {
        let rest = &src[line_start(src, at)..];
        &rest[..rest.len() - rest.trim_start_matches([' ', '\t']).len()]
    }

    /// The indent step of the first container in `j` whose members stand on their own lines.
    fn step(src: &str, j: &J) -> Option<String> {
        if let Some(&(f, _)) = spans(j).first() {
            if only_space_before(src, f) {
                if let Some(u) = indent_of(src, f).strip_prefix(indent_of(src, j.at)).filter(|u| !u.is_empty()) {
                    return Some(u.to_string());
                }
            }
        }
        children(j).into_iter().find_map(|c| step(src, c))
    }

    /// How a value in the file is placed: the indent of its line, and whether it stands on a line of
    /// its own (a container written there takes several lines).
    #[derive(Clone)]
    struct Ctx {
        indent: String,
        lines: bool,
    }

    struct Layout {
        lead: String,
        between: String,
        trail: String,
        child: Ctx,
    }

    impl Layout {
        fn wrap(&self, open: char, close: char, parts: &[String]) -> String {
            if parts.is_empty() {
                return format!("{open}{close}");
            }
            format!("{open}{}{}{}{close}", self.lead, parts.join(&format!(",{}", self.between)), self.trail)
        }
    }

    /// The file's text of `j` with each `(start, end)` replaced.
    fn splice(src: &str, j: &J, mut reps: Vec<(usize, usize, String)>) -> String {
        reps.sort_by_key(|r| r.0);
        let mut out = String::with_capacity(j.end - j.at);
        let mut cur = j.at;
        for (s, e, t) in reps {
            out.push_str(&src[cur..s]);
            out.push_str(&t);
            cur = e;
        }
        out.push_str(&src[cur..j.end]);
        out
    }

    fn join(path: &str, key: &str) -> String {
        if path.is_empty() { key.to_string() } else { format!("{path}.{key}") }
    }

    pub(super) fn rewrite(a: &str, o: &str, n: &str) -> Result<String, String> {
        let aj = read(a, "the file")?;
        let oj = read(o, "qm's text of the file")?;
        let nj = read(n, "qm's text of the edit")?;
        let step_n = step(n, &nj);
        // A file all on one line takes qm's step for a list it now writes on lines of their own.
        let step_a = step(a, &aj).or_else(|| step_n.clone());
        let e = Editor { a, o, n, step_a, step_n };
        let cx = Ctx { indent: indent_of(a, aj.at).to_string(), lines: true };
        let body = e.render(&aj, &oj, &nj, &cx, "")?;
        Ok(format!("{}{body}{}", &a[..aj.at], &a[aj.end..]))
    }

    struct Editor<'s> {
        a: &'s str,
        o: &'s str,
        n: &'s str,
        step_a: Option<String>,
        step_n: Option<String>,
    }

    impl Editor<'_> {
        fn ta(&self, j: &J) -> &str {
            &self.a[j.at..j.end]
        }

        fn to(&self, j: &J) -> &str {
            &self.o[j.at..j.end]
        }

        fn tn(&self, j: &J) -> &str {
            &self.n[j.at..j.end]
        }

        /// The placement of the file's value at `value`, in a member or item starting at `start`.
        fn step_a(&self) -> Result<&str, String> {
            self.step_a.as_deref().ok_or_else(|| "neither the file nor qm's text sets an indent step".to_string())
        }

        fn ctx_at(&self, start: usize, value: usize) -> Ctx {
            Ctx { indent: indent_of(self.a, value).to_string(), lines: only_space_before(self.a, start) }
        }

        /// The file's text of `a`, edited to read as qm's `n` where qm's `o` is what it reads as.
        fn render(&self, a: &J, o: &J, n: &J, cx: &Ctx, path: &str) -> Result<String, String> {
            if self.to(o) == self.tn(n) {
                return Ok(self.ta(a).to_string());
            }
            match (&a.kind, &o.kind, &n.kind) {
                (Kind::Obj(_), Kind::Obj(_), Kind::Obj(_)) => self.obj(a, o, n, cx, path),
                (Kind::Arr(_), Kind::Arr(_), Kind::Arr(_)) => self.arr(a, o, n, cx, path),
                _ => self.fresh(n, cx),
            }
        }

        fn obj(&self, a: &J, o: &J, n: &J, cx: &Ctx, path: &str) -> Result<String, String> {
            let (Kind::Obj(am), Kind::Obj(om), Kind::Obj(nm)) = (&a.kind, &o.kind, &n.kind) else {
                unreachable!("render passes three objects")
            };
            let find = |ms: &[Member], k: &str| ms.iter().position(|m| m.key == k);
            let changed = |m: &Member| -> Result<Option<String>, String> {
                match (find(om, &m.key), find(nm, &m.key)) {
                    (Some(oi), Some(ni)) if self.to(&om[oi].value) != self.tn(&nm[ni].value) => {
                        let cx = self.ctx_at(m.at, m.value.at);
                        self.render(&m.value, &om[oi].value, &nm[ni].value, &cx, &join(path, &m.key)).map(Some)
                    }
                    (None, Some(ni)) => self.fresh(&nm[ni].value, &self.ctx_at(m.at, m.value.at)).map(Some),
                    _ => Ok(None),
                }
            };
            // A member qm writes that the file leaves out (a default) stays left out while unchanged.
            let adds = |x: &Member| {
                find(am, &x.key).is_none() && find(om, &x.key).is_none_or(|oi| self.to(&om[oi].value) != self.tn(&x.value))
            };
            let drops = |m: &Member| find(nm, &m.key).is_none() && find(am, &m.key).is_some();
            if !om.iter().any(drops) && !nm.iter().any(adds) {
                let mut reps = Vec::new();
                for m in am {
                    if let Some(t) = changed(m)? {
                        reps.push((m.value.at, m.value.end, t));
                    }
                }
                return Ok(splice(self.a, a, reps));
            }
            // The file's members that stay, in the file's order; each new one after the member qm
            // writes before it.
            let mut order: Vec<(&str, Option<&Member>, Option<&Member>)> = am
                .iter()
                .filter(|m| find(om, &m.key).is_none() || find(nm, &m.key).is_some())
                .map(|m| (m.key.as_str(), Some(m), None))
                .collect();
            for (j, x) in nm.iter().enumerate() {
                if adds(x) {
                    let at = nm[..j]
                        .iter()
                        .rev()
                        .find_map(|p| order.iter().position(|(k, ..)| *k == p.key))
                        .map_or(0, |i| i + 1);
                    order.insert(at, (x.key.as_str(), None, Some(x)));
                }
            }
            let lay = self.layout(a, n, cx)?;
            let colon = am.first().map_or(": ".to_string(), |m| self.a[m.key_end..m.value.at].to_string());
            let mut parts = Vec::new();
            for (_, kept, new) in order {
                parts.push(match (kept, new) {
                    (Some(m), _) => {
                        let value = changed(m)?.unwrap_or_else(|| self.ta(&m.value).to_string());
                        format!("{}{value}", &self.a[m.at..m.value.at])
                    }
                    (None, Some(x)) => format!("{}{colon}{}", &self.n[x.at..x.key_end], self.fresh(&x.value, &lay.child)?),
                    (None, None) => unreachable!("every member is the file's or qm's"),
                });
            }
            Ok(lay.wrap('{', '}', &parts))
        }

        fn arr(&self, a: &J, o: &J, n: &J, cx: &Ctx, path: &str) -> Result<String, String> {
            let (Kind::Arr(ai), Kind::Arr(oi), Kind::Arr(ni)) = (&a.kind, &o.kind, &n.kind) else {
                unreachable!("render passes three lists")
            };
            if ai.len() != oi.len() {
                return Err(format!(
                    "{}: the file lists {} items where it reads as {}; the file is left as it is",
                    if path.is_empty() { "the list" } else { path },
                    ai.len(),
                    oi.len()
                ));
            }
            let leaves = |xs: &[J]| xs.iter().all(|x| matches!(x.kind, Kind::Leaf));
            let (slots, gone) = if ai.len() == ni.len() && leaves(oi) && leaves(ni) {
                // A list of plain values the same length: each value is edited where it stands.
                let slots = (0..ni.len()).map(|k| if self.to(&oi[k]) == self.tn(&ni[k]) { Slot::Stays(k) } else { Slot::Changed(k) });
                (slots.collect(), Vec::new())
            } else {
                let ko: Vec<&str> = oi.iter().map(|x| self.to(x)).collect();
                let kn: Vec<&str> = ni.iter().map(|x| self.tn(x)).collect();
                align(&ko, &kn)
            };
            let item = |i: usize, j: usize| {
                let cx = self.ctx_at(ai[i].at, ai[i].at);
                self.render(&ai[i], &oi[i], &ni[j], &cx, &format!("{path}[{j}]"))
            };
            let reshaped = !gone.is_empty() || slots.iter().any(|s| matches!(s, Slot::Moved(_) | Slot::New));
            if !reshaped {
                let mut reps = Vec::new();
                for (j, s) in slots.iter().enumerate() {
                    if let Slot::Changed(i) = *s {
                        reps.push((ai[i].at, ai[i].end, item(i, j)?));
                    }
                }
                return Ok(splice(self.a, a, reps));
            }
            let lay = self.layout(a, n, cx)?;
            let mut parts = Vec::new();
            for (j, s) in slots.iter().enumerate() {
                parts.push(match *s {
                    Slot::Stays(i) | Slot::Moved(i) => self.ta(&ai[i]).to_string(),
                    Slot::Changed(i) => item(i, j)?,
                    Slot::New => self.fresh(&ni[j], &lay.child)?,
                });
            }
            Ok(lay.wrap('[', ']', &parts))
        }

        /// How the file lays out the container `a`; for an empty one, how qm lays out `n`, at the
        /// file's indent step.
        fn layout(&self, a: &J, n: &J, cx: &Ctx) -> Result<Layout, String> {
            let sa = spans(a);
            if let (Some(&(f, _)), Some(&(_, l))) = (sa.first(), sa.last()) {
                let lead = self.a[a.at + 1..f].to_string();
                let between = match sa.get(1) {
                    Some(&(s, _)) => {
                        let gap = &self.a[sa[0].1..s];
                        gap[gap.find(',').map_or(0, |c| c + 1)..].to_string()
                    }
                    None => lead.clone(),
                };
                let trail = self.a[l..a.end - 1].to_string();
                let child = Ctx { indent: indent_of(self.a, f).to_string(), lines: only_space_before(self.a, f) };
                return Ok(Layout { lead, between, trail, child });
            }
            let own_lines = spans(n).first().is_some_and(|&(f, _)| only_space_before(self.n, f));
            Ok(if own_lines && cx.lines {
                let outer = indent_of(self.a, a.at);
                let inner = format!("{outer}{}", self.step_a()?);
                Layout {
                    lead: format!("\n{inner}"),
                    between: format!("\n{inner}"),
                    trail: format!("\n{outer}"),
                    child: Ctx { indent: inner, lines: true },
                }
            } else {
                Layout { lead: String::new(), between: " ".into(), trail: String::new(), child: Ctx { indent: cx.indent.clone(), lines: false } }
            })
        }

        /// qm's value `n`, placed in the file at `cx`: on several lines at the file's indent step
        /// when it stands on its own line, else on one.
        fn fresh(&self, n: &J, cx: &Ctx) -> Result<String, String> {
            let t = self.tn(n);
            if matches!(n.kind, Kind::Leaf) {
                return Ok(t.to_string());
            }
            if !cx.lines || !t.contains('\n') {
                return Ok(self.flat(n));
            }
            let step_n = self.step_n.as_deref().ok_or("qm's text sets no indent step")?;
            let base = indent_of(self.n, n.at).len();
            let mut out = String::with_capacity(t.len());
            for (k, line) in t.split_inclusive('\n').enumerate() {
                if k == 0 {
                    out.push_str(line);
                    continue;
                }
                let body = line.trim_start_matches([' ', '\t']);
                let depth = (line.len() - body.len())
                    .checked_sub(base)
                    .filter(|d| d % step_n.len() == 0)
                    .ok_or_else(|| format!("qm's text indents `{}` by part of a step", body.trim_end()))?;
                out.push_str(&cx.indent);
                out.push_str(&self.step_a()?.repeat(depth / step_n.len()));
                out.push_str(body);
            }
            Ok(out)
        }

        /// qm's value `n` on one line.
        fn flat(&self, n: &J) -> String {
            match &n.kind {
                Kind::Leaf => self.tn(n).to_string(),
                Kind::Arr(xs) => format!("[{}]", xs.iter().map(|x| self.flat(x)).collect::<Vec<_>>().join(", ")),
                Kind::Obj(ms) => format!(
                    "{{{}}}",
                    ms.iter()
                        .map(|m| format!("{}: {}", &self.n[m.at..m.key_end], self.flat(&m.value)))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    use super::*;

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct Doc {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        note: Option<String>,
        #[serde(default)]
        items: Vec<Thing>,
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct Thing {
        key: String,
        pos: [f32; 3],
        #[serde(default, skip_serializing_if = "Option::is_none")]
        yaw: Option<f32>,
    }

    fn thing(key: &str, x: f32) -> Thing {
        Thing { key: key.into(), pos: [x, 0.0, 0.0], yaw: None }
    }

    fn yaml(text: &str, new: &Doc) -> Result<String, String> {
        rewrite(
            text,
            Format::Yaml,
            new,
            |t| serde_norway::from_str(t).map_err(|e| e.to_string()),
            |d| serde_norway::to_string(d).map_err(|e| e.to_string()),
        )
    }

    fn toml_(text: &str, new: &Doc) -> Result<String, String> {
        rewrite(
            text,
            Format::Toml,
            new,
            |t| toml::from_str(t).map_err(|e| e.to_string()),
            |d| toml::to_string(d).map_err(|e| e.to_string()),
        )
    }

    fn json(text: &str, new: &Doc) -> Result<String, String> {
        rewrite(
            text,
            Format::Json,
            new,
            |t| serde_json::from_str(t).map_err(|e| e.to_string()),
            |d| serde_json::to_string_pretty(d).map_err(|e| e.to_string()),
        )
    }

    fn base() -> Doc {
        Doc { name: "barrels".into(), note: None, items: vec![thing("a", 1.0), thing("b", 2.0), thing("c", 3.0)] }
    }

    /// Every kind of edit the helper places, on one value.
    fn edits() -> Vec<(&'static str, Doc)> {
        let b = base();
        let mut out = Vec::new();
        let mut d = b.clone();
        d.name = "crates".into();
        out.push(("scalar", d));
        let mut d = b.clone();
        d.items[1].pos[1] = -13.18;
        out.push(("nested scalar", d));
        let mut d = b.clone();
        d.items[2].yaw = Some(90.0);
        out.push(("added key", d));
        let mut d = b.clone();
        d.note = Some("by hand".into());
        out.push(("added top key", d));
        let mut d = b.clone();
        d.items.push(thing("d", 4.0));
        out.push(("appended item", d));
        let mut d = b.clone();
        d.items.insert(0, thing("z", 0.0));
        out.push(("prepended item", d));
        let mut d = b.clone();
        d.items.remove(1);
        out.push(("removed item", d));
        let mut d = b.clone();
        d.items.remove(2);
        out.push(("removed last item", d));
        let mut d = b.clone();
        let m = d.items.remove(2);
        d.items.insert(0, m);
        out.push(("moved item", d));
        let mut d = b.clone();
        d.items.clear();
        out.push(("emptied list", d));
        let mut d = b;
        d.items.reverse();
        d.items[0].key = "cc".into();
        out.push(("reversed and edited", d));
        out
    }

    #[test]
    fn a_file_qm_wrote_is_rewritten_to_exactly_what_qm_writes_of_the_edit() {
        let w = |d: &Doc, f: Format| match f {
            Format::Yaml => serde_norway::to_string(d).unwrap(),
            Format::Toml => toml::to_string(d).unwrap(),
            Format::Json => serde_json::to_string_pretty(d).unwrap(),
        };
        for f in [Format::Yaml, Format::Toml, Format::Json] {
            for (what, new) in edits() {
                let text = w(&base(), f);
                let got = match f {
                    Format::Yaml => yaml(&text, &new),
                    Format::Toml => toml_(&text, &new),
                    Format::Json => json(&text, &new),
                }
                .unwrap_or_else(|e| panic!("{f:?} {what}: {e}"));
                assert_eq!(got, w(&new, f), "{f:?} {what}");
            }
            let mut empty = base();
            empty.items.clear();
            let mut one = base();
            one.items.truncate(1);
            for full in [base(), one] {
                let got = match f {
                    Format::Yaml => yaml(&w(&empty, f), &full),
                    Format::Toml => toml_(&w(&empty, f), &full),
                    Format::Json => json(&w(&empty, f), &full),
                }
                .unwrap_or_else(|e| panic!("{f:?} filled list of {}: {e}", full.items.len()));
                assert_eq!(got, w(&full, f), "{f:?} filled list of {}", full.items.len());
            }
        }
    }

    const HAND: &str = "\
# barrels by the dock
name: barrels   # what the layer is

items:
  - key: a
    pos: [1, 0, 0]

  # the second barrel
  - key: b
    pos: [2, 0, 0]   # by the crane

  - key: c
    pos: [3, 0, 0]
";

    #[test]
    fn a_hand_written_file_keeps_its_comments_spacing_and_spelling_around_an_edit() {
        let mut d = base();
        d.items[1].pos[1] = -13.18;
        d.name = "crates".into();
        let got = yaml(HAND, &d).unwrap();
        assert_eq!(got, HAND.replace("name: barrels", "name: crates").replace("[2, 0, 0]", "[2, -13.18, 0]"));
    }

    #[test]
    fn a_removed_item_takes_the_comment_above_it_and_one_blank_line() {
        let mut d = base();
        d.items.remove(1);
        let got = yaml(HAND, &d).unwrap();
        assert_eq!(
            got,
            "# barrels by the dock\nname: barrels   # what the layer is\n\nitems:\n  - key: a\n    pos: [1, 0, 0]\n\n  - key: c\n    pos: [3, 0, 0]\n"
        );
    }

    #[test]
    fn a_moved_item_takes_the_comment_above_it() {
        let mut d = base();
        let b = d.items.remove(1);
        d.items.push(b);
        let got = yaml(HAND, &d).unwrap();
        assert_eq!(
            got,
            "# barrels by the dock\nname: barrels   # what the layer is\n\nitems:\n  - key: a\n    pos: [1, 0, 0]\n\n  - key: c\n    pos: [3, 0, 0]\n\n  # the second barrel\n  - key: b\n    pos: [2, 0, 0]   # by the crane\n"
        );
    }

    #[test]
    fn an_added_item_is_indented_as_the_file_indents_and_spaced_as_its_list_is() {
        let mut d = base();
        d.items.insert(1, thing("n", 9.0));
        d.items[3].yaw = Some(45.0);
        let got = yaml(HAND, &d).unwrap();
        assert_eq!(
            got,
            "# barrels by the dock\nname: barrels   # what the layer is\n\nitems:\n  - key: a\n    pos: [1, 0, 0]\n\n  - key: n\n    pos:\n    - 9.0\n    - 0.0\n    - 0.0\n\n  # the second barrel\n  - key: b\n    pos: [2, 0, 0]   # by the crane\n\n  - key: c\n    pos: [3, 0, 0]\n    yaw: 45.0\n"
        );
        let packed = HAND.replace("\n\n  ", "\n  ");
        let got = yaml(&packed, &d).unwrap();
        assert!(got.contains("    pos: [1, 0, 0]\n  - key: n\n"), "{got}");
    }

    #[test]
    fn a_hand_written_toml_file_keeps_its_comments_around_an_edit_a_move_and_a_removal() {
        let hand = "\
# barrels by the dock
name = \"barrels\"   # what the layer is

[[items]]
key = \"a\"
pos = [1, 0, 0]

# the second barrel
[[items]]
key = \"b\"
pos = [2, 0, 0]   # by the crane

[[items]]
key = \"c\"
pos = [3, 0, 0]
";
        let mut d = base();
        d.items[1].pos[1] = -13.18;
        let got = toml_(hand, &d).unwrap();
        assert_eq!(got, hand.replace("[2, 0, 0]", "[2, -13.18, 0]"));

        let mut d = base();
        let b = d.items.remove(1);
        d.items.push(b);
        let got = toml_(hand, &d).unwrap();
        assert_eq!(
            got,
            "# barrels by the dock\nname = \"barrels\"   # what the layer is\n\n[[items]]\nkey = \"a\"\npos = [1, 0, 0]\n\n[[items]]\nkey = \"c\"\npos = [3, 0, 0]\n\n# the second barrel\n[[items]]\nkey = \"b\"\npos = [2, 0, 0]   # by the crane\n"
        );

        let mut d = base();
        d.items.remove(1);
        let got = toml_(hand, &d).unwrap();
        assert_eq!(
            got,
            "# barrels by the dock\nname = \"barrels\"   # what the layer is\n\n[[items]]\nkey = \"a\"\npos = [1, 0, 0]\n\n[[items]]\nkey = \"c\"\npos = [3, 0, 0]\n"
        );
    }

    /// Four-space indent, `items` before `name`, whole numbers spelled without `.0`, `pos` on one
    /// line: what qm writes differs from this on nearly every line.
    const HAND_JSON: &str = "{\n    \"items\": [\n        {\"key\": \"a\", \"pos\": [1, 0, 0]},\n        {\n            \"key\": \"b\",\n            \"pos\": [2, 0, 0]\n        },\n        {\"key\": \"c\", \"pos\": [3, 0, 0]}\n    ],\n    \"name\": \"barrels\"\n}\n";

    #[test]
    fn a_hand_written_json_file_keeps_its_layout_order_and_spelling_around_an_edit() {
        let mut d = base();
        d.items[1].pos[1] = -13.18;
        d.name = "crates".into();
        let got = json(HAND_JSON, &d).unwrap();
        assert_eq!(got, HAND_JSON.replace("\"barrels\"", "\"crates\"").replace("[2, 0, 0]", "[2, -13.18, 0]"));
        assert_eq!(json(HAND_JSON, &base()).unwrap(), HAND_JSON);
    }

    #[test]
    fn a_hand_written_json_list_takes_a_removal_a_move_and_an_addition_in_its_own_layout() {
        let a = "{\"key\": \"a\", \"pos\": [1, 0, 0]}";
        let b = "{\n            \"key\": \"b\",\n            \"pos\": [2, 0, 0]\n        }";
        let c = "{\"key\": \"c\", \"pos\": [3, 0, 0]}";
        let list = |items: &[&str]| {
            format!("{{\n    \"items\": [\n        {}\n    ],\n    \"name\": \"barrels\"\n}}\n", items.join(",\n        "))
        };
        assert_eq!(list(&[a, b, c]), HAND_JSON);

        let mut d = base();
        d.items.remove(1);
        assert_eq!(json(HAND_JSON, &d).unwrap(), list(&[a, c]));

        let mut d = base();
        let m = d.items.remove(2);
        d.items.insert(0, m);
        assert_eq!(json(HAND_JSON, &d).unwrap(), list(&[c, a, b]));

        let mut d = base();
        d.items.push(thing("d", 4.0));
        let added = "{\n            \"key\": \"d\",\n            \"pos\": [\n                4.0,\n                0.0,\n                0.0\n            ]\n        }";
        assert_eq!(json(HAND_JSON, &d).unwrap(), list(&[a, b, c, added]));

        let mut d = base();
        d.note = Some("by hand".into());
        let got = json(HAND_JSON, &d).unwrap();
        assert!(got.ends_with("    \"name\": \"barrels\",\n    \"note\": \"by hand\"\n}\n"), "{got}");
    }

    #[test]
    fn a_json_file_on_one_line_stays_on_one_line_and_a_tab_indented_one_takes_tabs() {
        let one = serde_json::to_string(&base()).unwrap();
        let mut d = base();
        d.items[2].yaw = Some(90.0);
        let got = json(&one, &d).unwrap();
        assert_eq!(got, one.replace("\"pos\":[3.0,0.0,0.0]}", "\"pos\":[3.0,0.0,0.0],\"yaw\":90.0}"));

        let tabs = "{\n\t\"name\": \"barrels\",\n\t\"items\": []\n}";
        let mut d = base();
        d.items.truncate(1);
        let got = json(tabs, &d).unwrap();
        assert_eq!(got, "{\n\t\"name\": \"barrels\",\n\t\"items\": [\n\t\t{\n\t\t\t\"key\": \"a\",\n\t\t\t\"pos\": [\n\t\t\t\t1.0,\n\t\t\t\t0.0,\n\t\t\t\t0.0\n\t\t\t]\n\t\t}\n\t]\n}");
    }

    /// `note` repeats `name` through an alias, so the edit to `name` cannot leave `note` as it was.
    #[test]
    fn an_edit_that_does_not_read_back_as_itself_is_refused() {
        let text = "name: &n barrels\nnote: *n\n";
        let mut d: Doc = serde_norway::from_str(text).unwrap();
        d.name = "crates".into();
        let e = yaml(text, &d).unwrap_err();
        assert!(e.contains("does not read back") || e.contains("reads back otherwise"), "{e}");
    }

    /// An empty flow list under a key deep in the file, filled: the items go on the lines below the
    /// key, at the depth qm's writer puts them.
    #[test]
    fn an_empty_list_deep_in_the_file_takes_its_first_items() {
        let text = "# a machine\nnodes:\n- name: a\n  states:\n  - name: Init\n    enter:\n    - 1\n    exit: []\nslots:\n- 0\n";
        let mut v: serde_norway::Value = serde_norway::from_str(text).unwrap();
        v["nodes"][0]["states"][0]["exit"] = serde_norway::from_str("[3]").unwrap();
        let got = rewrite(
            text,
            Format::Yaml,
            &v,
            |t| serde_norway::from_str(t).map_err(|e| e.to_string()),
            |d| serde_norway::to_string(d).map_err(|e| e.to_string()),
        );
        assert_eq!(got.as_deref(), Ok(text.replace("    exit: []\n", "    exit:\n    - 3\n").as_str()));
    }

    #[test]
    fn an_unchanged_value_leaves_the_file_byte_for_byte() {
        assert_eq!(yaml(HAND, &base()).unwrap(), HAND);
    }

    #[test]
    fn align_pairs_moves_edits_additions_and_removals() {
        let (s, gone) = align(&["a", "b", "c", "d"], &["d", "a", "x", "c", "y"]);
        assert_eq!(s, [Slot::Moved(3), Slot::Stays(0), Slot::Changed(1), Slot::Stays(2), Slot::New]);
        assert_eq!(gone, [3]);
    }
}
