use adapteros_core::B3Hash;
use unicode_normalization::UnicodeNormalization;

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

/// Normalize filename for consistent hashing across platforms/encodings.
/// Applies: trim whitespace, Unicode NFD normalization, lowercase.
pub fn normalize_filename(name: &str) -> String {
    name.trim().nfd().collect::<String>().to_lowercase()
}

/// Deterministically hash a dataset manifest using normalized file name, size, and file hash.
/// Files are sorted by normalized name to ensure stable ordering regardless of upload order
/// or filename case/encoding differences.
pub fn hash_dataset_manifest(files: &[DatasetHashInput]) -> String {
    let mut entries: Vec<String> = files
        .iter()
        .map(|f| {
            let normalized_name = normalize_filename(&f.file_name);
            format!("{}:{}:{}", normalized_name, f.size_bytes, f.file_hash_b3)
        })
        .collect();
    entries.sort();
    hash_multi(&entries)
}

#[cfg(test)]
mod tests {
    use super::{hash_dataset_manifest, normalize_filename, DatasetHashInput};

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

    #[test]
    fn normalize_filename_trims_whitespace() {
        assert_eq!(normalize_filename("  test.jsonl  "), "test.jsonl");
        assert_eq!(normalize_filename("\t data.txt \n"), "data.txt");
    }

    #[test]
    fn normalize_filename_lowercases() {
        assert_eq!(normalize_filename("Test.JSONL"), "test.jsonl");
        assert_eq!(normalize_filename("DATA.TXT"), "data.txt");
        assert_eq!(normalize_filename("MixedCase.Json"), "mixedcase.json");
    }

    #[test]
    fn normalize_filename_nfd_unicode() {
        // NFD: é = e + combining acute accent (U+0301)
        // NFC: é = single codepoint (U+00E9)
        // Both should normalize to the same NFD form
        let nfc = "café.txt"; // using precomposed é
        let nfd = "cafe\u{0301}.txt"; // using e + combining accent

        assert_eq!(normalize_filename(nfc), normalize_filename(nfd));
    }

    #[test]
    fn manifest_hash_is_case_insensitive() {
        let files_upper = vec![DatasetHashInput {
            file_name: "DATA.JSONL".into(),
            size_bytes: 100,
            file_hash_b3: "abc123".into(),
        }];
        let files_lower = vec![DatasetHashInput {
            file_name: "data.jsonl".into(),
            size_bytes: 100,
            file_hash_b3: "abc123".into(),
        }];

        let hash_upper = hash_dataset_manifest(&files_upper);
        let hash_lower = hash_dataset_manifest(&files_lower);

        assert_eq!(hash_upper, hash_lower);
    }

    #[test]
    fn manifest_hash_is_whitespace_insensitive() {
        let files_trimmed = vec![DatasetHashInput {
            file_name: "data.jsonl".into(),
            size_bytes: 100,
            file_hash_b3: "abc123".into(),
        }];
        let files_padded = vec![DatasetHashInput {
            file_name: "  data.jsonl  ".into(),
            size_bytes: 100,
            file_hash_b3: "abc123".into(),
        }];

        let hash_trimmed = hash_dataset_manifest(&files_trimmed);
        let hash_padded = hash_dataset_manifest(&files_padded);

        assert_eq!(hash_trimmed, hash_padded);
    }
}
