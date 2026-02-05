# Constellation Landing Page Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a spatial, chat-first landing page where users navigate a constellation of their work through conversation.

**Architecture:** SVG-based canvas for the constellation view, integrated with existing chat signals. New state module for constellation positioning and user interaction level. Progressive disclosure controlled by session count stored in localStorage.

**Tech Stack:** Leptos 0.7, SVG for constellation rendering, existing chat signals, CSS custom properties for Calm Glass variant.

---

## Task 1: Add Calm Glass CSS Tokens

**Files:**
- Modify: `crates/adapteros-ui/dist/glass.css`

**Step 1: Add calm glass tokens to :root**

Add after line ~70 (after `--glass-blur-3`):

```css
    /* Calm Glass variant (constellation view - higher translucency, no noise) */
    --glass-bg-calm: hsla(0, 0%, 100%, 0.88);
    --glass-blur-calm: 20px;
    --glass-border-calm: hsla(0, 0%, 100%, 0.15);
    --transition-drift: 500ms cubic-bezier(0.4, 0, 0.2, 1);
```

**Step 2: Add dark mode calm tokens**

Add after line ~99 (after dark mode `--glass-glow`):

```css
    /* Calm Glass variant (dark mode) */
    --glass-bg-calm: hsla(222, 47%, 8%, 0.90);
    --glass-blur-calm: 24px;
    --glass-border-calm: hsla(215, 30%, 40%, 0.12);
```

**Step 3: Verify CSS is valid**

Run: `trunk build --release 2>&1 | head -20`
Expected: Build proceeds (CSS is bundled)

**Step 4: Commit**

```bash
git add crates/adapteros-ui/dist/glass.css
git commit -m "style: add calm glass tokens for constellation view"
```

---

## Task 2: Create Constellation State Module

**Files:**
- Create: `crates/adapteros-ui/src/signals/constellation.rs`
- Modify: `crates/adapteros-ui/src/signals/mod.rs`

**Step 1: Create constellation state file**

```rust
//! Constellation state management
//!
//! Spatial state for the constellation landing view.

use leptos::prelude::*;
use serde::{Deserialize, Serialize};

/// LocalStorage key for user interaction level
const INTERACTION_LEVEL_KEY: &str = "adapteros_constellation_level";
/// LocalStorage key for session count
const SESSION_COUNT_KEY: &str = "adapteros_constellation_sessions";
/// LocalStorage key for camera position
const CAMERA_POSITION_KEY: &str = "adapteros_constellation_camera";
/// LocalStorage key for pinned quicklinks
const QUICKLINKS_KEY: &str = "adapteros_constellation_quicklinks";

/// Progressive disclosure level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum InteractionLevel {
    /// Conversation only - no interactive nodes
    #[default]
    ConversationOnly,
    /// Soft hints - hover states, clickable nodes
    SoftHints,
    /// Full spatial - drag, pan, zoom, keyboard nav
    FullSpatial,
}

impl InteractionLevel {
    /// Determine level from session count
    pub fn from_session_count(count: u32) -> Self {
        match count {
            0..=2 => Self::ConversationOnly,
            3..=9 => Self::SoftHints,
            _ => Self::FullSpatial,
        }
    }
}

/// Camera position in the constellation
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct CameraPosition {
    pub x: f64,
    pub y: f64,
    pub zoom: f64,
}

impl CameraPosition {
    pub fn center() -> Self {
        Self { x: 0.0, y: 0.0, zoom: 1.0 }
    }
}

/// A node in the constellation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConstellationNode {
    pub id: String,
    pub title: String,
    pub node_type: NodeType,
    pub position: (f64, f64),
    pub size: f64,
    pub last_accessed: Option<String>,
}

/// Type of artifact a node represents
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeType {
    Document,
    Dataset,
    Adapter,
    TrainingRun,
    ChatSession,
    Collection,
}

impl NodeType {
    pub fn icon_path(&self) -> &'static str {
        match self {
            Self::Document => "M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z",
            Self::Dataset => "M4 7v10c0 2.21 3.582 4 8 4s8-1.79 8-4V7M4 7c0 2.21 3.582 4 8 4s8-1.79 8-4M4 7c0-2.21 3.582-4 8-4s8 1.79 8 4",
            Self::Adapter => "M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z",
            Self::TrainingRun => "M13 10V3L4 14h7v7l9-11h-7z",
            Self::ChatSession => "M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z",
            Self::Collection => "M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10",
        }
    }
}

/// A connection between nodes
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConstellationConnection {
    pub from_id: String,
    pub to_id: String,
    pub strength: f64, // 0.0 - 1.0, affects opacity
}

/// Quicklink for fast navigation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Quicklink {
    pub id: String,
    pub label: String,
    pub icon_path: String,
    pub action: QuicklinkAction,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum QuicklinkAction {
    Navigate(String),
    NewChat,
    ShowRecent,
}

/// Full constellation state
#[derive(Debug, Clone, Default)]
pub struct ConstellationState {
    pub nodes: Vec<ConstellationNode>,
    pub connections: Vec<ConstellationConnection>,
    pub camera: CameraPosition,
    pub focused_node: Option<String>,
    pub interaction_level: InteractionLevel,
    pub quicklinks: Vec<Quicklink>,
    pub is_at_center: bool,
}

/// Actions for constellation state
#[derive(Clone)]
pub struct ConstellationAction {
    state: RwSignal<ConstellationState>,
}

impl ConstellationAction {
    /// Navigate to a node by ID
    pub fn navigate_to(&self, node_id: &str) {
        self.state.update(|s| {
            if let Some(node) = s.nodes.iter().find(|n| n.id == node_id) {
                s.camera.x = node.position.0;
                s.camera.y = node.position.1;
                s.focused_node = Some(node_id.to_string());
                s.is_at_center = false;
            }
        });
    }

    /// Return to center
    pub fn return_to_center(&self) {
        self.state.update(|s| {
            s.camera = CameraPosition::center();
            s.focused_node = None;
            s.is_at_center = true;
        });
    }

    /// Set nodes (from API response)
    pub fn set_nodes(&self, nodes: Vec<ConstellationNode>, connections: Vec<ConstellationConnection>) {
        self.state.update(|s| {
            s.nodes = nodes;
            s.connections = connections;
        });
    }

    /// Increment session count and update interaction level
    pub fn increment_session(&self) {
        let count = load_session_count() + 1;
        save_session_count(count);
        self.state.update(|s| {
            s.interaction_level = InteractionLevel::from_session_count(count);
        });
    }
}

/// Context type for providing constellation state
pub type ConstellationContext = (Signal<ConstellationState>, ConstellationAction);

/// Provide constellation context
pub fn provide_constellation_context() {
    let session_count = load_session_count();
    let interaction_level = InteractionLevel::from_session_count(session_count);
    let camera = load_camera_position().unwrap_or_else(CameraPosition::center);
    let quicklinks = load_quicklinks().unwrap_or_else(default_quicklinks);

    let state = RwSignal::new(ConstellationState {
        nodes: vec![],
        connections: vec![],
        camera,
        focused_node: None,
        interaction_level,
        quicklinks,
        is_at_center: true,
    });

    let action = ConstellationAction { state };

    provide_context((Signal::from(state), action));
}

/// Use constellation context
pub fn use_constellation() -> ConstellationContext {
    expect_context::<ConstellationContext>()
}

// LocalStorage helpers

fn load_session_count() -> u32 {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item(SESSION_COUNT_KEY).ok().flatten())
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

fn save_session_count(count: u32) {
    if let Some(storage) = web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
    {
        let _ = storage.set_item(SESSION_COUNT_KEY, &count.to_string());
    }
}

fn load_camera_position() -> Option<CameraPosition> {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item(CAMERA_POSITION_KEY).ok().flatten())
        .and_then(|v| serde_json::from_str(&v).ok())
}

fn load_quicklinks() -> Option<Vec<Quicklink>> {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item(QUICKLINKS_KEY).ok().flatten())
        .and_then(|v| serde_json::from_str(&v).ok())
}

fn default_quicklinks() -> Vec<Quicklink> {
    vec![
        Quicklink {
            id: "new-chat".to_string(),
            label: "New Chat".to_string(),
            icon_path: "M12 4v16m8-8H4".to_string(),
            action: QuicklinkAction::NewChat,
        },
        Quicklink {
            id: "recent".to_string(),
            label: "Recent".to_string(),
            icon_path: "M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z".to_string(),
            action: QuicklinkAction::ShowRecent,
        },
        Quicklink {
            id: "datasets".to_string(),
            label: "Datasets".to_string(),
            icon_path: "M4 7v10c0 2.21 3.582 4 8 4s8-1.79 8-4V7".to_string(),
            action: QuicklinkAction::Navigate("/datasets".to_string()),
        },
        Quicklink {
            id: "adapters".to_string(),
            label: "Adapters".to_string(),
            icon_path: "M10.325 4.317c.426-1.756 2.924-1.756 3.35 0".to_string(),
            action: QuicklinkAction::Navigate("/adapters".to_string()),
        },
    ]
}
```

