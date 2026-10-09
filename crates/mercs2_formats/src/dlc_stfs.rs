//! STFS (Xbox 360 secure container) reader + RAR extraction.
//!
//! Port of the STFS half of `tools/x360_dlc_io.py` (py360-derived block
//! mapping). Walks the interleaved hash-table/data-block layout to read logical
//! file data (here: the DLC `.doh`). RAR extraction shells out to the extractor
//! the caller names ([`RarExtractor`]: `UnRAR.exe` or `bsdtar` at an explicit path),
//! mirroring the ffmpeg/java subprocess pattern elsewhere in this workspace.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::dlc_input::SCFF_MAGIC;

const STFS_BLOCK_SIZE: usize = 0x1000; // 4 KB
const STFS_DATA_OFFSET: i64 = 0xC000;
const STFS_HASH_ENTRY_SIZE: usize = 0x18;
const STFS_BLOCKS_PER_L0: u64 = 0xAA; // 170
const STFS_BLOCKS_PER_L1: u64 = 0x70E4; // 28,900
const STFS_BLOCKS_PER_L2: u64 = 0x4AF768; // 4,913,000
const STFS_HASH_BLOCK_SENTINEL: u32 = 0xFFFFFF;

pub const STFS_MAGIC_LIVE: &[u8; 4] = b"LIVE";
pub const STFS_MAGIC_PIRS: &[u8; 4] = b"PIRS";
pub const STFS_MAGIC_CON: &[u8; 4] = b"CON ";

fn read_u32_be(d: &[u8], off: usize) -> u32 {
    u32::from_be_bytes([d[off], d[off + 1], d[off + 2], d[off + 3]])
}
fn u24le(d: &[u8], o: usize) -> u32 {
    d[o] as u32 | (d[o + 1] as u32) << 8 | (d[o + 2] as u32) << 16
}

#[derive(Clone, Debug)]
pub struct FileTableEntry {
    pub name: String,
    pub is_dir: bool,
    pub consecutive: bool,
    pub valid_blocks: u32,
    pub alloc_blocks: u32,
    pub first_block: u32,
    pub path_ind: u16,
    pub file_size: u32,
}

/// LIVE/PIRS use 1 block per hash table (shift=0); CON uses 2 (shift=1).
fn detect_table_size_shift(stfs: &[u8]) -> u32 {
    if stfs.len() >= 0x344 && &stfs[0..4] == STFS_MAGIC_CON {
        let entry_id = read_u32_be(stfs, 0x340);
        if ((entry_id.wrapping_add(0xFFF)) & 0xF000) >> 0xC == 0xB {
            return 0;
        }
        return 1;
    }
    0
}

/// Logical data-block index → physical on-disk block index (skips hash tables).
fn fix_blocknum(logical: u32, shift: u32) -> u32 {
    let lb = logical as u64;
    let mut adjust: u64 = 0;
    if lb >= STFS_BLOCKS_PER_L0 {
        adjust += ((lb / STFS_BLOCKS_PER_L0) + 1) << shift;
    }
    if lb >= STFS_BLOCKS_PER_L1 {
        adjust += ((lb / STFS_BLOCKS_PER_L1) + 1) << shift;
    }
    if lb >= STFS_BLOCKS_PER_L2 {
        adjust += ((lb / STFS_BLOCKS_PER_L2) + 1) << shift;
    }
    (lb + adjust) as u32
}

/// Read `length` bytes from a physical block (signed: the first L0 table is at -1 → 0xB000).
fn read_physical_block(stfs: &[u8], phys: i64, length: usize) -> &[u8] {
    let offset = STFS_DATA_OFFSET + phys * STFS_BLOCK_SIZE as i64;
    if offset < 0 {
        return &[];
    }
    let start = offset as usize;
    if start >= stfs.len() {
        return &[];
    }
    let end = (start + length).min(stfs.len());
    &stfs[start..end]
}

