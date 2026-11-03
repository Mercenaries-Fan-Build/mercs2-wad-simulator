//! The movie widgets: a Bink player per widget the scripts start (`MovieWidget:SetMovie` + `Play`).
//!
//! Retail (`0x00709790` → `0x0070A230`, once per rendered frame) decodes a frame when Bink's own
//! clock says it is due, copies it to an `A8R8G8B8` texture, and draws it stretched over the widget's
//! corrected rect as it stood when `Play` was called (`0x00621AB0`), 1.25 times larger for a 1024-wide
//! movie (`0x0070A41A`). A finished movie closes unless it loops, when it starts over.

use std::collections::BTreeMap;
use std::path::PathBuf;

use mercs2_bink::{Movie, VideoDecoder};
use mercs2_ui::WidgetTree;

struct Playing {
    file: String,
    bytes: Vec<u8>,
    decoder: VideoDecoder,
    /// The next frame to decode, and the movie's frame count and rate.
    next: u32,
    frames: u32,
    fps: f64,
    /// Seconds since this run of the movie started.
    clock: f64,
    rect: [f32; 4],
    looping: bool,
    /// The last decoded frame, as RGBA, and whether it changed since it was last drawn.
    rgba: Vec<u8>,
    size: [u32; 2],
    fresh: bool,
}

impl Playing {
    fn open(dir: &std::path::Path, file: &str, rect: [f32; 4], looping: bool) -> Result<Playing, String> {
        let path = dir.join(format!("{file}.bik"));
        let bytes = std::fs::read(&path).map_err(|e| format!("movie {}: {e}", path.display()))?;
        let movie = Movie::parse(&bytes).map_err(|e| format!("movie {}: {e}", path.display()))?;
        let (num, den) = movie.fps;
        if num == 0 || den == 0 {
            return Err(format!("movie {} has a frame rate of {num}/{den}", path.display()));
        }
        let decoder = VideoDecoder::new(&movie)?;
        let (frames, size) = (movie.frame_count, [movie.width, movie.height]);
        Ok(Playing { file: file.to_string(), decoder, next: 0, frames, fps: num as f64 / den as f64, clock: 0.0, rect, looping, rgba: Vec::new(), size, fresh: false, bytes })
    }

    /// Start over from the first frame, on the bytes already read.
    fn rewind(&mut self) -> Result<(), String> {
        let movie = Movie::parse(&self.bytes)?;
        self.decoder = VideoDecoder::new(&movie)?;
        self.next = 0;
        self.clock = 0.0;
        Ok(())
    }

    /// Decode the next frame if it is due. One a frame at most, as `FUN_0070A230` does (`BinkWait`,
    /// then one `BinkDoFrame` + `BinkNextFrame`): a movie the renderer cannot keep up with plays
    /// slower, rather than each late frame owing the next several decodes.
    fn advance(&mut self, dt: f32) -> Result<(), String> {
        self.clock += dt as f64;
        let due = ((self.clock * self.fps) as u32 + 1).min(self.frames);
        if self.next < due {
            let movie = Movie::parse(&self.bytes)?;
            let frame = self.decoder.decode(&movie, self.next).map_err(|e| format!("movie {} frame {}: {e}", self.file, self.next))?;
            self.rgba = mercs2_bink::rgb::to_rgba(frame);
            self.fresh = true;
            self.next += 1;
        }
        Ok(())
    }
}

/// One movie to draw: where, and its latest frame.
pub struct MovieDraw<'a> {
    pub widget: u64,
    pub rect: [f32; 4],
    pub size: [u32; 2],
    pub rgba: &'a [u8],
    /// The frame changed since the last draw and needs uploading.
    pub fresh: bool,
}

/// Movie updates before a skip is honoured: the 30.0 at `0x00B977D0` that `FUN_00709740` holds the
/// count at `0x011761AC` to.
const UPDATES_BEFORE_SKIP: u32 = 30;

/// What a held skip button does on one frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Skip {
    /// No movie is playing.
    Nothing,
    /// The movie can't be skipped yet, or on this run; `first` on its first refusal since it started.
    Refused { first: bool },
    Skipped,
}

/// Every playing movie widget.
pub struct MovieStage {
    dir: PathBuf,
    playing: BTreeMap<u64, Playing>,
    /// Movie updates since a movie last started (`0x011761AC`: `FUN_0070A230` counts, `FUN_00709CB0`
    /// zeroes it at a start).
    updates: u32,
    /// No movie can be skipped this session (`0x0117619A`, set on a first run).
    unskippable: bool,
    /// The latest movie's refusal was logged; a skip button is held for many frames.
    refusal_logged: bool,
}

impl MovieStage {
    /// Play movies out of `dir`, the install's `data\Movies` (`"%sMovies\%s.bik"`, `0x00BD1EB0`), and
    /// take the movie widgets of `tree` over from its stand-in. On a `first_run` nothing skips.
    pub fn new(dir: PathBuf, tree: &mut WidgetTree, first_run: bool) -> MovieStage {
        tree.attach_movie_player();
        MovieStage { dir, playing: BTreeMap::new(), updates: 0, unskippable: first_run, refusal_logged: false }
    }

    /// Skip every movie (`FUN_00709740`), unless this is a first run or the latest one started under
    /// 30 updates ago. A skipped movie stops and its widget's end callback fires, as when it ends.
    /// Called every frame a skip button is held, so with no movie playing it does nothing, and a
    /// refusal is logged once a movie.
    pub fn skip(&mut self, tree: &mut WidgetTree) {
        match self.verdict(!self.playing.is_empty()) {
            Skip::Nothing | Skip::Refused { first: false } => {}
            Skip::Refused { first: true } => println!("MOVIE Skip(): This movie can't be skipped"),
            Skip::Skipped => {
                println!("MOVIE Skip(): Skipping movie");
                for (id, _) in std::mem::take(&mut self.playing) {
                    tree.movie_finished(id);
                }
            }
        }
    }

