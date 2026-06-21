/// Exhaustive chunk tag enum for UCFX descriptor tags.
/// All known tags from format_reference.md.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkTag {
    // Container/group markers (row_u0 == 0xFFFFFFFF)
    Ucfx,
    Comp,
    Geom,
    Strm,

    // Common leaf chunks
    Info,      // lowercase "info"
    Data,      // lowercase "data"
    Schm,      // "schm"
    Flgs,      // "flgs"
    Decl,      // "decl"
    Ibuf,      // "IBUF"
    Bnds,      // "BNDS"
    Hier,      // "HIER"
    Prmg,      // "PRMG"
    Mtrl,      // "MTRL"
    Area,      // "AREA" (mesh sub-area container; consumed by Mesh_ConsumeChunk @0x478366 → 0x4a4ab0)
    InfoUpper, // "INFO" (different from lowercase "info")
    Body,      // "BODY"
    Chdr,      // "CHDR"
    Stat,      // "STAT"

    // GEOM internals
    Swit,      // "SWIT"
    Prmt,      // "PRMT"
    Cexe,      // "CEXE"
    Enum,      // "enum"
    Flgt,      // "flgt"
    Indx,      // "INDX" (inside GEOM, not FFCS-level)

    // Stringdb — body is natively BE on ALL platforms
    Syek,      // "SYEK"
    Srts,      // "SRTS"

    // Sequence / precache / shader
    Sequ,      // "sequ"
    Sinf,      // "SINF"
    Item,      // "ITEM"
    Cerp,      // "CERP"
    Scrb,      // "SCRB"

    // ECS / entity metadata
    Name,      // "NAME"
    Strs,      // "STRS"
    Trns,      // "TRNS"
    Ainf,      // "AINF"
    Uniq,      // "UNIQ"

    // Anim state machine
    Stns,      // "stns"
    Actn,      // "actn"

    // FX dictionary
    Dict,      // "DICT"

    // Dependency list
    Deps,      // "DEPS"

    // Skinned mesh marker + resident watermap
    Skin,      // "SKIN"
    Watr,      // "watr"

    // UCFX asset chunks dispatched by the engine but previously unregistered
    // (see tag_registry.rs / docs/ucfx_tag_registry.md for handler addresses).
    Asto, // "ASTO"
    Atrb, // "ATRB"
    Binn, // "BINN" (Lua bytecode container; transcoded, not byte-swapped)
    Bshi, // "BSHI"
    Bshp, // "BSHP"
    Char, // "CHAR" (renderable sub-chunk; consumer FUN_004ac8e0)
    Colr, // "COLR"
    Damg, // "DAMG"
    DataUpper, // "DATA"
    Debr, // "DEBR"
    DeclUpper, // "DECL"
    Emit, // "EMIT"
    Emtr, // "EMTR"
    Frce, // "FRCE"
    Inst, // "INST"
    Keys, // "KEYS"
    Manm, // "MANM"
    Mesh, // "MESH"
    Minf, // "MINF"
    Node, // "NODE"
    Part, // "PART"
    Phy2, // "PHY2"
    Poff, // "POFF"
    Ptch, // "PTCH"
    Ptms, // "PTMS"
    Ptyp, // "PTYP"
    Soun, // "SOUN"
    Text, // "TEXT"
    Tiny, // "TINY"
    Trck, // "TRCK"
    Tree, // "TREE"
    Trfm, // "TRFM"
    Type, // "TYPE"
    Valu, // "VALU"
    TrnsLower, // "trns"

    // Unknown tag (carries raw bytes for diagnostics)
    Unknown([u8; 4]),
}

impl ChunkTag {
    /// Parse a 4-byte tag (already in LE/native order, not reversed).
    pub fn from_bytes(b: [u8; 4]) -> Self {
        match &b {
            b"UCFX" => Self::Ucfx,
            b"COMP" => Self::Comp,
            b"GEOM" => Self::Geom,
            b"STRM" => Self::Strm,
            b"info" => Self::Info,
            b"data" => Self::Data,
            b"schm" => Self::Schm,
            b"flgs" => Self::Flgs,
            b"decl" => Self::Decl,
            b"IBUF" => Self::Ibuf,
            b"BNDS" => Self::Bnds,
            b"HIER" => Self::Hier,
            b"PRMG" => Self::Prmg,
            b"MTRL" => Self::Mtrl,
            b"AREA" => Self::Area,
            b"INFO" => Self::InfoUpper,
            b"BODY" => Self::Body,
            b"CHDR" => Self::Chdr,
            b"STAT" => Self::Stat,
            b"SWIT" => Self::Swit,
            b"PRMT" => Self::Prmt,
            b"CEXE" => Self::Cexe,
            b"enum" => Self::Enum,
            b"flgt" => Self::Flgt,
            b"INDX" => Self::Indx,
            b"SYEK" => Self::Syek,
            b"SRTS" => Self::Srts,
            b"sequ" => Self::Sequ,
            b"SINF" => Self::Sinf,
            b"ITEM" => Self::Item,
            b"CERP" => Self::Cerp,
            b"SCRB" => Self::Scrb,
            b"NAME" => Self::Name,
            b"STRS" => Self::Strs,
            b"TRNS" => Self::Trns,
            b"AINF" => Self::Ainf,
            b"UNIQ" => Self::Uniq,
            b"stns" => Self::Stns,
            b"actn" => Self::Actn,
            b"DICT" => Self::Dict,
            b"DEPS" => Self::Deps,
            b"SKIN" => Self::Skin,
            b"watr" => Self::Watr,
            b"ASTO" => Self::Asto,
            b"ATRB" => Self::Atrb,
            b"BINN" => Self::Binn,
            b"BSHI" => Self::Bshi,
            b"BSHP" => Self::Bshp,
            b"CHAR" => Self::Char,
            b"COLR" => Self::Colr,
            b"DAMG" => Self::Damg,
            b"DATA" => Self::DataUpper,
            b"DEBR" => Self::Debr,
            b"DECL" => Self::DeclUpper,
            b"EMIT" => Self::Emit,
            b"EMTR" => Self::Emtr,
            b"FRCE" => Self::Frce,
            b"INST" => Self::Inst,
            b"KEYS" => Self::Keys,
            b"MANM" => Self::Manm,
            b"MESH" => Self::Mesh,
            b"MINF" => Self::Minf,
            b"NODE" => Self::Node,
            b"PART" => Self::Part,
            b"PHY2" => Self::Phy2,
            b"POFF" => Self::Poff,
            b"PTCH" => Self::Ptch,
            b"PTMS" => Self::Ptms,
            b"PTYP" => Self::Ptyp,
            b"SOUN" => Self::Soun,
            b"TEXT" => Self::Text,
            b"TINY" => Self::Tiny,
            b"TRCK" => Self::Trck,
            b"TREE" => Self::Tree,
            b"TRFM" => Self::Trfm,
            b"TYPE" => Self::Type,
            b"VALU" => Self::Valu,
            b"trns" => Self::TrnsLower,
            _ => Self::Unknown(b),
        }
    }

