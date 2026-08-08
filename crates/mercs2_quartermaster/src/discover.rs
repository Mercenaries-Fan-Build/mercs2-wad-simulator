//! Finding a Shipment on disk, and checking that everything it references is really there.
//!
//! Everything here is **hermetic** — no game install, no network. That is deliberate: it is exactly
//! the set of checks `qm lint` can run in the template repo's CI, where the retail WADs will never
//! be available (spec: "The game stack is host-provided").
//!
//! Two rules carry most of the weight:
//!
//! * **More than one `manifest.*` is an ERROR, never a silent pick.** Choosing one quietly is the
//!   failure mode the format most wants to avoid — an author edits `manifest.yaml`, the tool reads
//!   `manifest.json`, and nothing they do has any effect.
//! * **A source path may not leave the Shipment root.** The Quartermaster reads and copies these
//!   files; `../../secrets` or `/etc/passwd` must not be expressible. Checked LEXICALLY (so it works
//!   for files that do not exist yet) and again by canonicalization when the file does exist, which
//!   is what catches a symlink pointing outward.

use crate::manifest::{Contribution, Manifest};
use crate::Format;
use std::path::{Component, Path, PathBuf};

/// Manifest filename stem. The extension picks the serialization.
pub const MANIFEST_STEM: &str = "manifest";

/// Extensions probed, in the order they are reported when several exist.
pub const MANIFEST_EXTENSIONS: [&str; 4] = ["yaml", "yml", "json", "toml"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscoverError {
    NotADirectory(PathBuf),
    NoManifest {
        root: PathBuf,
    },
    /// Several `manifest.*` in one root. Loud by design.
    Ambiguous {
        root: PathBuf,
        found: Vec<PathBuf>,
    },
    Io {
        path: PathBuf,
        message: String,
    },
}

impl std::fmt::Display for DiscoverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiscoverError::NotADirectory(p) => {
                write!(f, "{} is not a directory", p.display())
            }
            DiscoverError::NoManifest { root } => write!(
                f,
                "no manifest in {} — expected one of {}",
                root.display(),
                MANIFEST_EXTENSIONS
                    .iter()
                    .map(|e| format!("{MANIFEST_STEM}.{e}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            DiscoverError::Ambiguous { root, found } => {
                let names: Vec<_> = found
                    .iter()
                    .filter_map(|p| p.file_name())
                    .map(|n| n.to_string_lossy().into_owned())
                    .collect();
                write!(
                    f,
                    "{} contains {} manifests ({}) — refusing to guess which one is authoritative. \
                     Keep exactly one.",
                    root.display(),
                    found.len(),
                    names.join(", ")
                )
            }
            DiscoverError::Io { path, message } => {
                write!(f, "reading {}: {message}", path.display())
            }
        }
    }
}

impl std::error::Error for DiscoverError {}

/// Locate the single manifest in a Shipment root.
pub fn find_manifest(root: &Path) -> Result<(PathBuf, Format), DiscoverError> {
    if !root.is_dir() {
        return Err(DiscoverError::NotADirectory(root.to_path_buf()));
    }
    let mut found = Vec::new();
    for ext in MANIFEST_EXTENSIONS {
        let candidate = root.join(format!("{MANIFEST_STEM}.{ext}"));
        if candidate.is_file() {
            found.push(candidate);
        }
    }
    match found.len() {
        0 => Err(DiscoverError::NoManifest {
            root: root.to_path_buf(),
        }),
        1 => {
            let path = found.pop().expect("len checked");
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .expect("candidate was built with an extension");
            let format = Format::from_extension(ext).expect("extension came from the probe list");
            Ok((path, format))
        }
        _ => Err(DiscoverError::Ambiguous {
            root: root.to_path_buf(),
            found,
        }),
    }
}

/// A Shipment read from disk: its root plus the parsed manifest.
///
/// Named to avoid collision with [`crate::manifest::Shipment`], which is the `shipment:` metadata
/// BLOCK inside the manifest. This is the whole package.
#[derive(Debug, Clone)]
pub struct LoadedShipment {
    pub root: PathBuf,
    pub manifest_path: PathBuf,
    pub format: Format,
    pub manifest: Manifest,
}

#[derive(Debug)]
pub enum OpenError {
    Discover(DiscoverError),
    Read(crate::ReadError),
}

impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpenError::Discover(e) => write!(f, "{e}"),
            OpenError::Read(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for OpenError {}

/// Find, read, parse and validate the Shipment rooted at `root`. No game install required.
pub fn open(root: &Path) -> Result<LoadedShipment, OpenError> {
    let (manifest_path, format) = find_manifest(root).map_err(OpenError::Discover)?;
    let text = std::fs::read_to_string(&manifest_path).map_err(|e| {
        OpenError::Discover(DiscoverError::Io {
            path: manifest_path.clone(),
            message: e.to_string(),
        })
    })?;
    let manifest = crate::from_str(&text, format).map_err(OpenError::Read)?;
    Ok(LoadedShipment {
        root: root.to_path_buf(),
        manifest_path,
        format,
        manifest,
    })
}

// ---------------------------------------------------------------------------
// Source paths
// ---------------------------------------------------------------------------

/// One `src/` reference from a contribution, with enough context to name it back to the author.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRef<'a> {
    /// Index into `manifest.contributions` — the author counts from the top of the list.
    pub index: usize,
    pub kind: &'static str,
    /// The manifest field the path came from (`model`, `image`, `payload`, …).
    pub field: &'static str,
    pub path: &'a Path,
}

