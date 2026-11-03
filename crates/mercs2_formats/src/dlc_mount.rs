//! Where a level added to the PC game keeps the files its console scripts open from the DLC's own folder.
//!
//! The console builds return the installed DLC's folder from `Net.DLCGetMountPoint` (PS3 `0x005370B8`:
//! `<gamedata>/USRDIR/DLC/<dir>`, one subfolder of `DLC` per installed DLC), and the DLC's sound bootstrap
//! opens each stream at `<mount>\<Sound.GetAudioDir()>\<NAME>.PWS` (`dlctest01_soundbootstrap.lua:188`).
//! PC v1.1 registers no `Net.DLCGetMountPoint`. On PC a level's folder is `DLC\<level>` under the game
//! folder: the Quartermaster installs the level's streams there, and the Shipment that ships the level
//! registers the binding to return it. One definition, so the two cannot drift.

/// The folder under the game folder that holds one folder per level.
pub const DLC_DIR: &str = "DLC";

/// The game's audio directory, as `Sound.GetAudioDir` returns it on PC (`.\Data\Audios`), relative to a
/// mount point.
pub const AUDIO_DIR: &str = "Data/Audios";

/// What `Net.DLCGetMountPoint` returns for `level`: its folder, relative to the game folder, in the
/// backslash form the game's Lua joins paths with.
pub fn mount_point(level: &str) -> String {
    format!("{DLC_DIR}\\{level}")
}

/// Where `level`'s stream `file` installs, relative to the game folder.
pub fn stream_relative(level: &str, file: &str) -> String {
    format!("{DLC_DIR}/{level}/{AUDIO_DIR}/{file}")
}

/// The level whose folder [`mount_point`] names, from the names of the folders under [`DLC_DIR`]: the one
/// folder there. None, or more than one, is an error naming what was found, since the console scripts
/// ask for the mount point without saying which DLC they belong to.
pub fn single_level(folders: &[String]) -> Result<&str, String> {
    match folders {
        [one] => Ok(one),
        [] => Err(format!("the game folder's {DLC_DIR} folder holds no level folder")),
        many => Err(format!(
            "the game folder's {DLC_DIR} folder holds {} level folders ({}), and Net.DLCGetMountPoint names one",
            many.len(),
            many.join(", ")
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stream_installs_where_the_levels_sound_bootstrap_opens_it() {
        // dlctest01_soundbootstrap.lua:188, with PC's Sound.GetAudioDir():
        // mount .. "\" .. upper(dir) .. "\" .. upper(name) .. ".PWS"
        let opened = format!("{}\\{}\\{}.PWS", mount_point("dlc01"), ".\\Data\\Audios".to_uppercase(), "VO_STREAM_DLCTEST.ENGLISH");
        // Windows paths ignore case and `.` components.
        let resolved: Vec<String> = opened.split('\\').filter(|c| *c != ".").map(str::to_lowercase).collect();
        let installed: Vec<String> =
            stream_relative("dlc01", "vo_stream_dlctest.english.pws").split('/').map(str::to_lowercase).collect();
        assert_eq!(resolved, installed);
        assert!(!stream_relative("dlc01", "dlctest_streaming.pws").starts_with("data/"));
    }

    #[test]
    fn the_mount_point_is_the_one_level_folder() {
        assert_eq!(single_level(&["dlc01".to_string()]).unwrap(), "dlc01");
        assert!(single_level(&[]).unwrap_err().contains("no level folder"));
        let two = ["dlc01".to_string(), "dlc02".to_string()];
        assert!(single_level(&two).unwrap_err().contains("dlc01, dlc02"));
    }
}
