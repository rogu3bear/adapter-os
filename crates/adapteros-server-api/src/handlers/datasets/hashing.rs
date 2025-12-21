use adapteros_core::B3Hash;

pub fn hash_file(bytes: &[u8]) -> String {
    B3Hash::hash(bytes).to_hex()
}

pub fn hash_multi(file_hashes: &[String]) -> String {
    let slices: Vec<&[u8]> = file_hashes.iter().map(|h| h.as_bytes()).collect();
    B3Hash::hash_multi(&slices).to_hex()
}
