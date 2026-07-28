//! The builder: lint-gate → lower → assemble one overlay WAD → emit, verified by hash.
//!
//! Two mandates shape this module.
//!
//! **Gated on EXIT CODE, never a printed count.** [`build`] returns `Err(BuildError::Blocked)` when
//! the linter finds anything at `Error` or above; a caller cannot accidentally ship by ignoring
//! stdout.
//!
//! **Verified by hash** (`verify-artifacts-by-hash-not-size-mtime`). Every artifact carries its
//! sha256 in the [`Placement`] record, which is also what makes a file drop reversible — a WAD
//! overlay is undone by deleting one file, but an `.asi` in the game folder is not backable-out
//! unless something wrote down what was placed and where.
//!
//! ## Lowering status
//!
//! `replace_texture`, `add_model`, `add_outfit` and `patch_lua` all lower here.
//!
//! `add_outfit` is the composed case and the reason [`Lowered`] carries an `Option` of each half: a
//! Data half (the model, injected into a hero-rigged donor) that lowers immediately, and a Script
//! half that cannot, because Lua links across the installed set rather than per Shipment. Linking a
//! Shipment's own mutations here keeps its overlay valid **standalone**; the cross-Shipment relink
//! is deploy's job, and skipping it is what lets one script mod overwrite another's Lua.
//!
//! `edit_state_machine`, `native_hook` and `raw` still return `Unsupported` — with the reason,
//! rather than being quietly skipped, because a dropped contribution produces a WAD that looks fine
//! and does nothing.

use crate::discover::LoadedShipment;
use crate::game::{GameStack, Platform};
use crate::link::{self, ScriptMutation};
use crate::lint::{self, Diagnostic};
use crate::manifest::Contribution;
use crate::names::NameTable;
use mercs2_formats::donor;
use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::mesh_import;
use mercs2_formats::model_inject::inject_static_into_donor_block;
use mercs2_formats::patch_wad::{build_patch_wad_multi, AsetEntry, PatchBlock, FFCS_CERT_BLOB};
use mercs2_formats::scripts_block::ScriptsBlock;
use mercs2_formats::texture::{build_texture_block, TexFormat, TextureData};
use mercs2_formats::texture_encode::{self, encode_bc1, encode_bc3, mip_chain};
use mercs2_formats::types::{TYPE_ID_MODEL, TYPE_ID_SCRIPT, TYPE_ID_TEXTURE};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Where a built artifact has to end up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Destination {
    /// Inside the Shipment's overlay WAD.
    Overlay,
    /// A file placed in the game folder, relative to it (an `.asi` in the loader's search path).
    GameFolder { relative: String },
}

/// One emitted artifact and its digest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placement {
    pub name: String,
    pub bytes: usize,
    pub sha256: String,
    pub destination: Destination,
}

/// Read the assembled WAD back and run [`crate::lint::artifact_checks`] on it.
///
/// Deliberately re-parses the bytes rather than checking the in-memory `Vec<PatchBlock>` that went
/// in. Those blocks have not been through `build_patch_wad_multi`'s LOD-rung remap, so their rungs
/// are still source-relative — checking them would answer the wrong question. More usefully, this
/// verifies what will actually be on disk, so a serializer bug is in scope too.
///
/// Fails the build on any blocking finding. That is the whole value: both structural bugs this
/// crate has shipped were invisible in the manifest and plain in the bytes.
fn verify_emitted(wad: &[u8]) -> Result<Vec<crate::lint::Diagnostic>, BuildError> {
    let contents =
        mercs2_formats::patch_wad::read_patch_wad(wad).map_err(|m| BuildError::Lower {
            index: 0,
            kind: "verify",
            message: format!("the WAD we just wrote does not read back: {m}"),
        })?;
    let found = crate::lint::artifact_checks(&contents.blocks);
    if crate::lint::blocks_build(&found) {
        return Err(BuildError::Artifact { diagnostics: found });
    }
    Ok(found)
}

#[derive(Debug, Clone)]
pub struct BuildReport {
    /// Everything the linter said, including non-blocking warnings.
    pub diagnostics: Vec<Diagnostic>,
    /// The overlay WAD, when any contribution produced one.
    pub wad: Option<PathBuf>,
    /// Every artifact with its digest — the record deploy/undo consumes.
    pub placements: Vec<Placement>,
    pub log: Vec<String>,
}

