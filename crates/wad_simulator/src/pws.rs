//! External `.pws` streaming audio validation.
//!
//! A PC `.pws` is **headerless streaming blob storage** — it has no self-describing
//! layout. Each streaming wavebank clip (codec `0x04`) addresses its audio by
//! `(data_offset, data_size)` into the `.pws`; the format (codec / channels /
//! sample-rate) lives in the wavebank clip record, NOT the `.pws`. That is a
//! space-saving "assumed codec / no per-blob header" design — verified on retail
//! `music.pws` / `ambience.pws` / `vo_stream.english.pws`, which contain no
//! `RIFF` / `OggS` / IMA-version markers and are not block-aligned to any IMA
//! stride.
//!
//! Consequently there is nothing to parse standalone (the old IMA-block
//! `detect_pws_payload_layout` model false-positived as "unrecognized layout").
//! This audit only confirms each `.pws` is present and non-empty; the real
//! per-clip reference check (`data_offset + data_size` fits the file) is done by
//! the wavebank consumer in `validate_streaming_pws_present`.

use std::path::Path;

#[derive(Debug, Default)]
pub struct PwsAudit {
    pub files_found: usize,
    pub files_validated: usize,
    pub issues: Vec<String>,
}

pub fn audit_audios_dir(dir: &Path) -> PwsAudit {
    let mut audit = PwsAudit::default();
    let Ok(entries) = std::fs::read_dir(dir) else {
        audit.issues.push(format!("cannot read {}", dir.display()));
        return audit;
    };

    for ent in entries.flatten() {
        let path = ent.path();
        if path.extension().and_then(|e| e.to_str()) != Some("pws") {
            continue;
        }
        audit.files_found += 1;
        // A `.pws` is an opaque streaming blob store; validity is per-clip (checked
        // against the wavebank). Standalone, only presence + non-empty is
        // meaningful. Use metadata so we don't read a multi-hundred-MB file.
        match std::fs::metadata(&path) {
            Ok(m) if m.len() > 0 => audit.files_validated += 1,
            Ok(_) => audit
                .issues
                .push(format!("{}: empty .pws", path.display())),
            Err(_) => audit
                .issues
                .push(format!("{}: read failed", path.display())),
        }
    }
    audit
}
