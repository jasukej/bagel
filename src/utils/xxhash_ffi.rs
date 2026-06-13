unsafe extern "C" {
    // *const u8 is the Rust equivalent of const void*
    fn XXH64(input: *const u8, length: usize, seed: u64) -> u64;
}

pub fn xxhash64(data: &[u8]) -> u64 {
    unsafe { XXH64(data.as_ptr(), data.len(), 0) } 
}

pub fn xxhash_file(path: &std::path::Path) -> std::io::Result<u64> {
    let data = std::fs::read(path)?;
    Ok(xxhash64(&data))
}