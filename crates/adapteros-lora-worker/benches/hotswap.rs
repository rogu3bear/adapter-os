use adapteros_lora_worker::adapter_hotswap::{AdapterTable, B3Hash};
use criterion::{criterion_group, criterion_main, Criterion};

fn bench_hotswap(c: &mut Criterion) {
    let table = AdapterTable::new();
    let h = B3Hash::zero();
    table.preload("test".to_string(), h, 10).unwrap();
    table.swap(&["test".to_string()], &[]).unwrap();

    c.bench_function("hotswap_inc_dec", |b| {
        b.iter(|| {
            table.inc_ref("test");
            table.dec_ref("test");
        });
    });

    c.bench_function("hotswap_full_cycle", |b| {
        b.iter(|| {
            let new_h = B3Hash::zero();
            table.preload("new".to_string(), new_h, 10).unwrap();
            table
                .swap(&["new".to_string()], &["test".to_string()])
                .unwrap();
            table.inc_ref("new");
            table.dec_ref("new");
        });
    });
}

criterion_group!(benches, bench_hotswap);
criterion_main!(benches);


