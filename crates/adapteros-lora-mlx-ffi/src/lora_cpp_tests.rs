use std::sync::Arc;
use std::thread;

use crate::{
    mlx_array_data, mlx_array_free, mlx_array_size, mlx_model_adapter_remove,
    mlx_model_adapter_upsert_lora, mlx_model_free, mlx_runtime_init_with_device,
    mlx_runtime_is_initialized, MlxDeviceType,
};

#[cfg(test)]
use crate::{
    mlx_test_linear_projection_routed, mlx_test_model_new_empty, mlx_test_model_set_weight_f32,
};

fn ensure_mlx_cpu_runtime() {
    if !mlx_runtime_is_initialized() {
        mlx_runtime_init_with_device(MlxDeviceType::Cpu).expect("mlx init cpu");
    } else {
        // Tests assume CPU determinism. If a prior test initialized GPU, that's a bug in the suite.
        // We don't try to flip devices here since other threads may have active arrays.
    }
}

unsafe fn array_to_vec_f32(array: *mut crate::mlx_array_t) -> Vec<f32> {
    let n = mlx_array_size(array);
    let ptr = mlx_array_data(array);
    assert!(!ptr.is_null());
    std::slice::from_raw_parts(ptr, n).to_vec()
}

struct TestModel(*mut crate::mlx_model_t);
unsafe impl Send for TestModel {}
unsafe impl Sync for TestModel {}

impl Drop for TestModel {
    fn drop(&mut self) {
        unsafe { mlx_model_free(self.0) };
    }
}

fn make_test_model_with_weight(weight_key: &str, w_rows: i32, w_cols: i32, w: &[f32]) -> TestModel {
    ensure_mlx_cpu_runtime();
    let model = unsafe { mlx_test_model_new_empty() };
    assert!(!model.is_null());
    let weight_key = cstr(weight_key);
    let rc = unsafe {
        mlx_test_model_set_weight_f32(model, weight_key.as_ptr(), w.as_ptr(), w_rows, w_cols)
    };
    assert_eq!(rc, 0);
    TestModel(model)
}

fn cstr(s: &str) -> std::ffi::CString {
    std::ffi::CString::new(s).expect("cstr")
}

#[test]
fn mlx_lora_projection_math_and_gate_scaling() {
    // W = I2, x = [2,3]
    // A = [[1,1]] (rank=1), B = [[1],[2]], alpha=1
    // gate is Q15 with denominator 32767.0 (not 32768), so 16384 represents ~0.500015.
    // delta = (x A^T) B^T * gate = (5) * [1,2] * gate
    // y = x W^T + delta
    // linear_projection("foo") looks up "foo.weight"
    let model = make_test_model_with_weight("foo.weight", 2, 2, &[1.0, 0.0, 0.0, 1.0]);

    let a = [1.0f32, 1.0f32]; // [1,2]
    let b = [1.0f32, 2.0f32]; // [2,1]

    let module_name = cstr("foo");
    let rc = unsafe {
        mlx_model_adapter_upsert_lora(
            model.0,
            1,
            module_name.as_ptr(),
            a.as_ptr(),
            1,
            2,
            b.as_ptr(),
            2,
            1,
            1.0,
        )
    };
    assert_eq!(rc, 0);

    let x = [2.0f32, 3.0f32]; // [1,2]
    let adapter_ids = [1u16];
    let gates = [16384i16]; // ~0.5

    let out = unsafe {
        mlx_test_linear_projection_routed(
            model.0,
            x.as_ptr(),
            1,
            2,
            module_name.as_ptr(),
            adapter_ids.as_ptr(),
            gates.as_ptr(),
            1,
        )
    };
    assert!(!out.is_null());
    let out_vec = unsafe { array_to_vec_f32(out) };
    unsafe { mlx_array_free(out) };

    assert_eq!(out_vec.len(), 2);
    let gate = 16384.0f32 / 32767.0f32;
    let expected0 = 2.0f32 + 5.0f32 * gate;
    let expected1 = 3.0f32 + 10.0f32 * gate;
    assert!(
        (out_vec[0] - expected0).abs() < 1e-3,
        "out_vec={out_vec:?} expected0={expected0}"
    );
    assert!(
        (out_vec[1] - expected1).abs() < 1e-3,
        "out_vec={out_vec:?} expected1={expected1}"
    );
}

