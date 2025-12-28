use adapteros_core::B3Hash;

pub fn hash_file(bytes: &[u8]) -> String {
    B3Hash::hash(bytes).to_hex()
}

#[derive(Debug, Clone)]
pub struct DatasetHashInput {
    pub file_name: String,
    pub size_bytes: u64,
    pub file_hash_b3: String,
}

pub fn hash_multi(file_hashes: &[String]) -> String {
    let slices: Vec<&[u8]> = file_hashes.iter().map(|h| h.as_bytes()).collect();
    B3Hash::hash_multi(&slices).to_hex()
}

/// Deterministically hash a dataset manifest using file name, size, and file hash.
/// Files are sorted by name to ensure stable ordering regardless of upload order.
pub fn hash_dataset_manifest(files: &[DatasetHashInput]) -> String {
    let mut entries: Vec<String> = files
        .iter()
        .map(|f| format!("{}:{}:{}", f.file_name, f.size_bytes, f.file_hash_b3))
        .collect();
    entries.sort();
    hash_multi(&entries)
}

#[cfg(test)]
mod tests {
    use super::{hash_dataset_manifest, DatasetHashInput};

    #[test]
    fn dataset_hash_is_stable_across_ordering() {
        let files_a = vec![
            DatasetHashInput {
                file_name: "a.jsonl".into(),
                size_bytes: 10,
                file_hash_b3: "hash-a".into(),
            },
            DatasetHashInput {
                file_name: "b.jsonl".into(),
                size_bytes: 20,
                file_hash_b3: "hash-b".into(),
            },
        ];
        let mut files_b = files_a.clone();
        files_b.swap(0, 1);

        let hash_a = hash_dataset_manifest(&files_a);
        let hash_b = hash_dataset_manifest(&files_b);

        assert_eq!(hash_a, hash_b);
    }
}
