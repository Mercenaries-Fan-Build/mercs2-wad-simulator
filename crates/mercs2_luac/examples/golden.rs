//! Compile a `.lua` file to LuaQ and write it out, so the bytes can be diffed against the
//! reference `tools/lua51-mercs2/luac.exe` — the toolchain already proven to produce
//! bytecode the game loads.
//!
//! ```text
//! cargo run -p mercs2_luac --example golden -- in.lua out.luac
//! ```

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: golden <in.lua> <out.luac>");
        std::process::exit(2);
    }
    let src = std::fs::read_to_string(&args[1]).expect("read source");

    // `luac.exe` names the chunk "@<path>" and bakes that into the debug info, so match it
    // exactly or the bytes differ for a reason that has nothing to do with codegen.
    let chunk = format!("@{}", args[1]);

    let out = mercs2_luac::compile(&src, &chunk).expect("compile");
    std::fs::write(&args[2], &out).expect("write");
    eprintln!("wrote {} bytes to {}", out.len(), args[2]);
}