/// Something wrong with a referenced path. Severity is the LINTER's call, not this module's — these
/// are stated as facts so the caller can decide.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceIssue {
    /// Absolute paths are never portable between machines.
    Absolute {
        index: usize,
        kind: &'static str,
        field: &'static str,
        path: PathBuf,
    },
    /// Resolves outside the Shipment root — via `..`, or via a symlink when the file exists.
    EscapesRoot {
        index: usize,
        kind: &'static str,
        field: &'static str,
        path: PathBuf,
    },
    /// Referenced but not present.
    Missing {
        index: usize,
        kind: &'static str,
        field: &'static str,
        path: PathBuf,
    },
    /// Present and contained, but not under `src/`. The spec says sources live under `src/`; this is
    /// reported separately because it is a convention, not a safety property.
    OutsideSrc {
        index: usize,
        kind: &'static str,
        field: &'static str,
        path: PathBuf,
    },
}

impl SourceIssue {
    /// The problem, WITHOUT the `contributions[i]` prefix.
    ///
    /// [`Display`](std::fmt::Display) is self-contained because a `SourceIssue` may be reported on
    /// its own. A [`Diagnostic`](crate::lint::Diagnostic) already prints the contribution index from
    /// its `at` field, so folding the full Display into one produced the location twice:
    ///
    /// ```text
    /// [M0110] error: contributions[0]: contributions[0] (replace_texture) field `image`: …
    /// ```
    pub fn detail(&self) -> String {
        let (what, _, kind, field, path) = self.parts();
        format!("({kind}) field `{field}`: {} {what}", path.display())
    }

    fn parts(&self) -> (&'static str, usize, &'static str, &'static str, &Path) {
        let (what, index, kind, field, path) = match self {
            SourceIssue::Absolute {
                index,
                kind,
                field,
                path,
            } => (
                "is an absolute path (not portable)",
                index,
                kind,
                field,
                path,
            ),
            SourceIssue::EscapesRoot {
                index,
                kind,
                field,
                path,
            } => (
                "resolves outside the Shipment root",
                index,
                kind,
                field,
                path,
            ),
            SourceIssue::Missing {
                index,
                kind,
                field,
                path,
            } => ("does not exist", index, kind, field, path),
            SourceIssue::OutsideSrc {
                index,
                kind,
                field,
                path,
            } => ("is not under src/", index, kind, field, path),
        };
        (what, *index, kind, field, path.as_path())
    }
}

impl std::fmt::Display for SourceIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (_, index, _, _, _) = self.parts();
        write!(f, "contributions[{index}] {}", self.detail())
    }
}

impl Contribution {
    /// Every `src/` path this contribution references, with its field name.
    pub fn sources(&self) -> Vec<(&'static str, &Path)> {
        let mut out: Vec<(&'static str, &Path)> = Vec::new();
        match self {
            Contribution::AddOutfit {
                model, textures, ..
            } => {
                // Only an INJECTED outfit ships a model file; an existing-model outfit has none.
                if let Some(m) = model {
                    out.push(("model", m.as_path()));
                }
                if let Some(p) = &textures.diffuse {
                    out.push(("textures.diffuse", p.as_path()));
                }
                if let Some(p) = &textures.normal {
                    out.push(("textures.normal", p.as_path()));
                }
                if let Some(p) = &textures.specular {
                    out.push(("textures.specular", p.as_path()));
                }
            }
            Contribution::AddModel { model, textures, .. } => {
                out.push(("model", model.as_path()));
                if let Some(p) = &textures.diffuse {
                    out.push(("textures.diffuse", p.as_path()));
                }
                if let Some(p) = &textures.normal {
                    out.push(("textures.normal", p.as_path()));
                }
                if let Some(p) = &textures.specular {
                    out.push(("textures.specular", p.as_path()));
                }
            }
            Contribution::AddTexture { image, .. } => out.push(("image", image.as_path())),
            Contribution::AddSound { bank, .. } => out.push(("bank", bank.as_path())),
            Contribution::AddMovie { movie, .. } => out.push(("movie", movie.as_path())),
            Contribution::AddUi { movie, .. } => out.push(("movie", movie.as_path())),
            Contribution::ReplaceTexture { image, .. } => out.push(("image", image.as_path())),
            Contribution::PatchLua { append, .. } => out.push(("append", append.as_path())),
            Contribution::EditStateMachine { states, .. } => out.push(("states", states.as_path())),
            Contribution::EditWorld { edits, .. } => out.push(("edits", edits.as_path())),
            // No `src/` artifact: `layer` / `replaces` are layer NAMES the loader marks at runtime,
            // not files to pack.
            Contribution::ActivateLayer { .. } => {}
            Contribution::EditStringDb { strings, .. } => out.push(("strings", strings.as_path())),
            // The translation file, checked by the same source rules as every other `src/` path. `base`
            // is a table NAME, not a file, so there is nothing else to pack.
            Contribution::AddLanguage { strings, .. } => out.push(("strings", strings.as_path())),
            Contribution::NativeHook { plugin, .. } => {
                if let Some(p) = plugin {
                    out.push(("plugin", p.as_path()));
                }
            }
            // `place_file` is the kind with the most to lose here, because its source path is also
            // the OUTPUT filename: the file lands in the game folder under whatever it is called in
            // `src/`. Routing it through the same checks as every other source is what makes
            // `../../etc/passwd`, `/etc/passwd` and a symlink pointing out of the Shipment M0111
            // errors rather than a bespoke rule that could drift from this one.
            Contribution::PlaceFile { file, .. } => out.push(("file", file.as_path())),
            Contribution::Raw { payload, .. } => out.push(("payload", payload.as_path())),
        }
        out
    }
}

/// Every source path in the manifest, in contribution order.
pub fn source_refs(manifest: &Manifest) -> Vec<SourceRef<'_>> {
    manifest
        .contributions
        .iter()
        .enumerate()
        .flat_map(|(index, c)| {
            let kind = c.kind();
            c.sources().into_iter().map(move |(field, path)| SourceRef {
                index,
                kind,
                field,
                path,
            })
        })
        .collect()
}

