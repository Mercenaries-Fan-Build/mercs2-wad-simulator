//! The install's `Mercs2.ini`, read as retail reads it: `GetPrivateProfileIntA` on `[Render]` keys
//! (`FUN_00753280`), the file sitting beside the `data` folder that holds `vz.wad`.

use std::path::PathBuf;

/// `Mercs2.ini` of the install whose `data` folder holds `vz_wad`.
pub fn path(vz_wad: &str) -> Result<PathBuf, String> {
    let install = std::path::Path::new(vz_wad)
        .parent()
        .and_then(|data| data.parent())
        .ok_or_else(|| format!("{vz_wad} is not in an install's data folder"))?;
    Ok(install.join("Mercs2.ini"))
}

/// An integer key, or `None` when the file or the key is not there (where `GetPrivateProfileIntA`
/// hands back its caller's default).
pub fn int(vz_wad: &str, section: &str, key: &str) -> Result<Option<i64>, String> {
    let ini = path(vz_wad)?;
    let text = match std::fs::read(&ini) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{}: {e}", ini.display())),
    };
    Ok(find(&String::from_utf8_lossy(&text), section, key)
        .map(|value| value.parse::<i64>().map_err(|e| format!("{} [{section}] {key} {value:?}: {e}", ini.display())))
        .transpose()?)
}

/// A string key, or `None` when the file or the key is not there (`GetPrivateProfileStringA`'s
/// default case).
pub fn string(vz_wad: &str, section: &str, key: &str) -> Result<Option<String>, String> {
    let ini = path(vz_wad)?;
    match std::fs::read(&ini) {
        Ok(bytes) => Ok(find(&String::from_utf8_lossy(&bytes), section, key).map(str::to_string)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {e}", ini.display())),
    }
}

fn find<'a>(text: &'a str, section: &str, key: &str) -> Option<&'a str> {
    let mut current = "";
    for line in text.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            current = name;
        } else if let Some((k, value)) = line.split_once('=') {
            if current.eq_ignore_ascii_case(section) && k.trim().eq_ignore_ascii_case(key) {
                return Some(value.trim());
            }
        }
    }
    None
}

/// Write `(section, key, value)` entries into the install's `Mercs2.ini` as
/// `WritePrivateProfileStringA` does, which is how retail saves its options: a key already there gets
/// the new value in place; a new key goes at the end of its section; a new section goes at the end of
/// the file. Every other line, and the file's line ending, stays as it was.
pub fn write(vz_wad: &str, entries: &[(&str, &str, String)]) -> Result<(), String> {
    let ini = path(vz_wad)?;
    let text = match std::fs::read(&ini) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("{}: {e}", ini.display())),
    };
    let text = entries.iter().fold(text, |text, (section, key, value)| set(&text, section, key, value));
    std::fs::write(&ini, text).map_err(|e| format!("write {}: {e}", ini.display()))
}

/// `text` with `[section] key=value` set.
fn set(text: &str, section: &str, key: &str, value: &str) -> String {
    let newline = if text.contains("\r\n") || text.is_empty() { "\r\n" } else { "\n" };
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let header = |line: &str| line.trim().strip_prefix('[').and_then(|l| l.strip_suffix(']')).map(str::to_string);
    let start = lines.iter().position(|l| header(l).is_some_and(|h| h.eq_ignore_ascii_case(section)));
    let entry = format!("{key}={value}");
    match start {
        Some(start) => {
            let end = lines[start + 1..].iter().position(|l| header(l).is_some()).map_or(lines.len(), |i| start + 1 + i);
            let found = (start + 1..end).find(|&i| lines[i].split_once('=').is_some_and(|(k, _)| k.trim().eq_ignore_ascii_case(key)));
            match found {
                Some(i) => lines[i] = entry,
                None => {
                    // After the section's last non-blank line.
                    let at = (start + 1..end).rev().find(|&i| !lines[i].trim().is_empty()).map_or(start + 1, |i| i + 1);
                    lines.insert(at, entry);
                }
            }
        }
        None => {
            lines.push(format!("[{section}]"));
            lines.push(entry);
        }
    }
    let mut out = lines.join(newline);
    out.push_str(newline);
    out
}

/// `[Render] RefreshRate`, the rate retail presents at; 60 (`0x3C`) when absent, the default
/// `FUN_00753280` passes.
pub fn refresh_rate(vz_wad: &str) -> Result<u32, String> {
    let rate = int(vz_wad, "Render", "RefreshRate")?.unwrap_or(0x3C);
    u32::try_from(rate).ok().filter(|r| *r > 0).ok_or_else(|| format!("[Render] RefreshRate {rate} is not a rate"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_found_by_section_ignoring_case() {
        let text = "[Render]\nScrW=1600\nrefreshrate = 60\n[Joystick]\nRefreshRate=5\n";
        assert_eq!(find(text, "render", "RefreshRate"), Some("60"));
        assert_eq!(find(text, "Joystick", "refreshrate"), Some("5"));
        assert_eq!(find(text, "Render", "FirstRun"), None);
    }

    #[test]
    fn a_write_replaces_in_place_appends_to_its_section_or_adds_the_section() {
        let text = "[Render]\r\nGamma=50\r\nScrW=1600\r\n[Audio]\r\nSFXVolume=28\r\n";
        assert_eq!(set(text, "render", "Gamma", "70"), "[Render]\r\nGamma=70\r\nScrW=1600\r\n[Audio]\r\nSFXVolume=28\r\n");
        assert_eq!(set(text, "Render", "FSAA", "0"), "[Render]\r\nGamma=50\r\nScrW=1600\r\nFSAA=0\r\n[Audio]\r\nSFXVolume=28\r\n");
        assert_eq!(set(text, "Mouse", "InvertY", "1"), "[Render]\r\nGamma=50\r\nScrW=1600\r\n[Audio]\r\nSFXVolume=28\r\n[Mouse]\r\nInvertY=1\r\n");
    }
}