**Step 2: Export from signals mod.rs**

Add to `crates/adapteros-ui/src/signals/mod.rs`:

```rust
pub mod constellation;

pub use constellation::{
    provide_constellation_context, use_constellation, CameraPosition, ConstellationAction,
    ConstellationConnection, ConstellationContext, ConstellationNode, ConstellationState,
    InteractionLevel, NodeType, Quicklink, QuicklinkAction,
};
```

**Step 3: Verify compilation**

Run: `cargo check -p adapteros-ui --target wasm32-unknown-unknown`
Expected: Compiles without errors

**Step 4: Commit**

```bash
git add crates/adapteros-ui/src/signals/constellation.rs crates/adapteros-ui/src/signals/mod.rs
git commit -m "feat(ui): add constellation state management"
```

---

## Task 3: Create Constellation CSS

**Files:**
- Create: `crates/adapteros-ui/dist/constellation.css`
- Modify: `crates/adapteros-ui/index.html`

**Step 1: Create constellation.css**

```css
/*
 * Constellation Landing View
 * Spatial, chat-first landing experience with calm glass aesthetic
 */

/* ===== Constellation Container ===== */
.constellation {
    position: relative;
    width: 100%;
    height: 100%;
    overflow: hidden;
    background: var(--color-background);
}

/* Depth gradient - focus toward center */
.constellation::before {
    content: '';
    position: absolute;
    inset: 0;
    background: radial-gradient(
        ellipse at center,
        transparent 0%,
        transparent 30%,
        hsla(222, 47%, 4%, 0.3) 100%
    );
    pointer-events: none;
    z-index: 1;
}

.dark .constellation::before {
    background: radial-gradient(
        ellipse at center,
        transparent 0%,
        transparent 30%,
        hsla(222, 47%, 2%, 0.5) 100%
    );
}

/* ===== SVG Canvas ===== */
.constellation-canvas {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
}

/* ===== Connections ===== */
.constellation-connection {
    stroke: var(--glass-border-calm);
    stroke-width: 1;
    fill: none;
    transition: opacity var(--transition-drift);
}

.constellation-connection:hover {
    stroke: var(--color-primary);
    opacity: 0.6;
}

/* ===== Nodes ===== */
.constellation-node {
    cursor: default;
    transition: transform var(--transition-drift), opacity var(--transition-drift);
}

/* Level 2+: Nodes become interactive */
.constellation--interactive .constellation-node {
    cursor: pointer;
}

.constellation-node-bg {
    fill: var(--glass-bg-calm);
    stroke: var(--glass-border-calm);
    stroke-width: 1;
    filter: url(#constellation-blur);
}

.constellation-node:hover .constellation-node-bg {
    fill: var(--glass-bg-2);
    stroke: var(--color-primary);
    stroke-width: 1.5;
}

.constellation-node--focused .constellation-node-bg {
    stroke: var(--color-primary);
    stroke-width: 2;
    filter: url(#constellation-glow);
}

.constellation-node-icon {
    stroke: var(--color-muted-foreground);
    stroke-width: 1.5;
    fill: none;
}

.constellation-node:hover .constellation-node-icon {
    stroke: var(--color-foreground);
}

.constellation-node-label {
    fill: var(--color-foreground);
    font-size: 11px;
    font-weight: 500;
    text-anchor: middle;
    opacity: 0;
    transition: opacity var(--transition-drift);
}

.constellation-node:hover .constellation-node-label {
    opacity: 1;
}

/* ===== Center Area ===== */
.constellation-center {
    position: absolute;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 1.5rem;
    z-index: 10;
    max-width: 600px;
    width: 90%;
}

/* ===== Center Input ===== */
.constellation-input-wrapper {
    width: 100%;
    position: relative;
}

.constellation-input {
    width: 100%;
    padding: 1rem 1.25rem;
    font-size: 1rem;
    line-height: 1.5;
    color: var(--color-foreground);
    background: var(--glass-bg-calm);
    backdrop-filter: blur(var(--glass-blur-calm));
    -webkit-backdrop-filter: blur(var(--glass-blur-calm));
    border: 1px solid transparent;
    border-radius: 1rem;
    outline: none;
    transition:
        border-color 200ms ease,
        box-shadow 200ms ease,
        background 200ms ease;
}

.constellation-input::placeholder {
    color: var(--color-muted-foreground);
}

.constellation-input:focus {
    border-color: var(--glass-border-calm);
    box-shadow: 0 0 0 4px hsla(222, 47%, 50%, 0.1);
}

.dark .constellation-input:focus {
    box-shadow: 0 0 0 4px hsla(217, 91%, 60%, 0.15);
}

/* ===== Welcome Message ===== */
.constellation-welcome {
    text-align: center;
    color: var(--color-muted-foreground);
    font-size: 0.9375rem;
    line-height: 1.6;
    max-width: 400px;
}

/* ===== Quicklinks ===== */
.constellation-quicklinks {
    display: flex;
    gap: 0.5rem;
    justify-content: center;
    flex-wrap: wrap;
}

.constellation-quicklink {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 2.5rem;
    height: 2.5rem;
    border-radius: 50%;
    background: var(--glass-bg-calm);
    backdrop-filter: blur(var(--glass-blur-calm));
    -webkit-backdrop-filter: blur(var(--glass-blur-calm));
    border: 1px solid var(--glass-border-calm);
    color: var(--color-muted-foreground);
    cursor: pointer;
    transition:
        background 200ms ease,
        border-color 200ms ease,
        color 200ms ease,
        transform 200ms ease;
}

.constellation-quicklink:hover {
    background: var(--glass-bg-2);
    border-color: var(--glass-border);
    color: var(--color-foreground);
    transform: scale(1.05);
}

.constellation-quicklink:focus-visible {
    outline: 2px solid var(--color-ring);
    outline-offset: 2px;
}

.constellation-quicklink svg {
    width: 1.125rem;
    height: 1.125rem;
}

/* Quicklink tooltip */
.constellation-quicklink[data-tooltip]::after {
    content: attr(data-tooltip);
    position: absolute;
    bottom: calc(100% + 0.5rem);
    left: 50%;
    transform: translateX(-50%);
    padding: 0.375rem 0.625rem;
    font-size: 0.75rem;
    white-space: nowrap;
    background: var(--glass-bg-3);
    backdrop-filter: blur(var(--glass-blur-3));
    -webkit-backdrop-filter: blur(var(--glass-blur-3));
    border: 1px solid var(--glass-border);
    border-radius: 0.375rem;
    color: var(--color-foreground);
    opacity: 0;
    pointer-events: none;
    transition: opacity 150ms ease;
}

.constellation-quicklink:hover[data-tooltip]::after,
.constellation-quicklink:focus[data-tooltip]::after {
    opacity: 1;
}

/* ===== Return to Center Button ===== */
.constellation-home-btn {
    position: absolute;
    bottom: 1.5rem;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.5rem 1rem;
    font-size: 0.8125rem;
    color: var(--color-muted-foreground);
    background: var(--glass-bg-calm);
    backdrop-filter: blur(var(--glass-blur-calm));
    -webkit-backdrop-filter: blur(var(--glass-blur-calm));
    border: 1px solid var(--glass-border-calm);
    border-radius: 2rem;
    cursor: pointer;
    opacity: 0;
    transition:
        opacity var(--transition-drift),
        background 200ms ease,
        color 200ms ease;
    z-index: 10;
}

.constellation-home-btn--visible {
    opacity: 1;
}

.constellation-home-btn:hover {
    background: var(--glass-bg-2);
    color: var(--color-foreground);
}

.constellation-home-btn svg {
    width: 1rem;
    height: 1rem;
}

/* ===== Ghost Nodes (suggestions) ===== */
.constellation-ghost {
    opacity: 0.5;
    cursor: pointer;
}

.constellation-ghost .constellation-node-bg {
    stroke-dasharray: 4 2;
}

.constellation-ghost:hover {
    opacity: 0.8;
}

/* ===== Reduced Motion ===== */
@media (prefers-reduced-motion: reduce) {
    .constellation-node,
    .constellation-connection,
    .constellation-input,
    .constellation-quicklink,
    .constellation-home-btn {
        transition: none;
    }
}

/* ===== SVG Filters ===== */
/* Defined inline in the SVG, referenced here for documentation */
```