#[derive(Debug)]
pub enum BuildError {
    /// The linter found something at `Error` or above.
    Blocked(Vec<Diagnostic>),
    /// A contribution needed the retail WADs and none were configured.
    GameRequired {
        index: usize,
        kind: &'static str,
    },
    /// The configured stack is a console bake and we cannot yet EMIT for one.
    ConsoleOutputUnsupported,
    /// A kind whose lowering is not implemented yet, with the reason.
    Unsupported {
        index: usize,
        kind: &'static str,
        reason: String,
    },
    Lower {
        index: usize,
        kind: &'static str,
        message: String,
    },
    Io {
        path: PathBuf,
        message: String,
    },
    /// The WAD we just assembled failed its own self-check. Always a builder bug rather than an
    /// author one, and fatal on purpose: the whole point of a HANG-class rule is that the game will
    /// not tell anybody what went wrong.
    Artifact {
        diagnostics: Vec<crate::lint::Diagnostic>,
    },
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildError::Blocked(d) => {
                writeln!(f, "build blocked by {} finding(s):", d.len())?;
                for x in d.iter().filter(|x| x.severity >= lint::Severity::Error) {
                    writeln!(f, "  {x}")?;
                }
                Ok(())
            }
            BuildError::GameRequired { index, kind } => write!(
                f,
                "contributions[{index}] ({kind}) needs the retail WADs — configure the game folder \
                 (Workshop Settings, or `qm --game <dir>`). `qm lint` runs without one."
            ),
            BuildError::ConsoleOutputUnsupported => write!(
                f,
                "the configured game stack is a CONSOLE bake (Xbox 360 / PS3, big-endian `SCFF`), \
                 and we cannot emit for one yet. Note this is not merely an endianness flip: \
                 `ucfx_byteswap` converts console → PC only, and the reverse needs GPU texture \
                 RE-tiling, XMA/Xbox-ADPCM audio encoding, big-endian Lua bytecode and Xbox vertex \
                 declarations. Reading a console WAD is supported; writing one is not."
            ),
            BuildError::Unsupported {
                index,
                kind,
                reason,
            } => {
                write!(
                    f,
                    "contributions[{index}] ({kind}) cannot be lowered yet: {reason}"
                )
            }
            BuildError::Lower {
                index,
                kind,
                message,
            } => {
                write!(f, "contributions[{index}] ({kind}): {message}")
            }
            BuildError::Io { path, message } => write!(f, "{}: {message}", path.display()),
            BuildError::Artifact { diagnostics } => {
                write!(f, "the assembled WAD failed its self-check:")?;
                for d in diagnostics {
                    write!(f, "\n  {d}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for BuildError {}

pub fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Which drawing group of the donor hosts the injected geometry.
///
/// ⚠ The manifest has no field for this — the Workshop's UI lets an author pick, but `add_model`
/// carries only name/model/donor. Group 0 is the common case for a simple prop; a donor whose
/// interesting geometry sits in a later group cannot currently be targeted. Recorded rather than
/// papered over: it likely wants a `group:` field, which is a format change.
const DEFAULT_TARGET_GROUP: usize = 0;

/// Every script mutation a Shipment declares, derived from its manifest alone.
///
/// **Hermetic on purpose — no game stack, no lowering.** That is what lets the same function serve
/// two callers that are otherwise very different: [`build`], linking one Shipment so its overlay is
/// valid standalone, and [`link_installed`], linking N Shipments at deploy so none of them
/// overwrites another. If mutations could only be obtained as a side effect of lowering, the deploy
/// path would have to re-run model injection just to find out which scripts are touched.
pub fn script_mutations(
    manifest: &crate::manifest::Manifest,
    root: &Path,
) -> Result<Vec<ScriptMutation>, BuildError> {
    let shipment = manifest.shipment.name.clone();
    let mut out = Vec::new();
    for (index, c) in manifest.contributions.iter().enumerate() {
        match c {
            Contribution::AddOutfit {
                name,
                slug,
                display,
                wearer,
                ..
            } => {
                out.push(ScriptMutation {
                    shipment: shipment.clone(),
                    target: "wifpmcinterior".into(),
                    append: link::outfit_row_append(wearer, slug, name, display),
                });
            }
            Contribution::PatchLua { target, append } => {
                let path = root.join(append);
                let source = std::fs::read_to_string(&path).map_err(|e| BuildError::Lower {
                    index,
                    kind: "patch_lua",
                    message: format!("reading {}: {e}", path.display()),
                })?;
                out.push(ScriptMutation {
                    shipment: shipment.clone(),
                    target: target.clone(),
                    append: source,
                });
            }
            _ => {}
        }
    }
    Ok(out)
}

/// Lower a single contribution into a patch block.
fn lower(
    index: usize,
    contribution: &Contribution,
    root: &Path,
    game: Option<&mut GameStack>,
    log: &mut Vec<String>,
) -> Result<Option<PatchBlock>, BuildError> {
    let kind = contribution.kind();
    match contribution {
        Contribution::ReplaceTexture { target, image } => {
            let Some(game) = game else {
                return Err(BuildError::GameRequired { index, kind });
            };
            let hash = pandemic_hash_m2(target);

            // The target's OWN dimensions and format are the spec: a replacement is same-hash and
            // fully resident, so it must match what the engine already expects to read.
            let existing = game.texture(hash).ok_or_else(|| BuildError::Lower {
                index,
                kind,
                message: format!(
                    "{target:?} (0x{hash:08X}) is not in the configured game stack — check the \
                     spelling; a name that does not exist hashes to a lookup that simply misses"
                ),
            })?;

            let (w, h) = (existing.width as usize, existing.height as usize);
            let rgba = read_png_rgba(&root.join(image)).map_err(|m| BuildError::Lower {
                index,
                kind,
                message: m,
            })?;
            if rgba.width != w || rgba.height != h {
                return Err(BuildError::Lower {
                    index,
                    kind,
                    message: format!(
                        "image is {}x{} but {target} is {w}x{h}; a replacement is SAME-HASH and \
                         fully resident, so it must match the shipped dimensions exactly",
                        rgba.width, rgba.height
                    ),
                });
            }

            // Encode with the shipping encoder (`texture_encode`), not the workbench-preview one.
            let fourcc = *existing.format.fourcc();
            let body = match existing.format {
                TexFormat::Bc1 => {
                    let rgb = drop_alpha(&rgba.pixels);
                    mip_chain(w, h, 3, &rgb, encode_bc1)
                }
                TexFormat::Bc3 => mip_chain(w, h, 4, &rgba.pixels, encode_bc3),
            };
            // `build_texture_block` emits the UCFX container ALREADY WRAPPED in a single-entry
            // block table. A patch block is `[entry table][containers…]`, not a bare container —
            // handing over a raw container makes the loader read the `UCFX` magic as an entry-table
            // field. (Caught by `wad_simulator`, not by any digest check: the WAD hashed fine and
            // was structurally nonsense.)
            let mip_count = texture_encode::mip_count(w, h) as u32;
            let td = TextureData {
                width: existing.width,
                height: existing.height,
                format: existing.format,
                mip0: body[..mip0_len(w, h, existing.format).min(body.len())].to_vec(),
                all_mips: body,
                mip_count,
            };
            let block_bytes = build_texture_block(hash, &td);
            log.push(format!(
                "contributions[{index}] replace_texture {target} 0x{hash:08X} {w}x{h} {} \
                 {mip_count} mips → {} bytes",
                String::from_utf8_lossy(&fourcc),
                block_bytes.len()
            ));

            // Same hash, texture type, PRIMARY. The low 16 bits of `packed_block_ref` MUST be
            // 0xFFFF: that is what `is_primary()` tests, and any other value names a `_P001` LOD
            // block one level finer. A row pointing at a rung that does not exist is the
            // dangling-LOD-rung trap — a 549 GB buffer request and an open-world stream HANG.
            let aset = AsetEntry::new(hash, 0xFFFF_FFFF, 0x0000_FFFF, TYPE_ID_TEXTURE);
            // Path convention matches the proven publish pipeline
            // (`mercs2_workshop::publish`, docs/modernization/workshop_publish_pipeline.md):
            // `blocks\VZ\mod_<hash>.block`. It lands in PTHS; matching the shape that has actually
            // shipped working WADs costs nothing.
            let block = PatchBlock::from_decompressed(
                &block_bytes,
                format!("blocks\\VZ\\mod_{hash:08x}.block"),
                vec![aset],
                None,
            )
            .map_err(|m| BuildError::Lower {
                index,
                kind,
                message: m,
            })?;
            Ok(Some(block))
        }

        Contribution::AddModel {
            name,
            model,
            donor,
            retarget,
        } => {
            let Some(game) = game else {
                return Err(BuildError::GameRequired { index, kind });
            };
            if retarget.is_some() {
                return Err(BuildError::Unsupported {
                    index,
                    kind,
                    reason: "an inline `retarget:` is the CROSS-RIG path and needs char_skin's \
                             palette-relative BLENDINDICES + INFO(56) range table. This lowering is \
                             the RIGID one, which leaves joints empty; hand-authoring global joint \
                             indices for a skinned group is documented as wrong."
                        .into(),
                });
            }
            // Resolved Q2 says `donor` may be omitted and auto-picked. Auto-pick is not written, so
            // this asks rather than guessing — a wrong host silently produces a prop with the wrong
            // rig and materials.
            let Some(donor_name) = donor else {
                return Err(BuildError::Unsupported {
                    index,
                    kind,
                    reason: "donor auto-pick is not implemented yet — name a `donor:` explicitly. \
                             The donor supplies the rig, materials and state machine, so picking \
                             the wrong one fails quietly rather than loudly."
                        .into(),
                });
            };

            let donor_hash = pandemic_hash_m2(donor_name);
            let paths: Vec<PathBuf> = game.paths().iter().map(|p| p.to_path_buf()).collect();
            let donor_blk =
                donor::donor_block(&paths, donor_hash).map_err(|m| BuildError::Lower {
                    index,
                    kind,
                    message: m,
                })?;

            let mesh = mesh_import::external_mesh_from_gltf(&root.join(model)).map_err(|m| {
                BuildError::Lower {
                    index,
                    kind,
                    message: m,
                }
            })?;

            let hash = pandemic_hash_m2(name);
            // Flags mirror the workshop's proven call: auto-fit OFF (the mesh carries its own
            // transform), target the raw rendered group, neutralise the rest.
            let (new_block, stats) = inject_static_into_donor_block(
                &donor_blk,
                &mesh,
                DEFAULT_TARGET_GROUP,
                &[],
                hash,
                false,
                false,
                false,
                false,
                &[DEFAULT_TARGET_GROUP],
                1.0,
                false,
            )
            .map_err(|m| BuildError::Lower {
                index,
                kind,
                message: format!("inject into donor {donor_name}: {m}"),
            })?;

            log.push(format!(
                "contributions[{index}] add_model {name} 0x{hash:08X} ← donor {donor_name} \
                 group {DEFAULT_TARGET_GROUP}: {} verts, {} tris",
                stats.vertex_count, stats.triangle_count
            ));

            let aset = AsetEntry::new(hash, 0xFFFF_FFFF, 0x0000_FFFF, TYPE_ID_MODEL);
            let block = PatchBlock::from_decompressed(
                &new_block,
                format!("blocks\\VZ\\mod_{hash:08x}.block"),
                vec![aset],
                None,
            )
            .map_err(|m| BuildError::Lower {
                index,
                kind,
                message: m,
            })?;
            Ok(Some(block))
        }

        // `add_outfit` is a FIXED composition of add_model + a patch_lua on `_tOutfits`. The Data
        // half lowers here; the Script half is declared and linked later, because Lua is linked
        // across the installed set rather than per Shipment.
        // `slug`/`display` are the SCRIPT half's fields and are consumed by `script_mutations`;
        // only the Data half is lowered here.
        // `display` is the SCRIPT half's field and is consumed by `script_mutations`; only the Data
        // half is lowered here. `slug` is kept for the log line, which is how an author confirms the
        // wardrobe row they expected is the one that was generated.
        Contribution::AddOutfit {
            name,
            slug,
            wearer,
            model,
            donor,
            retarget,
            ..
        } => {
            let Some(game) = game else {
                return Err(BuildError::GameRequired { index, kind });
            };
            if retarget.is_some() {
                return Err(BuildError::Unsupported {
                    index,
                    kind,
                    reason: "an inline `retarget:` is the CROSS-RIG path and needs char_skin's \
                             palette-relative BLENDINDICES + INFO(56) range table; this lowering is \
                             the rigid one."
                        .into(),
                });
            }
            let Some(donor_name) = donor else {
                return Err(BuildError::Unsupported {
                    index,
                    kind,
                    reason:
                        "donor auto-pick is not implemented — name a `donor:` explicitly. For an \
                             outfit the donor must be a hero-rigged host, or the model will not \
                             animate."
                            .into(),
                });
            };

            let donor_hash = pandemic_hash_m2(donor_name);
            let paths: Vec<PathBuf> = game.paths().iter().map(|p| p.to_path_buf()).collect();
            let donor_blk =
                donor::donor_block(&paths, donor_hash).map_err(|m| BuildError::Lower {
                    index,
                    kind,
                    message: m,
                })?;
            let mesh = mesh_import::external_mesh_from_gltf(&root.join(model)).map_err(|m| {
                BuildError::Lower {
                    index,
                    kind,
                    message: m,
                }
            })?;

            let hash = pandemic_hash_m2(name);
            let (new_block, stats) = inject_static_into_donor_block(
                &donor_blk,
                &mesh,
                DEFAULT_TARGET_GROUP,
                &[],
                hash,
                false,
                false,
                false,
                false,
                &[DEFAULT_TARGET_GROUP],
                1.0,
                false,
            )
            .map_err(|m| BuildError::Lower {
                index,
                kind,
                message: format!("inject into donor {donor_name}: {m}"),
            })?;

            let aset = AsetEntry::new(hash, 0xFFFF_FFFF, 0x0000_FFFF, TYPE_ID_MODEL);
            let block = PatchBlock::from_decompressed(
                &new_block,
                format!("blocks\\VZ\\mod_{hash:08x}.block"),
                vec![aset],
                None,
            )
            .map_err(|m| BuildError::Lower {
                index,
                kind,
                message: m,
            })?;

            // The Script half. `Model` is the ASSET name SetOutfit receives; `Name` is the
            // unlock/tracking key; both are distinct from the display string.
            log.push(format!(
                "contributions[{index}] add_outfit {name} 0x{hash:08X} ← donor {donor_name}: \
                 {} verts, {} tris | wardrobe row {wearer}/{slug}",
                stats.vertex_count, stats.triangle_count
            ));
            Ok(Some(block))
        }

        // Contributes no block: its whole effect is a declared mutation, collected by
        // `script_mutations` and realised at link time.
        Contribution::PatchLua { .. } => Ok(None),
        Contribution::EditStateMachine { .. }
        | Contribution::NativeHook { .. }
        | Contribution::Raw { .. } => Err(BuildError::Unsupported {
            index,
            kind,
            reason: "not implemented in this increment".into(),
        }),
    }
}

struct Rgba {
    width: usize,
    height: usize,
    /// Straight RGBA as `f32` in 0..=255, the shape `texture_encode` expects.
    pixels: Vec<f32>,
}

fn read_png_rgba(path: &Path) -> Result<Rgba, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let decoder = png::Decoder::new(file);
    let mut reader = decoder
        .read_info()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let (w, h) = (info.width as usize, info.height as usize);
    let channels = info.color_type.samples();
    if info.bit_depth != png::BitDepth::Eight {
        return Err(format!("{}: only 8-bit PNGs are supported", path.display()));
    }
    let mut pixels = vec![0f32; w * h * 4];
    for i in 0..w * h {
        let src = i * channels;
        let (r, g, b, a) = match channels {
            1 => (buf[src], buf[src], buf[src], 255),
            2 => (buf[src], buf[src], buf[src], buf[src + 1]),
            3 => (buf[src], buf[src + 1], buf[src + 2], 255),
            _ => (buf[src], buf[src + 1], buf[src + 2], buf[src + 3]),
        };
        pixels[i * 4] = r as f32;
        pixels[i * 4 + 1] = g as f32;
        pixels[i * 4 + 2] = b as f32;
        pixels[i * 4 + 3] = a as f32;
    }
    Ok(Rgba {
        width: w,
        height: h,
        pixels,
    })
}

fn drop_alpha(rgba: &[f32]) -> Vec<f32> {
    rgba.chunks_exact(4)
        .flat_map(|p| [p[0], p[1], p[2]])
        .collect()
}

/// Byte length of mip level 0 for a BC-compressed surface: 4x4 blocks, 8 bytes for BC1, 16 for BC3.
fn mip0_len(w: usize, h: usize, format: TexFormat) -> usize {
    let blocks = w.div_ceil(4).max(1) * h.div_ceil(4).max(1);
    blocks
        * match format {
            TexFormat::Bc1 => 8,
            TexFormat::Bc3 => 16,
        }
}

/// Lint, lower, assemble, emit.
///
/// `out_dir` defaults to `<root>/build`. Returns `Err(BuildError::Blocked)` rather than a report
/// when the linter blocks — the gate is the return type, not a field a caller might not read.
pub fn build(
    shipment: &LoadedShipment,
    mut game: Option<&mut GameStack>,
    names: Option<&NameTable>,
    out_dir: Option<&Path>,
    corpus_root: Option<&Path>,
) -> Result<BuildReport, BuildError> {
    let mut log = Vec::new();
    let manifest = &shipment.manifest;

    let mut diagnostics = lint::lint(manifest, Some(&shipment.root), names);
    // Rules that need the retail WADs run only when a stack is configured. They are appended
    // BEFORE the gate so a game-aware Error would still block, even though M0007 is a warning.
    if let Some(g) = game.as_deref() {
        diagnostics.extend(lint::game_checks(manifest, g));
    }
    if lint::blocks_build(&diagnostics) {
        return Err(BuildError::Blocked(diagnostics));
    }
    // Reading a console bake is fine and supported; EMITTING for one is not. Checked before any
    // lowering so the failure names the real reason rather than surfacing as a texture-encode error.
    if game
        .as_deref()
        .is_some_and(|g| g.platform() != Platform::Pc)
    {
        return Err(BuildError::ConsoleOutputUnsupported);
    }
    log.push(format!(
        "lint: {} finding(s), none blocking",
        diagnostics.len()
    ));

    // NOTE: self-conflicts are NOT re-checked here. `lint` already reports them as blocking M0120
    // findings, so a Shipment that claims one target twice never reaches this point. A second check
    // would be a redundant path with different formatting — and the first version of it returned on
    // the first conflict, hiding the rest.

    let mut blocks = Vec::new();
    for (index, c) in manifest.contributions.iter().enumerate() {
        if let Some(b) = lower(index, c, &shipment.root, game.as_deref_mut(), &mut log)? {
            blocks.push(b);
        }
    }
    let mutations = script_mutations(manifest, &shipment.root)?;

    // ── Link the Script layer ──────────────────────────────────────────────────────────────────
    //
    // Linking this Shipment's own mutations produces a `scripts_vz` that is correct for a SOLO
    // install, which is what keeps each overlay valid standalone and verify-by-hash meaningful.
    //
    // ⚠ It is NOT the whole story. When several script-touching Shipments are installed together,
    // the deploy step must re-link all of their mutations into ONE block — otherwise the last WAD
    // mounted wins and the others' Lua disappears, which is the failure the linker exists to
    // prevent. That cross-Shipment relink belongs to deploy (Modkit), and this is deliberately only
    // its single-Shipment case.
    if !mutations.is_empty() {
        let Some(game) = game.as_deref_mut() else {
            return Err(BuildError::GameRequired {
                index: 0,
                kind: "patch_lua",
            });
        };
        let Some(corpus) = corpus_root else {
            return Err(BuildError::Lower {
                index: 0,
                kind: "patch_lua",
                message:
                    "linking Lua needs the decompiled corpus (the base source to append to) — \
                          pass its root; it is vendored at \
                          crates/mercs2_script/corpus/mercs2-luacd/src"
                        .into(),
            });
        };
        let raw = game
            .block_by_path("scripts_vz")
            .ok_or_else(|| BuildError::Lower {
                index: 0,
                kind: "patch_lua",
                message: "no scripts_vz block in the configured game stack".into(),
            })?;
        let mut script_block = ScriptsBlock::parse(&raw).map_err(|m| BuildError::Lower {
            index: 0,
            kind: "patch_lua",
            message: format!("parsing scripts_vz: {m}"),
        })?;
        let linked = link::link_into(&mut script_block, corpus, &mutations).map_err(|e| {
            BuildError::Lower {
                index: 0,
                kind: "patch_lua",
                message: e.to_string(),
            }
        })?;
        for l in &linked {
            log.push(format!(
                "linked {}: {} → {} B source, {} B bytecode, from {:?}",
                l.target,
                l.base_source_bytes,
                l.linked_source_bytes,
                l.bytecode_bytes,
                l.contributors
            ));
        }

        let decompressed = script_block.serialize();
        // Every entry keeps its own ASET row so the block resolves exactly as the base one did.
        let aset: Vec<AsetEntry> = script_block
            .entries
            .iter()
            .map(|e| AsetEntry::new(e.name_hash, 0xFFFF_FFFF, 0x0000_FFFF, TYPE_ID_SCRIPT))
            .collect();
        blocks.push(
            PatchBlock::from_decompressed(
                &decompressed,
                "blocks\\VZ\\scripts_vz_P000_Q3.block".into(),
                aset,
                None,
            )
            .map_err(|m| BuildError::Lower {
                index: 0,
                kind: "patch_lua",
                message: m,
            })?,
        );
    }

    // Mirror the base WAD's CSUM value/meta into the overlay, as the proven publish path does. I
    // previously passed 0/None here, which is a gratuitous divergence from output shapes that are
    // known to load — it costs one header read to match them.
    let csum = match game
        .as_deref()
        .and_then(|g| g.paths().first().map(|p| p.to_path_buf()))
    {
        Some(base) => mercs2_formats::donor::base_csum(&base).map_err(|m| BuildError::Lower {
            index: 0,
            kind: "assemble",
            message: m,
        })?,
        None => (0, None),
    };

    let out_dir = out_dir
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| shipment.root.join("build"));
    std::fs::create_dir_all(&out_dir).map_err(|e| BuildError::Io {
        path: out_dir.clone(),
        message: e.to_string(),
    })?;

    let mut placements = Vec::new();
    let mut wad_path = None;
    if !blocks.is_empty() {
        let wad = build_patch_wad_multi(&blocks, csum.0, csum.1, &FFCS_CERT_BLOB).map_err(|m| {
            BuildError::Lower {
                index: 0,
                kind: "assemble",
                message: m,
            }
        })?;
        // Self-check BEFORE writing: a WAD that would hang the game should not reach the disk at
        // all, where a later step could mistake its presence for success.
        let found = verify_emitted(&wad)?;
        for d in &found {
            log.push(format!("self-check: {d}"));
        }
        diagnostics.extend(found);

        let name = format!("{}.wad", manifest.shipment.name);
        let path = out_dir.join(&name);
        std::fs::write(&path, &wad).map_err(|e| BuildError::Io {
            path: path.clone(),
            message: e.to_string(),
        })?;
        let digest = sha256_hex(&wad);
        std::fs::write(
            out_dir.join(format!("{name}.sha256")),
            format!("{digest}  {name}\n"),
        )
        .map_err(|e| BuildError::Io {
            path: path.clone(),
            message: e.to_string(),
        })?;
        log.push(format!(
            "wrote {name}: {} bytes, sha256 {digest}",
            wad.len()
        ));
        placements.push(Placement {
            name,
            bytes: wad.len(),
            sha256: digest,
            destination: Destination::Overlay,
        });
        wad_path = Some(path);
    }

    // The placement record: what goes where, each with its digest. Deploy/undo consumes this — a
    // file drop cannot be backed out without it.
    let record = placement_json(&placements);
    std::fs::write(out_dir.join("placement.json"), &record).map_err(|e| BuildError::Io {
        path: out_dir.join("placement.json"),
        message: e.to_string(),
    })?;

    let log_text = log.join("\n") + "\n";
    std::fs::write(out_dir.join("build.log"), &log_text).map_err(|e| BuildError::Io {
        path: out_dir.join("build.log"),
        message: e.to_string(),
    })?;

    Ok(BuildReport {
        diagnostics,
        wad: wad_path,
        placements,
        log,
    })
}

/// The filename of the deploy-time link overlay. Named to sort and read as "last".
pub const LINK_WAD_NAME: &str = "zz-quartermaster-link.wad";

/// What a cross-Shipment link produced.
#[derive(Debug, Clone)]
pub struct LinkReport {
    pub wad: Option<PathBuf>,
    pub placements: Vec<Placement>,
    pub linked: Vec<crate::link::LinkedScript>,
    pub log: Vec<String>,
}

/// Link **every installed Shipment's** script mutations into ONE overlay.
///
/// This is the half `build` cannot do. Each Shipment's own overlay carries a `scripts_vz` linked
/// from its own mutations, which makes it valid standalone — but WAD resolution is last-mounted-
/// wins, so installing two of them means one Shipment's Lua silently disappears. That is the exact
/// failure the mutation-not-a-block design exists to prevent, and preventing it requires a step
/// that sees all of them at once.
///
/// The result **must be mounted LAST**, after every Shipment overlay. Because it is built from all
/// their mutations together it is a superset of each, so whichever per-Shipment block it shadows, it
/// shadows with something strictly more complete.
///
/// Returns `wad: None` when no installed Shipment touches a script — there is nothing to shadow, and
/// emitting an overlay that merely restates the base block would be noise a user has to reason about.
pub fn link_installed(
    shipments: &[&LoadedShipment],
    game: &mut GameStack,
    corpus_root: &Path,
    out_dir: &Path,
) -> Result<LinkReport, BuildError> {
    if game.platform() != Platform::Pc {
        return Err(BuildError::ConsoleOutputUnsupported);
    }
    let mut log = Vec::new();
    let mut mutations = Vec::new();
    for s in shipments {
        mutations.extend(script_mutations(&s.manifest, &s.root)?);
    }
    if mutations.is_empty() {
        log.push("no installed Shipment touches a script — nothing to link".into());
        return Ok(LinkReport {
            wad: None,
            placements: Vec::new(),
            linked: Vec::new(),
            log,
        });
    }
    log.push(format!(
        "linking {} mutation(s) from {} Shipment(s)",
        mutations.len(),
        shipments.len()
    ));

    let raw = game
        .block_by_path("scripts_vz")
        .ok_or_else(|| BuildError::Lower {
            index: 0,
            kind: "link",
            message: "no scripts_vz block in the configured game stack".into(),
        })?;
    let mut block = ScriptsBlock::parse(&raw).map_err(|m| BuildError::Lower {
        index: 0,
        kind: "link",
        message: format!("parsing scripts_vz: {m}"),
    })?;
    let linked =
        link::link_into(&mut block, corpus_root, &mutations).map_err(|e| BuildError::Lower {
            index: 0,
            kind: "link",
            message: e.to_string(),
        })?;
    for l in &linked {
        log.push(format!(
            "linked {}: {} → {} B source, {} B bytecode, from {:?}",
            l.target, l.base_source_bytes, l.linked_source_bytes, l.bytecode_bytes, l.contributors
        ));
    }

    let decompressed = block.serialize();
    let aset: Vec<AsetEntry> = block
        .entries
        .iter()
        .map(|e| AsetEntry::new(e.name_hash, 0xFFFF_FFFF, 0x0000_FFFF, TYPE_ID_SCRIPT))
        .collect();
    let patch = PatchBlock::from_decompressed(
        &decompressed,
        "blocks\\VZ\\scripts_vz_P000_Q3.block".into(),
        aset,
        None,
    )
    .map_err(|m| BuildError::Lower {
        index: 0,
        kind: "link",
        message: m,
    })?;

    let csum =
        mercs2_formats::donor::base_csum(game.paths()[0]).map_err(|m| BuildError::Lower {
            index: 0,
            kind: "link",
            message: m,
        })?;
    let wad_bytes =
        build_patch_wad_multi(&[patch], csum.0, csum.1, &FFCS_CERT_BLOB).map_err(|m| {
            BuildError::Lower {
                index: 0,
                kind: "link",
                message: m,
            }
        })?;

    // The link WAD is mounted LAST and so wins outright. It gets the same self-check as any other,
    // and for the same reason: nothing downstream would notice a defect here.
    let self_check = verify_emitted(&wad_bytes)?;

    std::fs::create_dir_all(out_dir).map_err(|e| BuildError::Io {
        path: out_dir.to_path_buf(),
        message: e.to_string(),
    })?;
    let path = out_dir.join(LINK_WAD_NAME);
    std::fs::write(&path, &wad_bytes).map_err(|e| BuildError::Io {
        path: path.clone(),
        message: e.to_string(),
    })?;
    let digest = sha256_hex(&wad_bytes);
    log.push(format!(
        "wrote {LINK_WAD_NAME}: {} bytes, sha256 {digest}",
        wad_bytes.len()
    ));
    for d in &self_check {
        log.push(format!("self-check: {d}"));
    }

    Ok(LinkReport {
        wad: Some(path),
        placements: vec![Placement {
            name: LINK_WAD_NAME.to_string(),
            bytes: wad_bytes.len(),
            sha256: digest,
            destination: Destination::Overlay,
        }],
        linked,
        log,
    })
}

fn placement_json(placements: &[Placement]) -> String {
    let entries: Vec<serde_json::Value> = placements
        .iter()
        .map(|p| {
            let dest = match &p.destination {
                Destination::Overlay => serde_json::json!({ "kind": "overlay" }),
                Destination::GameFolder { relative } => {
                    serde_json::json!({ "kind": "game_folder", "relative": relative })
                }
            };
            serde_json::json!({
                "name": p.name,
                "bytes": p.bytes,
                "sha256": p.sha256,
                "destination": dest,
            })
        })
        .collect();
    serde_json::to_string_pretty(&serde_json::json!({
        "format": 1,
        "placements": entries,
    }))
    .unwrap_or_else(|_| "{}".into())
        + "\n"
}
