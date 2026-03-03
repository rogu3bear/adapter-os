use adapteros_lora_worker::worker_utilities::{
    apply_runtime_model_load_transition, apply_runtime_model_unload_transition,
};
use adapteros_lora_worker::WorkerModelRuntimeState;

fn make_temp_model_path(label: &str) -> String {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("unix epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("aos-model-switch-{}-{}", label, unique));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let path = dir.join("model.safetensors");
    std::fs::write(&path, b"stub-model").expect("write model file");
    path.to_string_lossy().to_string()
}

#[test]
fn load_transition_sets_ready_state_and_is_idempotent() {
    let mut state = WorkerModelRuntimeState::default();
    let model_path = make_temp_model_path("load");

    apply_runtime_model_load_transition(&mut state, "model-a", &model_path)
        .expect("first load succeeds");

    assert_eq!(state.status, "ready");
    assert_eq!(state.active_model_id.as_deref(), Some("model-a"));
    assert!(state.active_model_hash.is_some());
    assert_eq!(state.generation, 1);
    assert!(state.last_error.is_none());

    let hash_before = state.active_model_hash.clone();
    apply_runtime_model_load_transition(&mut state, "model-a", &model_path)
        .expect("idempotent reload succeeds");

    assert_eq!(state.status, "ready");
    assert_eq!(state.active_model_id.as_deref(), Some("model-a"));
    assert_eq!(state.active_model_hash, hash_before);
    assert_eq!(
        state.generation, 1,
        "idempotent reload must not bump generation"
    );
}

#[test]
fn unload_transition_clears_active_model_state() {
    let mut state = WorkerModelRuntimeState::default();
    let model_path = make_temp_model_path("unload");

    apply_runtime_model_load_transition(&mut state, "model-a", &model_path).expect("load succeeds");
    apply_runtime_model_unload_transition(&mut state);

    assert_eq!(state.status, "no-model");
    assert!(state.active_model_id.is_none());
    assert!(state.active_model_hash.is_none());
    assert!(state.last_error.is_none());
    assert_eq!(state.generation, 2);
}

#[test]
fn failed_switch_keeps_previous_ready_model_active() {
    let mut state = WorkerModelRuntimeState::default();
    let model_path = make_temp_model_path("switch-ok");

    apply_runtime_model_load_transition(&mut state, "model-a", &model_path)
        .expect("initial load succeeds");
    let generation_before = state.generation;

    let err = apply_runtime_model_load_transition(
        &mut state,
        "model-b",
        "/path/that/does/not/exist/model.safetensors",
    )
    .expect_err("switch to missing model must fail");

    assert!(err.to_string().contains("does not exist"));
    assert_eq!(state.status, "ready");
    assert_eq!(state.active_model_id.as_deref(), Some("model-a"));
    assert_eq!(state.generation, generation_before + 1);
    assert!(state.last_error.is_some());
}

#[test]
fn failed_initial_load_sets_error_when_no_previous_model_exists() {
    let mut state = WorkerModelRuntimeState::default();

    let _ = apply_runtime_model_load_transition(
        &mut state,
        "model-a",
        "/path/that/does/not/exist/model.safetensors",
    )
    .expect_err("initial missing model load must fail");

    assert_eq!(state.status, "error");
    assert!(state.active_model_id.is_none());
    assert!(state.active_model_hash.is_none());
    assert_eq!(state.generation, 1);
    assert!(state.last_error.is_some());
}