**Step 2: Link CSS in index.html**

Find the CSS links in `crates/adapteros-ui/index.html` and add:

```html
<link data-trunk rel="copy-file" href="dist/constellation.css"/>
```

And in the head section:
```html
<link rel="stylesheet" href="constellation.css"/>
```

**Step 3: Verify build**

Run: `trunk build --release 2>&1 | head -20`
Expected: Build succeeds

**Step 4: Commit**

```bash
git add crates/adapteros-ui/dist/constellation.css crates/adapteros-ui/index.html
git commit -m "style: add constellation CSS with calm glass aesthetic"
```

---

## Task 4: Create Constellation Canvas Component

**Files:**
- Create: `crates/adapteros-ui/src/components/constellation/mod.rs`
- Create: `crates/adapteros-ui/src/components/constellation/canvas.rs`
- Create: `crates/adapteros-ui/src/components/constellation/node.rs`
- Create: `crates/adapteros-ui/src/components/constellation/connection.rs`
- Modify: `crates/adapteros-ui/src/components/mod.rs`

**Step 1: Create mod.rs**

```rust
//! Constellation components
//!
//! Spatial visualization of user's work as a navigable constellation.

pub mod canvas;
pub mod connection;
pub mod node;

pub use canvas::ConstellationCanvas;
pub use connection::Connection;
pub use node::Node;
```

