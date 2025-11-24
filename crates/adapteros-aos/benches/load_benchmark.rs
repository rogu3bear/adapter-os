#[cfg(feature = "mmap")]
use adapteros_aos::MmapAdapterLoader;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::path::PathBuf;

fn find_test_adapter() -> Option<PathBuf> {
    let adapters_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()?
        .parent()?
        .join("adapters");

    if !adapters_dir.exists() {
        return None;
    }

    std::fs::read_dir(adapters_dir)
        .ok()?
        .filter_map(|entry| entry.ok())
        .find(|entry| entry.path().extension().and_then(|s| s.to_str()) == Some("aos"))
        .map(|entry| entry.path())
}

#[cfg(feature = "mmap")]
fn benchmark_load(c: &mut Criterion) {
    let Some(adapter_path) = find_test_adapter() else {
        eprintln!("No .aos files found, skipping benchmark");
        return;
    };

    let runtime = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("load", |b| {
        b.to_async(&runtime).iter(|| async {
            let loader = MmapAdapterLoader::new();
            let result = loader.load(black_box(&adapter_path)).await;
            black_box(result)
        });
    });
}

#[cfg(not(feature = "mmap"))]
fn benchmark_load(_c: &mut Criterion) {
    eprintln!("mmap feature not enabled, skipping benchmark");
}

criterion_group!(benches, benchmark_load);
criterion_main!(benches);
