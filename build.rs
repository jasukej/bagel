//! Link against the system xxHash library, or fall back on 
//! otherwise, fall back to SHA-256.
fn main() {
    println!("cargo::rustc-check-cfg=cfg(has_xxhash)");

    match pkg_config::Config::new().atleast_version("0.8").probe("libxxhash") {
        // probe() has already printed the link-lib lines
        Ok(_) => println!("cargo::rustc-cfg=has_xxhash"),
        Err(_) => println!(
            "cargo::warning=libxxhash not found, falling back to SHA-256 for file hashing"
        ),
    }
}
