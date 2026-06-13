//! Cargo build script for compiling native C dependencies
fn main() {
    cc::Build::new().file("vendor/xxhash/xxhash.c").opt_level(3).compile("xxhash");

    println!("cargo:rerun-if-changed=vendor/xxhash/xxhash.c");
    println!("cargo:rerun-if-changed=vendor/xxhash/xxhash.h");
}
