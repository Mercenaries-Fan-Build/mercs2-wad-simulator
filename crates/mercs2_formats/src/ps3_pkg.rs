//! Retail PSN `.pkg` unpacker (port of `ps3_pkg_unpack.py`).
//!
//! A retail PKG has a cleartext header + metadata block, then an AES-128-CTR
//! encrypted data region (key = [`crate::ps3_keys::ps3_pkg_key`], IV = the
//! header `riv` @ 0x70). The decrypted region opens with a table of 32-byte file
//! entries followed by the file bodies. Decrypting the whole region and reading
//! the table gives the inner files (for MERCS2 WIF DLC: `DLC01.EDAT`).

use crate::ps3_crypto::aes128_ctr;
use crate::ps3_keys::ps3_pkg_key;

pub const PKG_MAGIC: &[u8; 4] = b"\x7fPKG";

fn u16_be(d: &[u8], o: usize) -> u16 {
    u16::from_be_bytes([d[o], d[o + 1]])
}
fn u32_be(d: &[u8], o: usize) -> u32 {
    u32::from_be_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}
fn u64_be(d: &[u8], o: usize) -> u64 {
    u64::from_be_bytes([
        d[o], d[o + 1], d[o + 2], d[o + 3], d[o + 4], d[o + 5], d[o + 6], d[o + 7],
    ])
}

/// One file entry from the decrypted PKG file table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PkgEntry {
    pub name: String,
    pub offset: u64,
    pub size: u64,
    pub flags: u32,
}

impl PkgEntry {
    /// Low byte of `flags` — the entry type (4 = directory, 3 = std file, etc.).
    pub fn type_id(&self) -> u8 {
        (self.flags & 0xff) as u8
    }
    pub fn is_dir(&self) -> bool {
        self.type_id() == 4
    }
}

/// A parsed + decrypted PKG.
#[derive(Clone, Debug)]
pub struct Pkg {
    pub content_id: String,
    pub revision: u16,
    pub pkg_type: u16,
    pub riv: [u8; 16],
    /// Decrypted data region (file table + bodies).
    pub decrypted: Vec<u8>,
    pub entries: Vec<PkgEntry>,
}

impl Pkg {
    /// Return the decrypted bytes of an entry by exact name (e.g. "USRDIR/DLC01.EDAT").
    pub fn file_bytes(&self, name: &str) -> Option<&[u8]> {
        self.entries.iter().find(|e| e.name == name).map(|e| {
            let s = e.offset as usize;
            let end = s + e.size as usize;
            &self.decrypted[s..end.min(self.decrypted.len())]
        })
    }
}

/// Parse a retail PSN PKG and decrypt its data region.
pub fn parse_pkg(blob: &[u8]) -> Result<Pkg, String> {
    if blob.len() < 0x80 || &blob[0..4] != PKG_MAGIC {
        return Err(format!(
            "not a PKG (magic {:02x?})",
            &blob[0..4.min(blob.len())]
        ));
    }
    let revision = u16_be(blob, 4);
    let pkg_type = u16_be(blob, 6);
    let item_cnt = u32_be(blob, 0x14) as usize; // meta block: off,cnt,sz,item_cnt @ 8
    let data_off = u64_be(blob, 0x20) as usize;
    let data_sz = u64_be(blob, 0x28) as usize;
    let content_id = {
        let raw = &blob[0x30..0x30 + 0x24];
        let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
        String::from_utf8_lossy(&raw[..end]).into_owned()
    };
    let mut riv = [0u8; 16];
    riv.copy_from_slice(&blob[0x70..0x80]);

    if data_off + data_sz > blob.len() {
        return Err(format!(
            "data region 0x{data_off:x}+0x{data_sz:x} exceeds file 0x{:x}",
            blob.len()
        ));
    }

    let decrypted = aes128_ctr(&ps3_pkg_key(), &riv, &blob[data_off..data_off + data_sz]);

    let mut entries = Vec::with_capacity(item_cnt);
    for i in 0..item_cnt {
        let base = i * 0x20;
        if base + 0x20 > decrypted.len() {
            return Err(format!("entry {i} table overruns decrypted region"));
        }
        let name_off = u32_be(&decrypted, base) as usize;
        let name_sz = u32_be(&decrypted, base + 4) as usize;
        let file_off = u64_be(&decrypted, base + 8);
        let file_sz = u64_be(&decrypted, base + 0x10);
        let flags = u32_be(&decrypted, base + 0x18);
        let name = String::from_utf8_lossy(&decrypted[name_off..name_off + name_sz]).into_owned();
        entries.push(PkgEntry {
            name,
            offset: file_off,
            size: file_sz,
            flags,
        });
    }

    Ok(Pkg {
        content_id,
        revision,
        pkg_type,
        riv,
        decrypted,
        entries,
    })
}
