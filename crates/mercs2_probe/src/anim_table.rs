//! `anim-table` — one shipped `animationtable` (type `0x207359C7`) as retail's loader holds it:
//! INFO dims, the TYPE column names, the defaults row (`VALU` row 0), the data rows, and — with
//! `--list <column>` — the `ASTO` list each row's index cell points at (`[count][clips]`).
//!
//! The table is resolved by name through the WAD stack (`AssetSource`, patch overlays included).
//! `--handle 0xH` keeps only data rows whose first column is `H`. Data rows are numbered from 0, as
//! retail's row array (`+0x38`) and Handle index number them.

use mercs2_engine::asset::AssetSource;
use mercs2_formats::anim_select::{AnimTable, TYPE_ANIMTABLE};

pub fn anim_table(wadpath: &str, name: u32, handle: Option<u32>, list_col: Option<&str>) -> Result<(), String> {
    let mut assets = AssetSource::discover(wadpath, &[])?;
    let cont = assets
        .resolve(TYPE_ANIMTABLE, name)
        .ok_or_else(|| format!("animationtable 0x{name:08X}: in no open wad"))?;
    let t = AnimTable::parse(&cont).map_err(|e| format!("animationtable 0x{name:08X}: {e}"))?;
    let list_idx = match list_col {
        Some(c) => Some(
            t.cols()
                .iter()
                .position(|n| n.eq_ignore_ascii_case(c))
                .ok_or_else(|| format!("no column {c:?} in {:?}", t.cols()))?,
        ),
        None => None,
    };
    println!(
        "animationtable 0x{name:08X}: keyDims {} totalDims {} data rows {} pool {}",
        t.key_dims(),
        t.cols().len(),
        t.row_count(),
        t.pool().len()
    );
    for (i, c) in t.cols().iter().enumerate() {
        println!("  col {i}: {c}");
    }
    let hex = |row: &[u32]| row.iter().map(|v| format!("{v:08X}")).collect::<Vec<_>>().join(" ");
    let list = |row: &[u32]| -> Result<String, String> {
        Ok(match list_idx {
            Some(c) => format!("  list = [{}]", hex(t.list(row[c])?)),
            None => String::new(),
        })
    };
    println!("defaults: {}{}", hex(t.defaults()), list(t.defaults())?);
    for r in 0..t.row_count() {
        let row = t.row(r);
        if handle.is_some_and(|h| row[0] != h) {
            continue;
        }
        println!("row {r:5}: {}{}", hex(row), list(row)?);
    }
    Ok(())
}
