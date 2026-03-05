//! Targeted lane/drawer transition tests for chat session detail layout.
//!
//! Run with: wasm-pack test --headless --chrome

#![cfg(target_arch = "wasm32")]

use adapteros_ui::pages::chat::conversation::{
    next_drawer_kind, next_mobile_lane, ChatDrawerKind, ChatMobileLane,
};
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn drawer_toggle_roundtrip() {
    let open_evidence = next_drawer_kind(None, ChatDrawerKind::Evidence);
    assert_eq!(open_evidence, Some(ChatDrawerKind::Evidence));

    let close_evidence = next_drawer_kind(open_evidence, ChatDrawerKind::Evidence);
    assert_eq!(close_evidence, None);

    let open_context = next_drawer_kind(close_evidence, ChatDrawerKind::Context);
    assert_eq!(open_context, Some(ChatDrawerKind::Context));
}

#[wasm_bindgen_test]
fn drawer_switches_between_kinds() {
    let evidence = next_drawer_kind(None, ChatDrawerKind::Evidence);
    assert_eq!(evidence, Some(ChatDrawerKind::Evidence));

    let context = next_drawer_kind(evidence, ChatDrawerKind::Context);
    assert_eq!(context, Some(ChatDrawerKind::Context));
}

#[wasm_bindgen_test]
fn mobile_lane_selection_transitions() {
    let lane = next_mobile_lane(ChatMobileLane::Conversation, ChatMobileLane::Evidence);
    assert_eq!(lane, ChatMobileLane::Evidence);

    let lane = next_mobile_lane(lane, ChatMobileLane::Context);
    assert_eq!(lane, ChatMobileLane::Context);

    let lane = next_mobile_lane(lane, ChatMobileLane::Conversation);
    assert_eq!(lane, ChatMobileLane::Conversation);
}
