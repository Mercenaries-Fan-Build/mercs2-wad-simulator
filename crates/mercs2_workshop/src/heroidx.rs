//! Every model's skeleton, as the set of HIER bone hashes it carries — what the hero picker ranks
//! models by. A clip binds by bone hash (its `trnm` track list) and a FaceFX actor names its face
//! bones by hash, so a model can host either exactly when its skeleton holds those hashes.
//!
//! Built once per archive on a worker thread ([`spawn`]) with `wad::for_each_model_hier` over every
//! source WAD: the stack in load order, a later WAD's copy of a model replacing an earlier one (the
//! game's patch rule), then the side WADs for the models the stack does not hold.

use std::collections::HashMap;

/// The skeletons of every model the archive holds.
#[derive(Default)]
pub struct Skeletons {
    /// Model hash → its HIER bone hashes (empty: the model has no HIER).
    pub bones: HashMap<u32, Vec<u32>>,
    /// Models neither read could find, with why.
    pub unreadable: Vec<(u32, String)>,
}

impl Skeletons {
    /// How many of `want` model `m`'s skeleton holds; `None` when the index has no such model.
    pub fn coverage(&self, m: u32, want: &[u32]) -> Option<usize> {
        let b = self.bones.get(&m)?;
        Some(want.iter().filter(|w| b.contains(w)).count())
    }
}

/// A message from the worker.
pub enum Msg {
    /// Fraction of the blocks read, and the WAD being read.
    Progress(f32, String),
    Done(Result<Skeletons, String>),
}

/// Read every model's skeleton from `stack` (in load order) then `side`, on a worker thread.
pub fn spawn(stack: Vec<String>, side: Vec<String>) -> std::sync::mpsc::Receiver<Msg> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let n_stack = stack.len();
        let all: Vec<String> = stack.into_iter().chain(side).collect();
        let n = all.len().max(1);
        let mut sk = Skeletons::default();
        for (i, path) in all.iter().enumerate() {
            let mut w = match mercs2_engine::wad::open(path) {
                Ok(w) => w,
                Err(e) => {
                    let _ = tx.send(Msg::Done(Err(format!("{path}: {e}"))));
                    return;
                }
            };
            let file = std::path::Path::new(path).file_name().map_or(path.clone(), |f| f.to_string_lossy().into_owned());
            let stacked = i < n_stack;
            mercs2_engine::wad::for_each_model_hier(
                &mut w,
                |done, total| {
                    let f = (i as f32 + done as f32 / total.max(1) as f32) / n as f32;
                    let _ = tx.send(Msg::Progress(f, file.clone()));
                },
                |m, read| {
                    if !stacked && sk.bones.contains_key(&m) {
                        return;
                    }
                    match read {
                        Ok((_, hier)) => {
                            sk.bones.insert(m, hier.iter().map(|h| h.hash).collect());
                        }
                        Err(e) => sk.unreadable.push((m, format!("{file}: {e}"))),
                    }
                },
            );
        }
        let _ = tx.send(Msg::Done(Ok(sk)));
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::Skeletons;

    fn sk() -> Skeletons {
        let mut s = Skeletons::default();
        s.bones.insert(0xA, vec![1, 2, 3, 4]);
        s.bones.insert(0xB, vec![1, 2]);
        s.bones.insert(0xC, vec![9]);
        s.bones.insert(0xD, Vec::new());
        s
    }

    /// A model's coverage is how many of the wanted bones its skeleton holds; a model the index
    /// does not hold has none to give.
    #[test]
    fn coverage_counts_the_wanted_bones_a_skeleton_holds() {
        assert_eq!(sk().coverage(0xA, &[1, 2, 3]), Some(3));
        assert_eq!(sk().coverage(0xC, &[1, 2, 3]), Some(0));
        assert_eq!(sk().coverage(0xB, &[1, 2, 3]), Some(2));
        assert_eq!(sk().coverage(0xD, &[1]), Some(0));
        assert_eq!(sk().coverage(0xE, &[1]), None);
    }
}