    /// What a skip does with `any` movie playing.
    fn verdict(&mut self, any: bool) -> Skip {
        if !any {
            Skip::Nothing
        } else if self.unskippable || self.updates < UPDATES_BEFORE_SKIP {
            Skip::Refused { first: !std::mem::replace(&mut self.refusal_logged, true) }
        } else {
            Skip::Skipped
        }
    }

    /// A movie started (`FUN_00709CB0` zeroes the update count).
    fn started(&mut self) {
        self.updates = 0;
        self.refusal_logged = false;
    }

    /// Start what the scripts started, drop what they stopped, and decode what came due. `screen` is
    /// the surface in pixels, for the rect a movie is placed at when it starts.
    pub fn advance(&mut self, tree: &mut WidgetTree, dt: f32, screen: [f32; 2]) -> Result<(), String> {
        // Per widget: its file, whether it plays, and whether it was stopped (`StopMovie` sets the end
        // latch; `PauseMovie` only clears `playing`).
        let wanted: Vec<(u64, String, bool, bool, bool)> =
            tree.movies().map(|(id, m)| (id, m.file.clone(), m.playing, m.end_fired, m.looping)).collect();
        // A movie whose widget went, changed file or was stopped is closed; a paused one keeps its place.
        self.playing.retain(|id, p| wanted.iter().any(|(w, file, playing, stopped, _)| w == id && *file == p.file && (*playing || !*stopped)));
        for (id, file, playing, _, looping) in &wanted {
            if *playing && !file.is_empty() && !self.playing.contains_key(id) {
                // Aspect 1.0: retail's `[[0xDFC2F8]+0x2BD0]` until a script sets a screen ratio, and no
                // binding here sets one.
                let rect = tree.corrected_rect(*id, screen, 1.0).ok_or("a movie widget vanished")?;
                self.playing.insert(*id, Playing::open(&self.dir, file, rect, *looping)?);
                self.started();
            }
        }
        let running: Vec<u64> = wanted.iter().filter(|w| w.2).map(|w| w.0).filter(|id| self.playing.contains_key(id)).collect();
        if !running.is_empty() {
            self.updates = self.updates.saturating_add(1);
        }
        for id in running {
            let p = self.playing.get_mut(&id).ok_or("a movie vanished mid-frame")?;
            p.advance(dt)?;
            if let Some(m) = tree.get_mut(id).and_then(|w| w.movie.as_mut()) {
                m.frame = p.next;
            }
            if p.next >= p.frames {
                if p.looping {
                    p.rewind()?;
                } else {
                    self.playing.remove(&id);
                    tree.movie_finished(id);
                }
            }
        }
        Ok(())
    }

    /// The movies on screen, each with the z its widget draws at. Called after [`Self::advance`],
    /// which closes the movie of a widget that went.
    pub fn draws(&mut self, tree: &WidgetTree) -> Vec<(i32, MovieDraw<'_>)> {
        let mut out = Vec::new();
        for (id, p) in self.playing.iter_mut() {
            let w = tree.get(*id).expect("advance closes the movie of a widget that went");
            if p.rgba.is_empty() || !tree.shown(*id) {
                continue;
            }
            let scale = if p.size[0] == 1024 { 1.25 } else { 1.0 };
            let [x1, y1, x2, y2] = p.rect;
            let (x, y) = (x1.trunc(), y1.trunc());
            let rect = [x - 0.5, y - 0.5, x - 0.5 + (x2 - x1).trunc() * scale, y - 0.5 + (y2 - y1).trunc() * scale];
            out.push((w.z, MovieDraw { widget: *id, rect, size: p.size, rgba: &p.rgba, fresh: std::mem::take(&mut p.fresh) }));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stage(first_run: bool) -> MovieStage {
        MovieStage { dir: PathBuf::new(), playing: BTreeMap::new(), updates: 0, unskippable: first_run, refusal_logged: false }
    }

    /// Held on a screen with no movie (the menus), the skip does nothing on every frame.
    #[test]
    fn a_held_skip_with_no_movie_does_nothing() {
        let mut s = stage(false);
        s.updates = UPDATES_BEFORE_SKIP;
        for _ in 0..60 {
            assert_eq!(s.verdict(false), Skip::Nothing);
        }
    }

    /// Held from a movie's start, the refusal is logged on the first frame only, and the skip goes
    /// through once the movie has had its 30 updates.
    #[test]
    fn a_held_skip_is_refused_once_then_skips() {
        let mut s = stage(false);
        s.started();
        assert_eq!(s.verdict(true), Skip::Refused { first: true });
        for _ in 1..UPDATES_BEFORE_SKIP {
            assert_eq!(s.verdict(true), Skip::Refused { first: false });
        }
        s.updates = UPDATES_BEFORE_SKIP;
        assert_eq!(s.verdict(true), Skip::Skipped);
    }

    /// On a first run nothing skips; each movie logs its refusal once.
    #[test]
    fn a_first_run_logs_each_movies_refusal_once() {
        let mut s = stage(true);
        for _ in 0..2 {
            s.started();
            s.updates = UPDATES_BEFORE_SKIP;
            assert_eq!(s.verdict(true), Skip::Refused { first: true });
            assert_eq!(s.verdict(true), Skip::Refused { first: false });
        }
    }
}