#[test]
fn mlx_lora_missing_module_is_noop() {
    let model = make_test_model_with_weight("foo.weight", 2, 2, &[1.0, 0.0, 0.0, 1.0]);

    // Load adapter weights under a different module name.
    let a = [1.0f32, 1.0f32];
    let b = [1.0f32, 2.0f32];

    let rc = unsafe {
        mlx_model_adapter_upsert_lora(
            model.0,
            7,
            cstr("bar").as_ptr(),
            a.as_ptr(),
            1,
            2,
            b.as_ptr(),
            2,
            1,
            1.0,
        )
    };
    assert_eq!(rc, 0);

    let x = [2.0f32, 3.0f32];
    let adapter_ids = [7u16];
    let gates = [32767i16];

    let out = unsafe {
        mlx_test_linear_projection_routed(
            model.0,
            x.as_ptr(),
            1,
            2,
            cstr("foo").as_ptr(),
            adapter_ids.as_ptr(),
            gates.as_ptr(),
            1,
        )
    };
    assert!(!out.is_null());
    let out_vec = unsafe { array_to_vec_f32(out) };
    unsafe { mlx_array_free(out) };

    assert_eq!(out_vec, vec![2.0, 3.0]);
}

#[test]
fn mlx_lora_cpu_determinism_byte_equal() {
    let model = make_test_model_with_weight("foo.weight", 2, 2, &[1.0, 0.0, 0.0, 1.0]);

    let a = [1.0f32, 1.0f32];
    let b = [1.0f32, 2.0f32];
    let rc = unsafe {
        mlx_model_adapter_upsert_lora(
            model.0,
            1,
            cstr("foo").as_ptr(),
            a.as_ptr(),
            1,
            2,
            b.as_ptr(),
            2,
            1,
            1.0,
        )
    };
    assert_eq!(rc, 0);

    let x = [2.0f32, 3.0f32];
    let adapter_ids = [1u16];
    let gates = [16384i16];

    let run_once = || {
        let out = unsafe {
            mlx_test_linear_projection_routed(
                model.0,
                x.as_ptr(),
                1,
                2,
                cstr("foo").as_ptr(),
                adapter_ids.as_ptr(),
                gates.as_ptr(),
                1,
            )
        };
        assert!(!out.is_null());
        let vec = unsafe { array_to_vec_f32(out) };
        unsafe { mlx_array_free(out) };
        vec.into_iter().map(|f| f.to_bits()).collect::<Vec<u32>>()
    };

    let a = run_once();
    let b = run_once();
    assert_eq!(a, b);
}

#[test]
fn mlx_lora_hotswap_concurrency_no_crash() {
    let model = Arc::new(make_test_model_with_weight(
        "foo.weight",
        2,
        2,
        &[1.0, 0.0, 0.0, 1.0],
    ));

    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));

    let readers: Vec<_> = (0..4)
        .map(|_| {
            let model = model.clone();
            let stop = stop.clone();
            thread::spawn(move || {
                let x = [2.0f32, 3.0f32];
                let adapter_ids = [42u16];
                let gates = [32767i16];
                while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                    let out = unsafe {
                        mlx_test_linear_projection_routed(
                            model.0,
                            x.as_ptr(),
                            1,
                            2,
                            cstr("foo").as_ptr(),
                            adapter_ids.as_ptr(),
                            gates.as_ptr(),
                            1,
                        )
                    };
                    assert!(!out.is_null());
                    let v = unsafe { array_to_vec_f32(out) };
                    unsafe { mlx_array_free(out) };
                    assert!(v.iter().all(|x| x.is_finite()));
                }
            })
        })
        .collect();

    let writer = {
        let model = model.clone();
        thread::spawn(move || {
            let a = [1.0f32, 1.0f32];
            let b = [1.0f32, 2.0f32];
            for _ in 0..200 {
                let rc = unsafe {
                    mlx_model_adapter_upsert_lora(
                        model.0,
                        42,
                        cstr("foo").as_ptr(),
                        a.as_ptr(),
                        1,
                        2,
                        b.as_ptr(),
                        2,
                        1,
                        1.0,
                    )
                };
                assert_eq!(rc, 0);
                let rc = unsafe { mlx_model_adapter_remove(model.0, 42) };
                assert_eq!(rc, 0);
            }
        })
    };

    writer.join().unwrap();
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    for t in readers {
        t.join().unwrap();
    }
}