**Step 2: Create node.rs**

```rust
//! Constellation node component

use crate::signals::{ConstellationNode, InteractionLevel, NodeType};
use leptos::prelude::*;

/// A single node in the constellation
#[component]
pub fn Node(
    node: ConstellationNode,
    #[prop(into)] is_focused: Signal<bool>,
    #[prop(into)] interaction_level: Signal<InteractionLevel>,
    #[prop(optional)] on_click: Option<Callback<String>>,
) -> impl IntoView {
    let node_id = node.id.clone();
    let node_id_click = node.id.clone();
    let (x, y) = node.position;
    let size = node.size;
    let radius = size * 20.0; // Base radius scaled by size

    let handle_click = move |_| {
        if let Some(cb) = on_click {
            cb.call(node_id_click.clone());
        }
    };

    let class = move || {
        let mut c = "constellation-node".to_string();
        if is_focused.get() {
            c.push_str(" constellation-node--focused");
        }
        c
    };

    let is_interactive = move || {
        matches!(
            interaction_level.get(),
            InteractionLevel::SoftHints | InteractionLevel::FullSpatial
        )
    };

    view! {
        <g
            class=class
            transform=format!("translate({}, {})", x, y)
            on:click=handle_click
            tabindex=move || if is_interactive() { "0" } else { "-1" }
            role="button"
            aria-label=format!("{}: {}", node.node_type.icon_path(), node.title)
        >
            // Background circle
            <circle
                class="constellation-node-bg"
                r=radius
                cx="0"
                cy="0"
            />
            // Icon
            <svg
                class="constellation-node-icon"
                x=(-radius * 0.4)
                y=(-radius * 0.4)
                width=(radius * 0.8)
                height=(radius * 0.8)
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
            >
                <path d=node.node_type.icon_path()/>
            </svg>
            // Label (below node)
            <text
                class="constellation-node-label"
                y=(radius + 16.0)
                x="0"
            >
                {node.title.clone()}
            </text>
        </g>
    }
}

/// A ghost node (suggestion)
#[component]
pub fn GhostNode(
    label: &'static str,
    icon_path: &'static str,
    position: (f64, f64),
    #[prop(optional)] on_click: Option<Callback<()>>,
) -> impl IntoView {
    let (x, y) = position;
    let radius = 24.0;

    let handle_click = move |_| {
        if let Some(cb) = on_click {
            cb.call(());
        }
    };

    view! {
        <g
            class="constellation-node constellation-ghost"
            transform=format!("translate({}, {})", x, y)
            on:click=handle_click
            tabindex="0"
            role="button"
            aria-label=label
        >
            <circle
                class="constellation-node-bg"
                r=radius
                cx="0"
                cy="0"
            />
            <svg
                class="constellation-node-icon"
                x=(-radius * 0.4)
                y=(-radius * 0.4)
                width=(radius * 0.8)
                height=(radius * 0.8)
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
            >
                <path d=icon_path/>
            </svg>
            <text
                class="constellation-node-label"
                y=(radius + 16.0)
                x="0"
            >
                {label}
            </text>
        </g>
    }
}
```

**Step 3: Create connection.rs**

```rust
//! Constellation connection component

use crate::signals::ConstellationConnection;
use leptos::prelude::*;

/// A connection line between two nodes
#[component]
pub fn Connection(
    connection: ConstellationConnection,
    #[prop(into)] from_pos: Signal<(f64, f64)>,
    #[prop(into)] to_pos: Signal<(f64, f64)>,
) -> impl IntoView {
    let opacity = connection.strength.clamp(0.1, 0.6);

    view! {
        <line
            class="constellation-connection"
            x1=move || from_pos.get().0
            y1=move || from_pos.get().1
            x2=move || to_pos.get().0
            y2=move || to_pos.get().1
            style=format!("opacity: {}", opacity)
        />
    }
}
```

