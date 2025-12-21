use adapteros_core::B3Hash;

pub fn hash_multi_bytes(chunks: &[&[u8]]) -> String {
    B3Hash::hash_multi(chunks).to_hex()
}