/// Lexically normalize a relative path, resolving `.` and `..` without touching the filesystem.
/// Returns `None` if it would climb above its own root — which is exactly the escape we reject.
///
/// Filesystem-free on purpose: a source file may legitimately not exist yet when linting, and we
/// still want to reject `../../..` rather than merely reporting it missing.
fn normalize_within(rel: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for comp in rel.components() {
        match comp {
            Component::CurDir => {}
            Component::Normal(c) => out.push(c),
            Component::ParentDir => {
                if !out.pop() {
                    return None;
                }
            }
            // A prefix or root component means it was not relative to begin with.
            Component::Prefix(_) | Component::RootDir => return None,
        }
    }
    Some(out)
}

/// Check every referenced path against the Shipment root. Returns all issues found, in order —
/// never short-circuits, because an author fixing their manifest wants the whole list.
pub fn check_sources(manifest: &Manifest, root: &Path) -> Vec<SourceIssue> {
    let mut issues = Vec::new();
    for SourceRef {
        index,
        kind,
        field,
        path,
    } in source_refs(manifest)
    {
        // `is_absolute()` alone is HOST-dependent, and a manifest is not: `/etc/hosts` is absolute
        // on Unix but not on Windows, where an absolute path needs a `C:\` prefix or a UNC root.
        // Judged by `is_absolute()` on Windows it fell through to the escapes-root arm — still
        // rejected, but reported as the wrong thing, and the diagnostic is the whole product here.
        // `has_root()` catches the leading-separator form on every platform, so a Shipment authored
        // on Linux is diagnosed identically when it is checked on Windows.
        if path.is_absolute() || path.has_root() {
            issues.push(SourceIssue::Absolute {
                index,
                kind,
                field,
                path: path.to_path_buf(),
            });
            continue;
        }
        let Some(normalized) = normalize_within(path) else {
            issues.push(SourceIssue::EscapesRoot {
                index,
                kind,
                field,
                path: path.to_path_buf(),
            });
            continue;
        };
        let full = root.join(&normalized);
        if !full.exists() {
            issues.push(SourceIssue::Missing {
                index,
                kind,
                field,
                path: path.to_path_buf(),
            });
            continue;
        }
        // The lexical check above cannot see a symlink that points outward; this can.
        if let (Ok(canonical_root), Ok(canonical_full)) = (root.canonicalize(), full.canonicalize())
        {
            if !canonical_full.starts_with(&canonical_root) {
                issues.push(SourceIssue::EscapesRoot {
                    index,
                    kind,
                    field,
                    path: path.to_path_buf(),
                });
                continue;
            }
        }
        if !normalized.starts_with("src") {
            issues.push(SourceIssue::OutsideSrc {
                index,
                kind,
                field,
                path: path.to_path_buf(),
            });
        }
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_resolves_dot_and_parent() {
        assert_eq!(
            normalize_within(Path::new("src/./a.png")),
            Some(PathBuf::from("src/a.png"))
        );
        assert_eq!(
            normalize_within(Path::new("src/x/../a.png")),
            Some(PathBuf::from("src/a.png"))
        );
    }

    #[test]
    fn normalize_rejects_climbing_out() {
        assert_eq!(normalize_within(Path::new("../a.png")), None);
        assert_eq!(normalize_within(Path::new("src/../../a.png")), None);
    }
}