/// Physical block of the L0 hash table covering `logical_block` (py360 spacing).
fn get_l0_hash_table_phys_block(logical: u32, shift: u32, table_offset: i64) -> i64 {
    // (blocks_per_table, l1_spacing, l2_spacing) for shift 0 and 1.
    let spacing: [(i64, i64, i64); 2] = [(0xAB, 0x718F, 0xFE7DA), (0xAC, 0x723A, 0xFD00B)];
    let lb = logical as i64;
    let mut table_num = (lb / STFS_BLOCKS_PER_L0 as i64) * spacing[shift as usize].0;
    if lb >= STFS_BLOCKS_PER_L0 as i64 {
        table_num += ((lb / STFS_BLOCKS_PER_L1 as i64) + 1) << shift;
    }
    if lb >= STFS_BLOCKS_PER_L1 as i64 {
        table_num += 1i64 << shift;
    }
    table_num += table_offset - (1i64 << shift);
    table_num
}

/// Read the hash record for `logical_block`: returns `(next_block, info_byte)`.
fn get_block_hash_entry(stfs: &[u8], logical: u32, shift: u32, table_offset: i64) -> (u32, u8) {
    let record = (logical as u64 % STFS_BLOCKS_PER_L0) as usize;
    let table_phys = get_l0_hash_table_phys_block(logical, shift, table_offset);
    let table_data = read_physical_block(stfs, table_phys, STFS_BLOCK_SIZE);
    let entry_start = record * STFS_HASH_ENTRY_SIZE;
    if table_data.len() < entry_start + STFS_HASH_ENTRY_SIZE {
        return (STFS_HASH_BLOCK_SENTINEL, 0);
    }
    let entry = &table_data[entry_start..entry_start + STFS_HASH_ENTRY_SIZE];
    let info = entry[0x14];
    let next_block = u32::from_be_bytes([0, entry[0x15], entry[0x16], entry[0x17]]);
    (next_block, info)
}

/// Volume descriptor @0x379 → (file_table_first_block, file_table_block_count, total_alloc).
fn parse_volume_descriptor(stfs: &[u8]) -> (u32, u16, u32) {
    let d = 0x379;
    let ft_block_count = u16::from_le_bytes([stfs[d + 3], stfs[d + 4]]);
    let ft_first_block = u24le(stfs, d + 5);
    let total_alloc = read_u32_be(stfs, d + 28);
    (ft_first_block, ft_block_count, total_alloc)
}

/// Walk the file-table block chain and parse 0x40-byte entries.
fn parse_stfs_file_table(stfs: &[u8], shift: u32) -> Vec<FileTableEntry> {
    let (ft_first, ft_count, _) = parse_volume_descriptor(stfs);

    let mut ft_data: Vec<u8> = Vec::new();
    let mut block = ft_first;
    for _ in 0..ft_count {
        if block == STFS_HASH_BLOCK_SENTINEL {
            break;
        }
        let phys = fix_blocknum(block, shift);
        ft_data.extend_from_slice(read_physical_block(stfs, phys as i64, STFS_BLOCK_SIZE));
        let (mut next, mut info) = get_block_hash_entry(stfs, block, shift, 0);
        if shift > 0 && info < 0x80 {
            let r = get_block_hash_entry(stfs, block, shift, 1);
            next = r.0;
            info = r.1;
        }
        let _ = info;
        block = next;
    }

    let mut entries = Vec::new();
    for i in 0..ft_data.len() / 0x40 {
        let off = i * 0x40;
        let name_raw = &ft_data[off..off + 0x28];
        if name_raw[0] == 0 {
            break;
        }
        let name_end = name_raw
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(name_raw.len());
        let name = String::from_utf8_lossy(&name_raw[..name_end]).into_owned();
        let flags = ft_data[off + 0x28];
        entries.push(FileTableEntry {
            name,
            is_dir: flags & 0x80 != 0,
            consecutive: flags & 0x40 != 0,
            valid_blocks: u24le(&ft_data, off + 0x29),
            alloc_blocks: u24le(&ft_data, off + 0x2C),
            first_block: u24le(&ft_data, off + 0x2F),
            path_ind: u16::from_be_bytes([ft_data[off + 0x32], ft_data[off + 0x33]]),
            file_size: read_u32_be(&ft_data, off + 0x34),
        });
    }
    entries
}

/// Hash-block-aware reader for file data inside an STFS container.
pub struct StfsReader {
    pub stfs_data: Vec<u8>,
    pub table_size_shift: u32,
    pub file_table: Vec<FileTableEntry>,
}

