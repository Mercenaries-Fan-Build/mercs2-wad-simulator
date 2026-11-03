//! Print `pmc_sites.gen.h`, the site table pmc_bb's C scanner compiles, from `mercs2_formats::pmc_sites`.
//! Run it into the pmc_bb repository: `cargo run -p mercs2_probe --bin pmc_sites_header > pmc_sites.gen.h`.

fn main() {
    print!("{}", mercs2_formats::pmc_sites::c_header());
}
