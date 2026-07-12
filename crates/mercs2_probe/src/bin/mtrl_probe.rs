//! Dump each MTRL material's flags@104 + a few preamble floats + texture hashes, to find what
//! distinguishes an intact material from a ruin/damage one (both share a PRMG group as PRMT strips).
use mercs2_engine::wad;
fn u16(b:&[u8],o:usize)->u16{u16::from_le_bytes([b[o],b[o+1]])}
fn u32(b:&[u8],o:usize)->u32{u32::from_le_bytes([b[o],b[o+1],b[o+2],b[o+3]])}
fn f32(b:&[u8],o:usize)->f32{f32::from_le_bytes([b[o],b[o+1],b[o+2],b[o+3]])}
fn main(){
    let name=std::env::args().nth(1).unwrap_or_else(||"ch_veh_tank_ztz98".into());
    let hash=name.strip_prefix("0x").and_then(|h|u32::from_str_radix(h,16).ok())
        .unwrap_or_else(||mercs2_formats::hash::pandemic_hash_m2(name.trim_start_matches('_')));
    let mut w=wad::registry_vz_wad().and_then(|p|wad::open(&p).ok()).unwrap();
    let c=wad::extract_container(&mut w,hash).unwrap();
    // find MTRL leaf
    let data_off=u32(&c,4) as usize; let ndesc=u32(&c,16) as usize;
    let mut mtrl=None;
    for i in 0..ndesc { let ro=20+i*20; if &c[ro..ro+4]==b"MTRL"{ let u0=u32(&c,ro+4); if u0!=0xFFFFFFFF { mtrl=Some((data_off+u0 as usize, u32(&c,ro+8) as usize)); break; } } }
    let Some((s,sz))=mtrl else { return println!("no MTRL") };
    let body=&c[s..s+sz];
    println!("{name}: MTRL {sz} bytes");
    let mut p=0; let mut idx=0;
    while p+108<=body.len(){
        let flags=u16(body,p+104); let tc=u16(body,p+106) as usize;
        if tc==0||tc>10 {break}
        if p+108+tc*4>body.len(){break}
        let texs:Vec<String>=(0..tc).map(|k|format!("{:#010x}",u32(body,p+108+k*4))).collect();
        // a few preamble floats (diffuse rgba? at 0, alpha somewhere)
        let pre:Vec<String>=(0..8).map(|k|format!("{:.2}",f32(body,p+k*4))).collect();
        println!("  mat{idx:2} flags={flags:#06x} tc={tc} tex={texs:?} pre0-7=[{}]", pre.join(","));
        p+=116+tc*4; idx+=1;
    }
}