impl StfsReader {
    pub fn new(stfs_data: Vec<u8>) -> Self {
        let table_size_shift = detect_table_size_shift(&stfs_data);
        let file_table = parse_stfs_file_table(&stfs_data, table_size_shift);
        Self {
            stfs_data,
            table_size_shift,
            file_table,
        }
    }

    /// The DOH file-table entry (first whose name contains "doh", case-insensitive).
    pub fn doh_entry(&self) -> Option<&FileTableEntry> {
        self.file_table
            .iter()
            .find(|e| e.name.to_lowercase().contains("doh"))
    }

    /// Build the ordered list of logical block numbers for a file by walking its chain.
    fn build_chain(&self, first_block: u32, alloc_blocks: u32) -> Vec<u32> {
        let mut chain = Vec::new();
        let mut block = first_block;
        let max_blocks = alloc_blocks as u64 + 10; // small safety margin
        for _ in 0..max_blocks {
            if block == STFS_HASH_BLOCK_SENTINEL {
                break;
            }
            chain.push(block);
            let (mut next, mut info) =
                get_block_hash_entry(&self.stfs_data, block, self.table_size_shift, 0);
            if self.table_size_shift > 0 && info < 0x80 {
                let r = get_block_hash_entry(&self.stfs_data, block, self.table_size_shift, 1);
                next = r.0;
                info = r.1;
            }
            let _ = info;
            block = next;
        }
        chain
    }

    fn read_with_chain(
        &self,
        chain: &[u32],
        doh_offset: usize,
        length: usize,
    ) -> Result<Vec<u8>, String> {
        let mut out = Vec::with_capacity(length);
        let mut remaining = length;
        let mut chain_idx = doh_offset / STFS_BLOCK_SIZE;
        let mut skip = doh_offset % STFS_BLOCK_SIZE;

        while remaining > 0 {
            if chain_idx >= chain.len() {
                return Err(format!(
                    "DOH chain index {chain_idx} exceeds chain length ({})",
                    chain.len()
                ));
            }
            let logical = chain[chain_idx];
            let phys = fix_blocknum(logical, self.table_size_shift) as i64;
            let abs = STFS_DATA_OFFSET + phys * STFS_BLOCK_SIZE as i64 + skip as i64;
            let chunk = (STFS_BLOCK_SIZE - skip).min(remaining);
            let start = abs as usize;
            let end = start + chunk;
            if end > self.stfs_data.len() {
                return Err(format!(
                    "STFS read past EOF: chain[{chain_idx}] logical {logical} phys {phys} offset 0x{abs:X}"
                ));
            }
            out.extend_from_slice(&self.stfs_data[start..end]);
            remaining -= chunk;
            chain_idx += 1;
            skip = 0;
        }
        Ok(out)
    }

    /// Read the full DOH file (walks its hash chain). Equivalent to Python `reader.read(0, doh_size)`.
    pub fn read_doh(&self) -> Result<Vec<u8>, String> {
        let entry = self
            .doh_entry()
            .ok_or("No DOH file found in STFS file table")?;
        self.read_entry(entry)
    }

    /// The file-table entry whose name is exactly `name` (STFS names are case-sensitive).
    pub fn entry(&self, name: &str) -> Option<&FileTableEntry> {
        self.file_table.iter().find(|e| !e.is_dir && e.name == name)
    }

    /// Read a whole file by walking its hash chain.
    pub fn read_entry(&self, entry: &FileTableEntry) -> Result<Vec<u8>, String> {
        let chain = self.build_chain(entry.first_block, entry.alloc_blocks);
        self.read_with_chain(&chain, 0, entry.file_size as usize)
    }
}

/// The program that unpacks a RAR'd DLC download, with the path of its executable. The caller
/// names both; [`extract_stfs_from_rar`] runs exactly that program.
#[derive(Clone, Debug)]
pub enum RarExtractor {
    /// `UnRAR.exe`, run as `<path> x -o+ -y <rar> <out_dir>\`.
    UnRar(PathBuf),
    /// `bsdtar`, run as `<path> -xf <rar> -C <out_dir>`.
    Bsdtar(PathBuf),
}

