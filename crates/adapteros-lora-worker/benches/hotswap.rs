use adapteros_core::B3Hash;
use adapteros_lora_worker::adapter_hotswap::AdapterTable;
use criterion::{criterion_group, criterion_main, Criterion};
use std::sync::Arc;
use tokio::runtime::Runtime;

fn bench_hotswap(c: &mut Criterion) {
    let rt = Arc::new(Runtime::new().unwrap());
    let table = AdapterTable::new();
    let h = B3Hash::zero();
    rt.block_on(async {
        table.preload("test".to_string(), h, 10).await.unwrap();
        table.swap(&["test".to_string()], &[]).await.unwrap();
    });

    let rt_clone1 = rt.clone();
    c.bench_function("hotswap_inc_dec", |b| {
        b.iter(|| {
            rt_clone1.block_on(async {
                table.inc_ref("test").await;
                table.dec_ref("test").await;
            });
        });
    });

    let rt_clone2 = rt.clone();
    c.bench_function("hotswap_full_cycle", |b| {
        b.iter(|| {
            rt_clone2.block_on(async {
                let new_h = B3Hash::zero();
                table.preload("new".to_string(), new_h, 10).await.unwrap();
                table
                    .swap(&["new".to_string()], &["test".to_string()])
                    .await
                    .unwrap();
                table.inc_ref("new").await;
                table.dec_ref("new").await;
            });
        });
    });
}

criterion_group!(benches, bench_hotswap);
criterion_main!(benches);