    /// Tags whose body data must NEVER be endian-swapped.
    /// These are natively big-endian on all platforms (PC included).
    pub fn is_native_be(&self) -> bool {
        matches!(self, Self::Syek | Self::Srts)
    }

    /// Get the raw 4-byte tag representation.
    pub fn as_bytes(&self) -> [u8; 4] {
        match self {
            Self::Ucfx => *b"UCFX",
            Self::Comp => *b"COMP",
            Self::Geom => *b"GEOM",
            Self::Strm => *b"STRM",
            Self::Info => *b"info",
            Self::Data => *b"data",
            Self::Schm => *b"schm",
            Self::Flgs => *b"flgs",
            Self::Decl => *b"decl",
            Self::Ibuf => *b"IBUF",
            Self::Bnds => *b"BNDS",
            Self::Hier => *b"HIER",
            Self::Prmg => *b"PRMG",
            Self::Mtrl => *b"MTRL",
            Self::Area => *b"AREA",
            Self::InfoUpper => *b"INFO",
            Self::Body => *b"BODY",
            Self::Chdr => *b"CHDR",
            Self::Stat => *b"STAT",
            Self::Swit => *b"SWIT",
            Self::Prmt => *b"PRMT",
            Self::Cexe => *b"CEXE",
            Self::Enum => *b"enum",
            Self::Flgt => *b"flgt",
            Self::Indx => *b"INDX",
            Self::Syek => *b"SYEK",
            Self::Srts => *b"SRTS",
            Self::Sequ => *b"sequ",
            Self::Sinf => *b"SINF",
            Self::Item => *b"ITEM",
            Self::Cerp => *b"CERP",
            Self::Scrb => *b"SCRB",
            Self::Name => *b"NAME",
            Self::Strs => *b"STRS",
            Self::Trns => *b"TRNS",
            Self::Ainf => *b"AINF",
            Self::Uniq => *b"UNIQ",
            Self::Stns => *b"stns",
            Self::Actn => *b"actn",
            Self::Dict => *b"DICT",
            Self::Deps => *b"DEPS",
            Self::Skin => *b"SKIN",
            Self::Watr => *b"watr",
            Self::Asto => *b"ASTO",
            Self::Atrb => *b"ATRB",
            Self::Binn => *b"BINN",
            Self::Bshi => *b"BSHI",
            Self::Bshp => *b"BSHP",
            Self::Char => *b"CHAR",
            Self::Colr => *b"COLR",
            Self::Damg => *b"DAMG",
            Self::DataUpper => *b"DATA",
            Self::Debr => *b"DEBR",
            Self::DeclUpper => *b"DECL",
            Self::Emit => *b"EMIT",
            Self::Emtr => *b"EMTR",
            Self::Frce => *b"FRCE",
            Self::Inst => *b"INST",
            Self::Keys => *b"KEYS",
            Self::Manm => *b"MANM",
            Self::Mesh => *b"MESH",
            Self::Minf => *b"MINF",
            Self::Node => *b"NODE",
            Self::Part => *b"PART",
            Self::Phy2 => *b"PHY2",
            Self::Poff => *b"POFF",
            Self::Ptch => *b"PTCH",
            Self::Ptms => *b"PTMS",
            Self::Ptyp => *b"PTYP",
            Self::Soun => *b"SOUN",
            Self::Text => *b"TEXT",
            Self::Tiny => *b"TINY",
            Self::Trck => *b"TRCK",
            Self::Tree => *b"TREE",
            Self::Trfm => *b"TRFM",
            Self::Type => *b"TYPE",
            Self::Valu => *b"VALU",
            Self::TrnsLower => *b"trns",
            Self::Unknown(b) => *b,
        }
    }
}

impl std::fmt::Display for ChunkTag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let bytes = self.as_bytes();
        let s: String = bytes.iter().map(|&b| {
            if b.is_ascii_graphic() || b == b' ' { b as char } else { '?' }
        }).collect();
        write!(f, "{}", s)
    }
}