/// Extract an STFS container from a RAR archive and return a reader.
///
/// Empties `work_dir/stfs` (removing what a previous extraction left there), runs `extractor`
/// into it, then selects the container with [`select_stfs_container`]: the archive must extract
/// exactly one file whose first four bytes are an STFS magic (`LIVE`/`PIRS`/`CON `). Errors when
/// the extractor's path is not a file, the program cannot be run or exits unsuccessfully, or the
/// selection fails.
pub fn extract_stfs_from_rar(
    rar_path: &Path,
    work_dir: &Path,
    extractor: &RarExtractor,
) -> Result<StfsReader, String> {
    let stfs_dir = work_dir.join("stfs");
    if stfs_dir.exists() {
        std::fs::remove_dir_all(&stfs_dir)
            .map_err(|e| format!("remove previous extraction {}: {e}", stfs_dir.display()))?;
    }
    std::fs::create_dir_all(&stfs_dir)
        .map_err(|e| format!("mkdir {}: {e}", stfs_dir.display()))?;

    let status = match extractor {
        RarExtractor::UnRar(unrar) => {
            if !unrar.is_file() {
                return Err(format!("UnRAR executable {} is not a file", unrar.display()));
            }
            Command::new(unrar)
                .arg("x")
                .arg("-o+")
                .arg("-y")
                .arg(rar_path)
                .arg(format!("{}\\", stfs_dir.display()))
                .output()
                .map_err(|e| format!("UnRAR {} spawn failed: {e}", unrar.display()))?
        }
        RarExtractor::Bsdtar(bsdtar) => {
            if !bsdtar.is_file() {
                return Err(format!("bsdtar executable {} is not a file", bsdtar.display()));
            }
            Command::new(bsdtar)
                .arg("-xf")
                .arg(rar_path)
                .arg("-C")
                .arg(&stfs_dir)
                .output()
                .map_err(|e| format!("bsdtar {} spawn failed: {e}", bsdtar.display()))?
        }
    };
    if !status.status.success() {
        return Err(format!(
            "archive extraction failed: {}",
            String::from_utf8_lossy(&status.stderr)
        ));
    }

    let path = select_stfs_container(&stfs_dir)?;
    let data = std::fs::read(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    Ok(StfsReader::new(data))
}

/// One regular file found under an extraction directory.
#[derive(Debug)]
struct ExtractedFile {
    path: PathBuf,
    size: u64,
    /// The file's first four bytes; `None` when the file is shorter than four bytes.
    magic: Option<[u8; 4]>,
}

impl ExtractedFile {
    fn is_stfs(&self) -> bool {
        matches!(&self.magic, Some(m) if m == STFS_MAGIC_LIVE || m == STFS_MAGIC_PIRS || m == STFS_MAGIC_CON)
    }

    fn describe(&self) -> String {
        let magic = match &self.magic {
            Some(m) => format!("magic {:?}", String::from_utf8_lossy(m)),
            None => "shorter than 4 bytes".to_string(),
        };
        format!("{} ({} bytes, {magic})", self.path.display(), self.size)
    }
}

/// The single STFS container among the files under `dir` (searched recursively).
///
/// Every regular file is opened and its first four bytes compared with the STFS magics
/// `LIVE`/`PIRS`/`CON `. Exactly one file must match; no match or several matches is an error
/// listing every file found with its size and magic. A directory or file that cannot be read, and
/// any entry that is neither a directory nor a regular file (symlinks included), is an error
/// naming it.
pub fn select_stfs_container(dir: &Path) -> Result<PathBuf, String> {
    let mut files = Vec::new();
    collect_extracted_files(dir, &mut files)?;
    files.sort_by(|a, b| a.path.cmp(&b.path));

    let stfs: Vec<&ExtractedFile> = files.iter().filter(|f| f.is_stfs()).collect();
    match stfs.as_slice() {
        [one] => Ok(one.path.clone()),
        _ => {
            let listing = if files.is_empty() {
                "  (no files)".to_string()
            } else {
                files
                    .iter()
                    .map(|f| format!("  {}", f.describe()))
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            let problem = if stfs.is_empty() {
                "no file has an STFS magic (LIVE/PIRS/CON )".to_string()
            } else {
                format!("{} files have an STFS magic (LIVE/PIRS/CON ); exactly one is required", stfs.len())
            };
            Err(format!("{}: {problem}; files found:\n{listing}", dir.display()))
        }
    }
}

fn collect_extracted_files(dir: &Path, out: &mut Vec<ExtractedFile>) -> Result<(), String> {
    use std::io::Read;
    let rd = std::fs::read_dir(dir).map_err(|e| format!("read dir {}: {e}", dir.display()))?;
    for entry in rd {
        let entry = entry.map_err(|e| format!("read dir {}: {e}", dir.display()))?;
        let path = entry.path();
        let ft = entry
            .file_type()
            .map_err(|e| format!("file type of {}: {e}", path.display()))?;
        if ft.is_dir() {
            collect_extracted_files(&path, out)?;
        } else if ft.is_file() {
            let mut f = std::fs::File::open(&path)
                .map_err(|e| format!("open {}: {e}", path.display()))?;
            let size = f
                .metadata()
                .map_err(|e| format!("metadata of {}: {e}", path.display()))?
                .len();
            let mut head = Vec::with_capacity(4);
            (&mut f)
                .take(4)
                .read_to_end(&mut head)
                .map_err(|e| format!("read {}: {e}", path.display()))?;
            let magic = <[u8; 4]>::try_from(head.as_slice()).ok();
            out.push(ExtractedFile { path, size, magic });
        } else {
            return Err(format!(
                "{} is neither a directory nor a regular file ({ft:?})",
                path.display()
            ));
        }
    }
    Ok(())
}

/// Load an STFS container or a raw DOH file → (doh_bytes, source_type).
pub fn load_stfs_or_doh(path: &Path) -> Result<(Vec<u8>, &'static str), String> {
    let data = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let magic = &data[..4.min(data.len())];
    if magic == STFS_MAGIC_LIVE || magic == STFS_MAGIC_PIRS || magic == STFS_MAGIC_CON {
        let reader = StfsReader::new(data);
        Ok((reader.read_doh()?, "stfs"))
    } else if magic == SCFF_MAGIC {
        Ok((data, "doh"))
    } else {
        Err(format!("Unknown file format: {magic:?}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fix_blocknum_skips_hash_tables() {
        // No adjustment below the first L0 boundary.
        assert_eq!(fix_blocknum(0, 0), 0);
        assert_eq!(fix_blocknum(169, 0), 169);
        // At block 170 (0xAA) the formula inserts (170//170)+1 = 2 hash blocks.
        assert_eq!(fix_blocknum(170, 0), 172);
    }

    #[test]
    fn first_l0_table_is_block_minus_one() {
        // The L0 hash table for the first group sits at physical block -1 (offset 0xB000).
        assert_eq!(get_l0_hash_table_phys_block(0, 0, 0), -1);
        assert_eq!(get_l0_hash_table_phys_block(5, 0, 0), -1);
    }

    /// A fresh, empty directory unique to `label` under the system temp dir.
    fn fresh_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mercs2_formats_select_stfs_{}_{label}",
            std::process::id()
        ));
        if dir.exists() {
            std::fs::remove_dir_all(&dir).expect("clear test dir");
        }
        std::fs::create_dir_all(&dir).expect("create test dir");
        dir
    }

    fn write(path: &Path, bytes: &[u8]) {
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(path, bytes).expect("write");
    }

    #[test]
    fn select_stfs_picks_the_only_stfs_file_of_any_size() {
        let dir = fresh_dir("one");
        // A small STFS file is selected over a larger non-STFS file: size plays no part.
        let stfs = dir.join("45410828/00000002/PKG");
        write(&stfs, b"LIVE");
        write(&dir.join("readme.nfo"), &[b'x'; 4096]);
        write(&dir.join("tiny"), b"ab");
        assert_eq!(select_stfs_container(&dir).expect("select"), stfs);
    }

    #[test]
    fn select_stfs_accepts_each_magic() {
        for (label, magic) in [("live", STFS_MAGIC_LIVE), ("pirs", STFS_MAGIC_PIRS), ("con", STFS_MAGIC_CON)] {
            let dir = fresh_dir(label);
            let path = dir.join("pkg");
            let mut bytes = magic.to_vec();
            bytes.extend_from_slice(&[0u8; 16]);
            write(&path, &bytes);
            assert_eq!(select_stfs_container(&dir).expect(label), path);
        }
    }

    #[test]
    fn select_stfs_errors_when_none_and_lists_files() {
        let dir = fresh_dir("none");
        write(&dir.join("a.bin"), b"SCFFxxxx");
        write(&dir.join("sub/b"), b"C");
        let err = select_stfs_container(&dir).expect_err("no STFS must be an error");
        assert!(err.contains("no file has an STFS magic"), "{err}");
        assert!(err.contains("a.bin (8 bytes, magic \"SCFF\")"), "{err}");
        assert!(err.contains("b (1 bytes, shorter than 4 bytes)"), "{err}");
    }

    #[test]
    fn select_stfs_errors_on_empty_directory() {
        let dir = fresh_dir("empty");
        let err = select_stfs_container(&dir).expect_err("empty dir must be an error");
        assert!(err.contains("no file has an STFS magic"), "{err}");
        assert!(err.contains("(no files)"), "{err}");
    }

    #[test]
    fn select_stfs_errors_when_several_and_lists_them() {
        let dir = fresh_dir("several");
        write(&dir.join("one"), b"LIVE....");
        write(&dir.join("two"), b"CON ");
        let err = select_stfs_container(&dir).expect_err("two STFS files must be an error");
        assert!(err.contains("2 files have an STFS magic"), "{err}");
        assert!(err.contains("one (8 bytes, magic \"LIVE\")"), "{err}");
        assert!(err.contains("two (4 bytes, magic \"CON \")"), "{err}");
    }

    #[test]
    fn select_stfs_errors_on_missing_directory() {
        let dir = fresh_dir("missing").join("absent");
        let err = select_stfs_container(&dir).expect_err("missing dir must be an error");
        assert!(err.contains("read dir"), "{err}");
        assert!(err.contains("absent"), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn select_stfs_errors_on_symlink() {
        let dir = fresh_dir("symlink");
        write(&dir.join("pkg"), b"LIVE");
        std::os::unix::fs::symlink(dir.join("pkg"), dir.join("link")).expect("symlink");
        let err = select_stfs_container(&dir).expect_err("a symlink must be an error");
        assert!(err.contains("link is neither a directory nor a regular file"), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn select_stfs_errors_on_unreadable_file() {
        use std::os::unix::fs::PermissionsExt;
        let dir = fresh_dir("unreadable");
        let path = dir.join("locked");
        write(&path, b"LIVE");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).expect("chmod");
        if std::fs::File::open(&path).is_ok() {
            // Running as root: permission bits do not deny the open, so the case cannot be built.
            panic!("cannot build an unreadable file as this user; run the test as a non-root user");
        }
        let err = select_stfs_container(&dir).expect_err("an unreadable file must be an error");
        assert!(err.contains("open"), "{err}");
        assert!(err.contains("locked"), "{err}");
    }

    /// The retail Xbox LIVE Blow It Up Again Pack, named by the `x360_dlc_biua_stfs` key of
    /// `.mercs2-local.toml`, yields the `DLC01.doh` the exe-map Lua corpus catalogues
    /// (`crates/mercs2_exemap/data/wads.tsv`, corpus `dlc-biua-x360-retail`): 251,953,152 bytes,
    /// sha256 `5b0c222d…`. A missing key or file fails the test naming it.
    #[cfg(feature = "retail")]
    #[test]
    fn retail_biua_package_yields_the_catalogued_doh() {
        use sha2::{Digest, Sha256};
        let path = crate::game_paths::local_config_x360_dlc_biua_stfs(Path::new(env!("CARGO_MANIFEST_DIR")))
            .unwrap_or_else(|e| panic!("{e}"));
        let stfs = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let doh = StfsReader::new(stfs).read_doh().expect("read_doh");
        assert_eq!(doh.len(), 251_953_152, "DOH length");
        assert_eq!(
            format!("{:x}", Sha256::digest(&doh)),
            "5b0c222d925e8c85000a925262e2b789fbf8c3e476d3d89e5935c7c018deb3ae",
            "DOH sha256"
        );
    }
}
