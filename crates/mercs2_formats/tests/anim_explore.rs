use mercs2_formats::havok;

// Reimplement enough of the packfile walk here to dump the wavelet object bytes.
fn u32le(b: &[u8], o: usize) -> u32 { u32::from_le_bytes([b[o],b[o+1],b[o+2],b[o+3]]) }
fn f32le(b: &[u8], o: usize) -> f32 { f32::from_le_bytes([b[o],b[o+1],b[o+2],b[o+3]]) }

#[test]
fn explore_anim_class_counts() {
    let buf: &[u8] = include_bytes!("fixtures/anim_ks750_le.bin");
    let pf = havok::parse_packfile(buf).unwrap();
    eprintln!("version={:?} size={}", pf.version, pf.size);
    for (name, n) in &pf.class_counts { eprintln!("  {n:>3}  {name}"); }

    // Manual section walk to locate __data__ and the wavelet object.
    let sh = buf.windows(14).position(|w| w==b"__classnames__").unwrap();
    let mut secs = [[0u32;7];3];
    for s in 0..3 { for k in 0..7 { secs[s][k] = u32le(buf, sh + s*48 + 20 + k*4); } }
    let body0 = sh + 3*48;
    let data_pk = body0 + secs[0][6] as usize + secs[1][6] as usize;
    eprintln!("sh={sh} body0={body0} data_pk={data_pk} file_len={}", buf.len());
    eprintln!("sec2 (data) [abs,lf,gf,vf,exp,imp,end] = {:?}", secs[2]);

    let (d_lf,d_gf,d_vf,d_end) = (secs[2][1] as usize, secs[2][2] as usize, secs[2][3] as usize, secs[2][4] as usize);
    // local fixups
    let mut k = data_pk + d_lf;
    eprintln!("--- local fixups (src -> dst) ---");
    while k+8 <= data_pk + d_gf {
        let src = u32le(buf,k); if src==0xFFFFFFFF {break;}
        eprintln!("  {} -> {}", src, u32le(buf,k+4)); k+=8;
    }
    // virtual fixups
    eprintln!("--- virtual fixups (src, sec, cnoff) ---");
    let mut k = data_pk + d_vf;
    let mut wavelet_src = None;
    while k+12 <= data_pk + d_end {
        let src = u32le(buf,k); if src==0xFFFFFFFF {break;}
        eprintln!("  src={} sec={} cnoff={}", src, u32le(buf,k+4), u32le(buf,k+8));
        if wavelet_src.is_none() && src != 0 { wavelet_src = Some(src as usize); }
        k+=12;
    }

    // dump first 160 bytes of __data__ (container @0, wavelet @ some src)
    eprintln!("--- __data__ dump (offsets relative to data_pk) ---");
    for row in 0..(200/16) {
        let base = row*16;
        let mut hex = String::new();
        let mut asc = String::new();
        for c in 0..16 {
            let o = data_pk+base+c;
            if o < buf.len() { hex += &format!("{:02x} ", buf[o]); let ch=buf[o]; asc.push(if ch>=0x20&&ch<0x7f {ch as char} else {'.'}); }
        }
        eprintln!("  +{:03}: {}  {}", base, hex, asc);
    }

    // Interpret wavelet object fields at its src offset
    if let Some(ws) = wavelet_src {
        let o = data_pk + ws;
        eprintln!("--- wavelet object @ data+{} ---", ws);
        eprintln!("  +8  m_type(int)               = {}", u32le(buf,o+8) as i32);
        eprintln!("  +12 m_duration(f32)            = {}", f32le(buf,o+12));
        eprintln!("  +16 m_numTransformTracks(int)  = {}", u32le(buf,o+16) as i32);
        eprintln!("  +20 m_numFloatTracks(int)      = {}", u32le(buf,o+20) as i32);
        for off in (24..96).step_by(4) {
            eprintln!("  +{:<3} u32={:<12} i32={:<12} f32={}", off, u32le(buf,o+off), u32le(buf,o+off) as i32, f32le(buf,o+off));
        }
    }
}

#[test]
fn explore_databuffer_and_be() {
    let le: &[u8] = include_bytes!("fixtures/anim_ks750_le.bin");
    // data_pk from prior run = 912; wavelet obj @ data+64 => abs 976; dataBuffer @ data+160 => abs 1072
    let data_pk = 912usize;
    let obj = data_pk + 64;
    let buf = data_pk + 160; // dataBuffer start (local fixup 152->160)
    let u32le = |o: usize| u32::from_le_bytes([le[o],le[o+1],le[o+2],le[o+3]]);
    let n_data = u32le(obj+92) as usize;
    eprintln!("dataBuffer abs={} count={} file_end={}", buf, n_data, le.len());
    eprintln!("buf+count = {} (should be <= {})", buf + n_data, le.len());
    // Interpreting index fields as byte offsets INTO dataBuffer:
    for (name,off) in [("offsetIdx",48),("scaleIdx",52),("bitWidthIdx",56),
                       ("f60",60),("staticMaskIdx",64),("staticDOFsIdx",68),
                       ("numStatic",72),("numDynamic",76),("blockIndexIdx",80),
                       ("blockIndexSize",84)] {
        eprintln!("  obj+{:<3} {:<14} = {}", off, name, u32le(obj+off));
    }
    // The largest index should be < n_data if these are indices into dataBuffer
    // dump first 32 bytes of dataBuffer
    let mut s=String::new();
    for i in 0..32 { s += &format!("{:02x} ", le[buf+i]); }
    eprintln!("  dataBuffer[0..32]: {}", s);
}