**Step 4: Create canvas.rs**

```rust
//! Constellation canvas - the SVG container for all nodes and connections

use super::{Connection, GhostNode, Node};
use crate::signals::{use_constellation, ConstellationNode, InteractionLevel};
use leptos::prelude::*;
use std::collections::HashMap;

/// The main constellation SVG canvas
#[component]
pub fn ConstellationCanvas() -> impl IntoView {
    let (state, action) = use_constellation();

    let interaction_level = Signal::derive(move || state.get().interaction_level);
    let is_interactive = move || {
        matches!(
            interaction_level.get(),
            InteractionLevel::SoftHints | InteractionLevel::FullSpatial
        )
    };

    let canvas_class = move || {
        let mut c = "constellation-canvas".to_string();
        if is_interactive() {
            c.push_str(" constellation--interactive");
        }
        c
    };

    // Build position lookup for connections
    let node_positions = move || {
        state
            .get()
            .nodes
            .iter()
            .map(|n| (n.id.clone(), n.position))
            .collect::<HashMap<_, _>>()
    };

    let camera = move || state.get().camera;
    let focused_node = move || state.get().focused_node.clone();

    // Transform for camera position
    let view_transform = move || {
        let cam = camera();
        format!(
            "translate({}, {}) scale({})",
            -cam.x + 50.0, // Center offset
            -cam.y + 50.0,
            cam.zoom
        )
    };

    view! {
        <svg
            class=canvas_class
            viewBox="0 0 100 100"
            preserveAspectRatio="xMidYMid meet"
        >
            // SVG filters for blur and glow effects
            <defs>
                <filter id="constellation-blur" x="-50%" y="-50%" width="200%" height="200%">
                    <feGaussianBlur in="SourceGraphic" stdDeviation="0.5"/>
                </filter>
                <filter id="constellation-glow" x="-50%" y="-50%" width="200%" height="200%">
                    <feGaussianBlur in="SourceGraphic" stdDeviation="1" result="blur"/>
                    <feComposite in="SourceGraphic" in2="blur" operator="over"/>
                </filter>
            </defs>

            // Main content group with camera transform
            <g transform=view_transform>
                // Connections first (below nodes)
                <For
                    each=move || state.get().connections.clone()
                    key=|c| format!("{}-{}", c.from_id, c.to_id)
                    children=move |conn| {
                        let positions = node_positions();
                        let from_id = conn.from_id.clone();
                        let to_id = conn.to_id.clone();
                        let from_pos = Signal::derive(move || {
                            positions.get(&from_id).copied().unwrap_or((0.0, 0.0))
                        });
                        let to_pos = Signal::derive(move || {
                            positions.get(&to_id).copied().unwrap_or((0.0, 0.0))
                        });
                        view! {
                            <Connection
                                connection=conn
                                from_pos=from_pos
                                to_pos=to_pos
                            />
                        }
                    }
                />

                // Nodes
                <For
                    each=move || state.get().nodes.clone()
                    key=|n| n.id.clone()
                    children=move |node| {
                        let node_id = node.id.clone();
                        let is_focused = Signal::derive(move || {
                            focused_node() == Some(node_id.clone())
                        });
                        let action_clone = action.clone();
                        let node_id_for_click = node.id.clone();
                        view! {
                            <Node
                                node=node
                                is_focused=is_focused
                                interaction_level=interaction_level
                                on_click=Callback::new(move |id: String| {
                                    action_clone.navigate_to(&id);
                                })
                            />
                        }
                    }
                />

                // Ghost nodes when empty
                <Show when=move || state.get().nodes.is_empty()>
                    <GhostNode
                        label="Upload a document"
                        icon_path="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12"
                        position=(-30.0, -15.0)
                    />
                    <GhostNode
                        label="Start a conversation"
                        icon_path="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z"
                        position=(30.0, -15.0)
                    />
                    <GhostNode
                        label="Explore training"
                        icon_path="M13 10V3L4 14h7v7l9-11h-7z"
                        position=(0.0, 25.0)
                    />
                </Show>
            </g>
        </svg>
    }
}
```

**Step 5: Export from components/mod.rs**

Add to `crates/adapteros-ui/src/components/mod.rs`:

```rust
pub mod constellation;

pub use constellation::{ConstellationCanvas, Connection as ConstellationConnection, Node as ConstellationNode};
```

**Step 6: Verify compilation**

Run: `cargo check -p adapteros-ui --target wasm32-unknown-unknown`
Expected: Compiles without errors

**Step 7: Commit**

```bash
git add crates/adapteros-ui/src/components/constellation/
git add crates/adapteros-ui/src/components/mod.rs
git commit -m "feat(ui): add constellation canvas, node, and connection components"
```

---

## Task 5: Create Center Input Component

**Files:**
- Create: `crates/adapteros-ui/src/components/constellation/center_input.rs`
- Modify: `crates/adapteros-ui/src/components/constellation/mod.rs`

**Step 1: Create center_input.rs**

