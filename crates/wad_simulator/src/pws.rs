//! External `.pws` streaming audio validation.

use std::path::Path;

use crate::audio::ima::{validate_ima_payload, DecodeError, MONO_BLOCK_SIZE, STEREO_BLOCK_SIZE};

const PC_PWS_VERSION: u16 = 1;
pub const PC_PWS_HEADER_SIZE: usize = 4;

#[derive(Debug, Default)]
pub struct PwsAudit {
    pub files_found: usize,
    pub files_validated: usize,
    pub issues: Vec<String>,
}

/// Returns (payload_offset, channels, block_size).
fn detect_pws_payload_layout(data: &[u8]) -> (usize, usize, u8) {
    if data.len() >= 4 {
        let ver = u16::from_le_bytes([data[2], data[3]]);
        if ver == PC_PWS_VERSION && data.len() >= 4 + MONO_BLOCK_SIZE {
            let payload = &data[4..];
            if payload.len() % MONO_BLOCK_SIZE == 0 {
                return (4, 1, MONO_BLOCK_SIZE as u8);
            }
            if payload.len() % STEREO_BLOCK_SIZE == 0 {
                return (4, 2, STEREO_BLOCK_SIZE as u8);
            }
        }
    }
    if data.len() >= MONO_BLOCK_SIZE && data.len() % MONO_BLOCK_SIZE == 0 {
        return (0, 1, MONO_BLOCK_SIZE as u8);
    }
    if data.len() >= STEREO_BLOCK_SIZE && data.len() % STEREO_BLOCK_SIZE == 0 {
        return (0, 2, STEREO_BLOCK_SIZE as u8);
    }
    (0, 1, 0)
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
        let Ok(data) = std::fs::read(&path) else {
            audit
                .issues
                .push(format!("{}: read failed", path.display()));
            continue;
        };

        let (payload_off, channels, block_size) = detect_pws_payload_layout(&data);
        if block_size == 0 {
            audit.issues.push(format!(
                "{}: unrecognized layout (size={})",
                path.display(),
                data.len()
            ));
            continue;
        }

        let payload = &data[payload_off..];
        match validate_ima_payload(payload, channels as u8) {
            Ok(_) => audit.files_validated += 1,
            Err(DecodeError::BlockTooSmall) => {
                audit.issues.push(format!("{}: IMA block too small", path.display()));
            }
            Err(DecodeError::Empty) => {
                audit.issues.push(format!("{}: empty payload", path.display()));
            }
            Err(DecodeError::StepIndexOutOfRange) => unreachable!(),
        }
    }
    audit
}
