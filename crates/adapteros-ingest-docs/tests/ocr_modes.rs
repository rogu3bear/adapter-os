use adapteros_ingest_docs::{default_ingest_options, DocumentIngestor, OcrMode};
use std::path::PathBuf;

fn fixture_pdf_bytes() -> Vec<u8> {
    // Keep this lightweight and deterministic: a small text-layer PDF fixture.
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/docs/training_overview.pdf");
    std::fs::read(&path)
        .unwrap_or_else(|e| panic!("missing test fixture {}: {}", path.display(), e.to_string()))
}

#[test]
fn pdf_ocr_off_emits_fingerprint_with_mode_off_reason() {
    let bytes = fixture_pdf_bytes();
    let ingestor = DocumentIngestor::new(default_ingest_options(), None);
    let doc = ingestor
        .ingest_pdf_bytes_with_ocr(&bytes, "training_overview.pdf", OcrMode::Off, None)
        .expect("pdf ingest should succeed");

    let fp = doc
        .ocr_fingerprint
        .expect("expected ocr_fingerprint for PDFs");
    assert_eq!(fp.mode, OcrMode::Off);
    assert_eq!(fp.tool.mode, OcrMode::Off);
    assert_eq!(fp.tool.skipped_reason.as_deref(), Some("mode_off"));
}

#[cfg(not(feature = "ocr-external"))]
#[test]
fn pdf_ocr_external_without_feature_records_skip_reason() {
    let bytes = fixture_pdf_bytes();
    let ingestor = DocumentIngestor::new(default_ingest_options(), None);
    let doc = ingestor
        .ingest_pdf_bytes_with_ocr(&bytes, "training_overview.pdf", OcrMode::External, None)
        .expect("pdf ingest should succeed even if external OCR is unavailable (text-layer PDF)");

    let fp = doc
        .ocr_fingerprint
        .expect("expected ocr_fingerprint for PDFs");
    assert_eq!(fp.mode, OcrMode::External);
    assert_eq!(fp.tool.mode, OcrMode::External);
    assert_eq!(
        fp.tool.skipped_reason.as_deref(),
        Some("ocr_external_feature_not_enabled")
    );
}

#[cfg(feature = "ocr-external")]
#[test]
fn pdf_ocr_external_fingerprints_tool_via_path() {
    use adapteros_core::B3Hash;

    struct PathGuard {
        old: Option<String>,
    }

    impl Drop for PathGuard {
        fn drop(&mut self) {
            match self.old.as_ref() {
                Some(v) => std::env::set_var("PATH", v),
                None => std::env::remove_var("PATH"),
            }
        }
    }

    struct CleanupGuard(std::path::PathBuf);

    impl Drop for CleanupGuard {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    let cleanup_root = adapteros_core::rebase_var_path("var/tmp/ocr-test");
    let _cleanup = CleanupGuard(cleanup_root);

    let bin_dir = adapteros_core::rebase_var_path("var/tmp/ocr-test/bin");
    std::fs::create_dir_all(&bin_dir).expect("create var/tmp/ocr-test/bin");

    let tool_path = bin_dir.join("tesseract");
    let script = b"#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then\n  echo \"tesseract 0.0-test\"\n  exit 0\nfi\n# For OCR invocations (not used in this test), output stable text.\necho \"stub-ocr\"\n";
    std::fs::write(&tool_path, script).expect("write stub tesseract");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&tool_path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&tool_path, perms).unwrap();
    }

    let old_path = std::env::var("PATH").ok();
    let _path_guard = PathGuard {
        old: old_path.clone(),
    };
    let old_path = old_path.unwrap_or_default();
    std::env::set_var(
        "PATH",
        format!("{}:{}", bin_dir.to_string_lossy(), old_path),
    );

    let bytes = fixture_pdf_bytes();
    let ingestor = DocumentIngestor::new(default_ingest_options(), None);
    let ocr_root = adapteros_core::rebase_var_path("var/tmp/ocr-test/artifacts");
    std::fs::create_dir_all(&ocr_root).expect("create ocr artifacts root");
    let doc = ingestor
        .ingest_pdf_bytes_with_ocr(
            &bytes,
            "training_overview.pdf",
            OcrMode::External,
            Some(&ocr_root),
        )
        .expect("pdf ingest should succeed");

    let fp = doc
        .ocr_fingerprint
        .expect("expected ocr_fingerprint for PDFs");
    assert_eq!(fp.mode, OcrMode::External);
    assert_eq!(fp.tool.command, "tesseract");
    assert_eq!(fp.tool.version.as_deref(), Some("tesseract 0.0-test"));

    let expected_hash = B3Hash::hash(script).to_hex();
    assert_eq!(
        fp.tool.binary_hash_b3.as_deref(),
        Some(expected_hash.as_str())
    );
    assert!(fp
        .tool
        .binary_path
        .as_deref()
        .unwrap_or("")
        .contains("tesseract"));
}