```rust
//! Center input component - the conversational interface at the constellation center

use crate::signals::{use_chat, use_constellation, ChatState, ConstellationState};
use leptos::prelude::*;

/// Adaptive center input for the constellation
#[component]
pub fn CenterInput() -> impl IntoView {
    let (constellation_state, constellation_action) = use_constellation();
    let (chat_state, chat_action) = use_chat();

    let input_value = RwSignal::new(String::new());
    let input_ref = NodeRef::<leptos::html::Input>::new();

    // Adaptive placeholder based on state
    let placeholder = move || {
        let state = constellation_state.get();
        if state.nodes.is_empty() {
            "What would you like to work on?"
        } else if let Some(focused) = &state.focused_node {
            // Find the focused node's title
            state
                .nodes
                .iter()
                .find(|n| &n.id == focused)
                .map(|n| n.title.as_str())
                .unwrap_or("Continue working...")
        } else {
            "Ask anything or navigate to your work..."
        }
    };

    // Handle submit
    let on_submit = move |ev: web_sys::SubmitEvent| {
        ev.prevent_default();
        let value = input_value.get();
        if value.trim().is_empty() {
            return;
        }

        // Check for navigation commands
        let lower = value.to_lowercase();
        if lower.starts_with("go to ")
            || lower.starts_with("open ")
            || lower.starts_with("show ")
            || lower.starts_with("navigate to ")
        {
            // Extract target and try to navigate
            let target = value
                .trim_start_matches(|c: char| !c.is_whitespace())
                .trim();
            // TODO: Implement navigation parsing
            web_sys::console::log_1(&format!("Navigate to: {}", target).into());
        } else if lower == "home" || lower == "center" || lower == "return" {
            constellation_action.return_to_center();
        } else {
            // Regular chat message
            let chat_action = chat_action.clone();
            let value = value.clone();
            wasm_bindgen_futures::spawn_local(async move {
                // Use existing chat action to send message
                // This integrates with the existing chat system
                chat_action.send_message(&value).await;
            });
        }

        input_value.set(String::new());
    };

    // Focus input on mount
    Effect::new(move || {
        if let Some(input) = input_ref.get() {
            let _ = input.focus();
        }
    });

    view! {
        <div class="constellation-center">
            // Welcome message (only when at center with no activity)
            <Show when=move || constellation_state.get().is_at_center && constellation_state.get().nodes.is_empty()>
                <p class="constellation-welcome">
                    "Start a conversation, upload a document, or explore your work."
                </p>
            </Show>

            // Main input
            <form class="constellation-input-wrapper" on:submit=on_submit>
                <input
                    node_ref=input_ref
                    type="text"
                    class="constellation-input"
                    placeholder=placeholder
                    prop:value=move || input_value.get()
                    on:input=move |ev| {
                        input_value.set(event_target_value(&ev));
                    }
                    autocomplete="off"
                    spellcheck="false"
                />
            </form>

            // Quicklinks
            <Quicklinks/>
        </div>
    }
}

/// Quicklinks component
#[component]
fn Quicklinks() -> impl IntoView {
    let (state, _action) = use_constellation();

    view! {
        <div class="constellation-quicklinks">
            <For
                each=move || state.get().quicklinks.clone()
                key=|q| q.id.clone()
                children=move |quicklink| {
                    view! {
                        <button
                            class="constellation-quicklink"
                            data-tooltip=quicklink.label.clone()
                            aria-label=quicklink.label.clone()
                        >
                            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                <path d=quicklink.icon_path.clone()/>
                            </svg>
                        </button>
                    }
                }
            />
        </div>
    }
}
```

**Step 2: Export from mod.rs**

Update `crates/adapteros-ui/src/components/constellation/mod.rs`:

```rust
//! Constellation components
//!
//! Spatial visualization of user's work as a navigable constellation.

pub mod canvas;
pub mod center_input;
pub mod connection;
pub mod node;

pub use canvas::ConstellationCanvas;
pub use center_input::CenterInput;
pub use connection::Connection;
pub use node::{GhostNode, Node};
```

**Step 3: Update components/mod.rs exports**

```rust
pub use constellation::{CenterInput, ConstellationCanvas, ConstellationConnection, ConstellationNode, GhostNode};
```

**Step 4: Verify compilation**

Run: `cargo check -p adapteros-ui --target wasm32-unknown-unknown`
Expected: Compiles without errors

**Step 5: Commit**

```bash
git add crates/adapteros-ui/src/components/constellation/center_input.rs
git add crates/adapteros-ui/src/components/constellation/mod.rs
git add crates/adapteros-ui/src/components/mod.rs
git commit -m "feat(ui): add center input component with quicklinks"
```

---

## Task 6: Create Home Page

**Files:**
- Create: `crates/adapteros-ui/src/pages/home.rs`
- Modify: `crates/adapteros-ui/src/pages/mod.rs`

**Step 1: Create home.rs**

```rust
//! Home page - Constellation landing view
//!
//! The spatial, chat-first landing experience.

use crate::components::{CenterInput, ConstellationCanvas};
use crate::signals::{
    provide_constellation_context, use_constellation, InteractionLevel,
};
use leptos::prelude::*;

/// Home page - the constellation landing
#[component]
pub fn Home() -> impl IntoView {
    // Provide constellation context
    provide_constellation_context();

    let (state, action) = use_constellation();

    // Increment session count on mount
    Effect::new(move || {
        action.increment_session();
    });

    let is_at_center = move || state.get().is_at_center;
    let interaction_level = move || state.get().interaction_level;

    // Show return-to-center button when not at center and level >= SoftHints
    let show_home_btn = move || {
        !is_at_center()
            && matches!(
                interaction_level(),
                InteractionLevel::SoftHints | InteractionLevel::FullSpatial
            )
    };

    view! {
        <div class="constellation">
            // SVG canvas with nodes and connections
            <ConstellationCanvas/>

            // Center area with input
            <CenterInput/>

            // Return to center button (progressive disclosure)
            <button
                class=move || {
                    let mut c = "constellation-home-btn".to_string();
                    if show_home_btn() {
                        c.push_str(" constellation-home-btn--visible");
                    }
                    c
                }
                on:click=move |_| action.return_to_center()
                aria-label="Return to center"
            >
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    <path d="M3 9l9-7 9 7v11a2 2 0 01-2 2H5a2 2 0 01-2-2z"/>
                    <polyline points="9 22 9 12 15 12 15 22"/>
                </svg>
                <span>"Home"</span>
            </button>
        </div>
    }
}
```

**Step 2: Export from pages/mod.rs**

Add to `crates/adapteros-ui/src/pages/mod.rs`:

```rust
pub mod home;

pub use home::Home;
```

**Step 3: Verify compilation**

Run: `cargo check -p adapteros-ui --target wasm32-unknown-unknown`
Expected: Compiles without errors

**Step 4: Commit**

```bash
git add crates/adapteros-ui/src/pages/home.rs crates/adapteros-ui/src/pages/mod.rs
git commit -m "feat(ui): add home page with constellation view"
```

---

## Task 7: Update Routing

**Files:**
- Modify: `crates/adapteros-ui/src/lib.rs`

**Step 1: Add /home route and update / redirect**

Find the routes section (around line 183) and update:

```rust
// Change this line:
<Route path=path!("/") view=|| view! { <ProtectedRoute><Shell><pages::Dashboard/></Shell></ProtectedRoute> }/>

// To:
<Route path=path!("/") view=|| view! { <Redirect path="/home"/> }/>
<Route path=path!("/home") view=|| view! { <ProtectedRoute><HomeShell><pages::Home/></HomeShell></ProtectedRoute> }/>
```

**Step 2: Create HomeShell component**

Add above the `App` component (around line 155):

```rust
/// Shell variant for the Home page - no taskbar, constellation is the navigation
#[component]
fn HomeShell(children: Children) -> impl IntoView {
    use crate::components::{CommandPalette, ToastContainer};
    use crate::components::status_center::StatusCenterProvider;
    use crate::components::layout::topbar::TopBar;
    use crate::signals::{
        provide_route_context, provide_ui_profile_context,
    };

    provide_ui_profile_context();
    provide_route_context();

    view! {
        <StatusCenterProvider>
            <div class="shell shell--home">
                // Minimal top bar (just branding and user menu)
                <TopBar/>

                // Main content - full height, no taskbar
                <main class="shell-main shell-main--full">
                    {children()}
                </main>
            </div>
        </StatusCenterProvider>
    }
}
```

**Step 3: Add shell--home CSS**

Add to `crates/adapteros-ui/dist/components.css` (in the shell section):

```css
/* Home shell variant - full height, no taskbar */
.shell--home {
    display: flex;
    flex-direction: column;
    height: 100vh;
}

.shell-main--full {
    flex: 1;
    overflow: hidden;
}
```

**Step 4: Verify compilation**

Run: `cargo check -p adapteros-ui --target wasm32-unknown-unknown`
Expected: Compiles without errors

**Step 5: Commit**

```bash
git add crates/adapteros-ui/src/lib.rs crates/adapteros-ui/dist/components.css
git commit -m "feat(ui): route / to /home, add HomeShell without taskbar"
```

---

## Task 8: Add Keyboard Navigation

**Files:**
- Modify: `crates/adapteros-ui/src/pages/home.rs`

**Step 1: Add keyboard handlers**

Update the `Home` component to include keyboard navigation:

```rust
//! Home page - Constellation landing view
//!
//! The spatial, chat-first landing experience.

use crate::components::{CenterInput, ConstellationCanvas};
use crate::signals::{
    provide_constellation_context, use_constellation, InteractionLevel,
};
use leptos::prelude::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

/// Home page - the constellation landing
#[component]
pub fn Home() -> impl IntoView {
    // Provide constellation context
    provide_constellation_context();

    let (state, action) = use_constellation();

    // Increment session count on mount
    Effect::new(move || {
        action.increment_session();
    });

    // Keyboard navigation (only for FullSpatial level)
    let keyboard_handler_set = StoredValue::new(false);
    Effect::new(move || {
        if keyboard_handler_set.get_value() {
            return;
        }
        if !matches!(state.get().interaction_level, InteractionLevel::FullSpatial) {
            return;
        }
        keyboard_handler_set.set_value(true);

        let action = action.clone();
        let closure = Closure::wrap(Box::new(move |event: web_sys::KeyboardEvent| {
            let key = event.key();

            // Don't intercept when in input
            if let Some(target) = event.target() {
                if let Some(element) = target.dyn_ref::<web_sys::HtmlElement>() {
                    let tag = element.tag_name().to_lowercase();
                    if tag == "input" || tag == "textarea" {
                        return;
                    }
                }
            }

            match key.as_str() {
                "h" | "H" => {
                    action.return_to_center();
                    event.prevent_default();
                }
                "/" => {
                    // Focus the input
                    if let Some(input) = web_sys::window()
                        .and_then(|w| w.document())
                        .and_then(|d| d.query_selector(".constellation-input").ok().flatten())
                    {
                        if let Some(el) = input.dyn_ref::<web_sys::HtmlElement>() {
                            let _ = el.focus();
                        }
                    }
                    event.prevent_default();
                }
                "Escape" => {
                    action.return_to_center();
                    event.prevent_default();
                }
                _ => {}
            }
        }) as Box<dyn FnMut(_)>);

        if let Some(window) = web_sys::window() {
            let _ = window
                .add_event_listener_with_callback("keydown", closure.as_ref().unchecked_ref());
        }
        closure.forget();
    });

    let is_at_center = move || state.get().is_at_center;
    let interaction_level = move || state.get().interaction_level;

    // Show return-to-center button when not at center and level >= SoftHints
    let show_home_btn = move || {
        !is_at_center()
            && matches!(
                interaction_level(),
                InteractionLevel::SoftHints | InteractionLevel::FullSpatial
            )
    };

    view! {
        <div class="constellation">
            // SVG canvas with nodes and connections
            <ConstellationCanvas/>

            // Center area with input
            <CenterInput/>

            // Return to center button (progressive disclosure)
            <button
                class=move || {
                    let mut c = "constellation-home-btn".to_string();
                    if show_home_btn() {
                        c.push_str(" constellation-home-btn--visible");
                    }
                    c
                }
                on:click=move |_| action.return_to_center()
                aria-label="Return to center"
                title="Return to center (H)"
            >
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    <path d="M3 9l9-7 9 7v11a2 2 0 01-2 2H5a2 2 0 01-2-2z"/>
                    <polyline points="9 22 9 12 15 12 15 22"/>
                </svg>
                <span>"Home"</span>
            </button>
        </div>
    }
}
```

**Step 2: Verify compilation**

Run: `cargo check -p adapteros-ui --target wasm32-unknown-unknown`
Expected: Compiles without errors

**Step 3: Commit**

```bash
git add crates/adapteros-ui/src/pages/home.rs
git commit -m "feat(ui): add keyboard navigation for constellation (H=home, /=focus, Esc=center)"
```

---

## Task 9: Add Screen Reader Support

**Files:**
- Modify: `crates/adapteros-ui/src/pages/home.rs`

**Step 1: Add ARIA live region for navigation announcements**

Add after the constellation div opens:

```rust
view! {
    <div class="constellation" role="application" aria-label="Constellation workspace">
        // Screen reader announcements
        <div
            class="sr-only"
            role="status"
            aria-live="polite"
            aria-atomic="true"
        >
            {move || {
                let s = state.get();
                if s.is_at_center {
                    if s.nodes.is_empty() {
                        "You are at center. No recent work. Say or type to begin."
                    } else {
                        "You are at center. Nearby items available."
                    }
                } else if let Some(node_id) = &s.focused_node {
                    // Find node title
                    s.nodes
                        .iter()
                        .find(|n| &n.id == node_id)
                        .map(|n| format!("Focused on: {}", n.title))
                        .unwrap_or_else(|| "Navigating...".to_string())
                        .as_str()
                } else {
                    "Navigating..."
                }
            }}
        </div>

        // ... rest of view
    }
}
```

**Step 2: Add sr-only CSS class**

Add to `crates/adapteros-ui/dist/base.css`:

```css
/* Screen reader only - visually hidden but accessible */
.sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
}
```

**Step 3: Verify compilation**

Run: `cargo check -p adapteros-ui --target wasm32-unknown-unknown`
Expected: Compiles without errors

**Step 4: Commit**

```bash
git add crates/adapteros-ui/src/pages/home.rs crates/adapteros-ui/dist/base.css
git commit -m "feat(ui): add screen reader support for constellation navigation"
```

---

## Task 10: Integration Test

**Files:**
- (No new files - manual verification)

**Step 1: Build the UI**

Run: `trunk build --release`
Expected: Build succeeds

**Step 2: Start the backend**

Run: `./start backend`
Expected: Backend starts on port 8080

**Step 3: Open in browser**

Navigate to: `http://localhost:8080/`
Expected: Redirects to `/home`, shows constellation view with:
- Centered input with placeholder "What would you like to work on?"
- Ghost nodes if no work exists
- Quicklinks below input

**Step 4: Verify progressive disclosure**

Clear localStorage: `localStorage.removeItem('adapteros_constellation_sessions')`
Refresh page.
Expected: Level 1 (ConversationOnly) - nodes not clickable

After 3 refreshes:
Expected: Level 2 (SoftHints) - nodes have hover states, clickable

**Step 5: Verify keyboard navigation**

Press `/`: Expected: Input focuses
Press `H`: Expected: Returns to center (if navigated away)
Press `Escape`: Expected: Returns to center

**Step 6: Commit final state**

```bash
git add -A
git commit -m "feat(ui): complete constellation landing page implementation"
```

---

## Summary

**Files created:**
- `crates/adapteros-ui/src/signals/constellation.rs`
- `crates/adapteros-ui/dist/constellation.css`
- `crates/adapteros-ui/src/components/constellation/mod.rs`
- `crates/adapteros-ui/src/components/constellation/canvas.rs`
- `crates/adapteros-ui/src/components/constellation/node.rs`
- `crates/adapteros-ui/src/components/constellation/connection.rs`
- `crates/adapteros-ui/src/components/constellation/center_input.rs`
- `crates/adapteros-ui/src/pages/home.rs`

**Files modified:**
- `crates/adapteros-ui/dist/glass.css`
- `crates/adapteros-ui/dist/base.css`
- `crates/adapteros-ui/dist/components.css`
- `crates/adapteros-ui/index.html`
- `crates/adapteros-ui/src/signals/mod.rs`
- `crates/adapteros-ui/src/components/mod.rs`
- `crates/adapteros-ui/src/pages/mod.rs`
- `crates/adapteros-ui/src/lib.rs`

**Key patterns:**
- Progressive disclosure via session count in localStorage
- SVG-based constellation rendering
- Integration with existing chat signals
- Calm Glass CSS variant (no noise, higher blur, slower transitions)
- Screen reader support via ARIA live regions
- Keyboard navigation for power users
