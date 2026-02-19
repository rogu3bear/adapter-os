//! Shared navigation registry for taskbar, start menu, command palette, and mobile nav.
//!
//! Workflow-first navigation groups:
//! Infer → Data → Train → Deploy → Route → Observe → Govern → Org
//!
//! IA route classes used in docs: Primary, Tools, Hidden, Experimental.

use adapteros_api_types::UiProfile;

// ═══════════════════════════════════════════════════════════════════════════
// WORKFLOW NAV GROUPS - Canonical source of truth for all navigation
// ═══════════════════════════════════════════════════════════════════════════

/// A navigation group representing a workflow phase
#[derive(Debug, Clone)]
pub struct NavGroup {
    /// Unique group identifier
    pub id: &'static str,
    /// Display label (e.g., "Data", "Train")
    pub label: &'static str,
    /// SVG icon path for the group
    pub icon: &'static str,
    /// Alt+N keyboard shortcut (1-8)
    pub alt_shortcut: Option<u8>,
    /// Navigation items in this group
    pub items: &'static [NavItem],
    /// Whether this group is collapsed by default in start menu
    pub collapsed_by_default: bool,
    /// Include in taskbar module buttons
    pub show_in_taskbar: bool,
    /// Show in mobile nav
    pub show_in_mobile: bool,
}

/// A single navigation item within a group
#[derive(Debug, Clone, Copy)]
pub struct NavItem {
    /// Unique item identifier
    pub id: &'static str,
    /// Display label
    pub label: &'static str,
    /// Navigation route path
    pub route: &'static str,
    /// Optional SVG icon path (inherits group icon if None)
    pub icon: Option<&'static str>,
    /// Search keywords for command palette
    pub keywords: &'static [&'static str],
    /// Hidden route class (kept addressable; omitted from taskbar/start menu/mobile nav)
    pub hidden: bool,
}

impl NavItem {
    const fn new(id: &'static str, label: &'static str, route: &'static str) -> Self {
        Self {
            id,
            label,
            route,
            icon: None,
            keywords: &[],
            hidden: false,
        }
    }

    const fn with_keywords(mut self, keywords: &'static [&'static str]) -> Self {
        self.keywords = keywords;
        self
    }

    #[allow(dead_code)]
    const fn hidden(mut self) -> Self {
        self.hidden = true;
        self
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// ICONS - SVG paths for navigation icons
// ═══════════════════════════════════════════════════════════════════════════

/// Dashboard / Home icon
const ICON_HOME: &str = "M3 12l2-2m0 0l7-7 7 7M5 10v10a1 1 0 001 1h3m10-11l2 2m-2-2v10a1 1 0 01-1 1h-3m-6 0a1 1 0 001-1v-4a1 1 0 011-1h2a1 1 0 011 1v4a1 1 0 001 1m-6 0h6";

/// Data / Database icon
const ICON_DATABASE: &str = "M4 7v10c0 2.21 3.582 4 8 4s8-1.79 8-4V7M4 7c0 2.21 3.582 4 8 4s8-1.79 8-4M4 7c0-2.21 3.582-4 8-4s8 1.79 8 4m0 5c0 2.21-3.582 4-8 4s-8-1.79-8-4";

/// Train / Flame icon
const ICON_FLAME: &str = "M17.657 18.657A8 8 0 016.343 7.343S7 9 9 10c0-2 .5-5 2.986-7C14 5 16.09 5.777 17.656 7.343A7.975 7.975 0 0120 13a7.975 7.975 0 01-2.343 5.657z M9.879 16.121A3 3 0 1012.015 11L11 14H9c0 .768.293 1.536.879 2.121z";

/// Deploy / Rocket icon
const ICON_ROCKET: &str = "M15.59 14.37a6 6 0 01-5.84 7.38v-4.8m5.84-2.58a14.98 14.98 0 006.16-12.12A14.98 14.98 0 009.631 8.41m5.96 5.96a14.926 14.926 0 01-5.841 2.58m-.119-8.54a6 6 0 00-7.381 5.84h4.8m2.581-5.84a14.927 14.927 0 00-2.58 5.84m2.699 2.7c-.103.021-.207.041-.311.06a15.09 15.09 0 01-2.448-2.448 14.9 14.9 0 01.06-.312m-2.24 2.39a4.493 4.493 0 00-1.757 4.306 4.493 4.493 0 004.306-1.758M16.5 9a1.5 1.5 0 11-3 0 1.5 1.5 0 013 0z";

/// Route / Branch icon
const ICON_BRANCH: &str =
    "M7.5 21L3 16.5m0 0L7.5 12M3 16.5h13.5m0-13.5L21 7.5m0 0L16.5 12M21 7.5H7.5";

/// Infer / Zap icon (chat/lightning)
const ICON_ZAP: &str = "M13 10V3L4 14h7v7l9-11h-7z";

/// Observe / Eye icon
const ICON_EYE: &str = "M15 12a3 3 0 11-6 0 3 3 0 016 0z M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z";

/// Govern / Shield icon
const ICON_SHIELD: &str = "M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z";

/// Org / Building icon
const ICON_BUILDING: &str = "M19 21V5a2 2 0 00-2-2H7a2 2 0 00-2 2v16m14 0h2m-2 0h-5m-9 0H3m2 0h5M9 7h1m-1 4h1m4-4h1m-1 4h1m-5 10v-5a1 1 0 011-1h2a1 1 0 011 1v5m-4 0h4";

/// Models / Cube icon (foundation model)
const ICON_CUBE: &str = "M21 16V8a2 2 0 00-1-1.73l-7-4a2 2 0 00-2 0l-7 4A2 2 0 003 8v8a2 2 0 001 1.73l7 4a2 2 0 002 0l7-4A2 2 0 0021 16z M3.27 6.96L12 12.01l8.73-5.05M12 22.08V12";

/// Chat icon (for separate chat button in taskbar)
pub const ICON_CHAT: &str = "M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z";

// ═══════════════════════════════════════════════════════════════════════════
// NAVIGATION GROUPS DEFINITION
// ═══════════════════════════════════════════════════════════════════════════

/// Dashboard - pinned item (not a group, special handling)
pub const DASHBOARD_ITEM: NavItem = NavItem {
    id: "dashboard",
    label: "Dashboard",
    route: "/",
    icon: Some(ICON_HOME),
    keywords: &["home", "overview", "main", "index", "metrics"],
    hidden: false,
};

/// Full profile navigation groups in workflow order
static NAV_GROUPS_FULL: &[NavGroup] = &[
    // 1. Infer (Alt+1) — Prompt Studio is the primary entry point
    NavGroup {
        id: "infer",
        label: "Infer",
        icon: ICON_ZAP,
        alt_shortcut: Some(1),
        items: &[
            NavItem::new("chat", "Prompt Studio", "/chat").with_keywords(&[
                "inference",
                "generate",
                "prompt",
                "conversation",
                "stream",
            ]),
        ],
        collapsed_by_default: false,
        show_in_taskbar: true,
        show_in_mobile: true,
    },
    // 2. Data (Alt+2)
    NavGroup {
        id: "data",
        label: "Data",
        icon: ICON_DATABASE,
        alt_shortcut: Some(2),
        items: &[
            NavItem::new("documents", "Documents", "/documents")
                .with_keywords(&["files", "upload", "corpus", "ingest"]),
            NavItem::new("collections", "Collections", "/collections")
                .with_keywords(&["groups", "corpus", "organize"]),
            NavItem::new("datasets", "Datasets", "/datasets")
                .with_keywords(&["training", "data", "upload", "versions", "jsonl"]),
            NavItem::new("repositories", "Repositories", "/repositories")
                .with_keywords(&["git", "code", "codebase", "repo"]),
        ],
        collapsed_by_default: false,
        show_in_taskbar: true,
        show_in_mobile: true,
    },
    // 3. Train (Alt+3)
    NavGroup {
        id: "train",
        label: "Train",
        icon: ICON_FLAME,
        alt_shortcut: Some(3),
        items: &[NavItem::new("training", "Adapter Training", "/training")
            .with_keywords(&["train", "finetune", "jobs", "pipeline", "lora"])],
        collapsed_by_default: false,
        show_in_taskbar: true,
        show_in_mobile: true,
    },
    // 4. Deploy (Alt+4)
    NavGroup {
        id: "deploy",
        label: "Deploy",
        icon: ICON_ROCKET,
        alt_shortcut: Some(4),
        items: &[
            NavItem::new("adapters", "Adapters", "/adapters").with_keywords(&[
                "lora",
                "finetune",
                "weights",
                "models",
                "lifecycle",
            ]),
            NavItem::new("update_center", "Update Center", "/update-center").with_keywords(&[
                "promote",
                "production",
                "draft",
                "reviewed",
                "rollback",
                "restore",
            ]),
            NavItem::new("stacks", "Adapter Stack", "/stacks").with_keywords(&[
                "combination",
                "ensemble",
                "runtime",
                "active",
            ]),
            NavItem::new("models", "Base Model Registry", "/models").with_keywords(&[
                "llm",
                "foundation",
                "base",
                "weights",
                "load",
            ]),
        ],
        collapsed_by_default: false,
        show_in_taskbar: true,
        show_in_mobile: true,
    },
    // 5. Route (Alt+5)
    NavGroup {
        id: "route",
        label: "Route",
        icon: ICON_BRANCH,
        alt_shortcut: Some(5),
        items: &[
            NavItem::new("routing", "Routing", "/routing").with_keywords(&[
                "rules",
                "decisions",
                "k-sparse",
                "gates",
                "debug",
            ]),
        ],
        collapsed_by_default: false,
        show_in_taskbar: true,
        show_in_mobile: true,
    },
    // 6. Observe (Alt+6)
    NavGroup {
        id: "observe",
        label: "Observe",
        icon: ICON_EYE,
        alt_shortcut: Some(6),
        items: &[
            NavItem::new("runs", "Restore Points", "/runs").with_keywords(&[
                "flight",
                "recorder",
                "traces",
                "provenance",
                "receipts",
                "runs",
            ]),
            NavItem::new("monitoring", "Activity Monitor", "/monitoring").with_keywords(&[
                "alerts",
                "anomalies",
                "health",
                "metrics",
                "observability",
            ]),
            NavItem::new("errors", "Recovery Console", "/errors").with_keywords(&[
                "incidents",
                "crashes",
                "live",
                "analysis",
                "client",
            ]),
            NavItem::new("diff", "Diff", "/diff").with_keywords(&[
                "compare",
                "divergence",
                "determinism",
                "anchor",
            ]),
            NavItem::new("workers", "Inference Engines", "/workers").with_keywords(&[
                "runtime",
                "instances",
                "compute",
                "nodes",
                "process",
            ]),
        ],
        collapsed_by_default: false,
        show_in_taskbar: true,
        show_in_mobile: true,
    },
    // 7. Govern (Alt+7)
    NavGroup {
        id: "govern",
        label: "Govern",
        icon: ICON_SHIELD,
        alt_shortcut: Some(7),
        items: &[
            NavItem::new("policies", "Safety Shield", "/policies").with_keywords(&[
                "rules",
                "constraints",
                "enforcement",
                "determinism",
            ]),
            NavItem::new("audit", "Event Viewer", "/audit").with_keywords(&[
                "logs",
                "history",
                "events",
                "compliance",
                "trail",
            ]),
            NavItem::new("reviews", "Safety Queue", "/reviews").with_keywords(&[
                "hitl",
                "approval",
                "pause",
                "queue",
                "moderation",
            ]),
        ],
        collapsed_by_default: true,
        show_in_taskbar: true,
        show_in_mobile: true,
    },
    // 8. Org (Alt+8)
    NavGroup {
        id: "org",
        label: "Org",
        icon: ICON_BUILDING,
        alt_shortcut: Some(8),
        items: &[
            NavItem::new("agents", "Automation Agents (Beta)", "/agents").with_keywords(&[
                "orchestration",
                "multi-agent",
                "executor",
                "sessions",
            ]),
            NavItem::new("files", "Files", "/files").with_keywords(&[
                "filesystem",
                "browse",
                "directories",
                "server",
            ]),
            NavItem::new("admin", "Admin", "/admin").with_keywords(&[
                "users",
                "roles",
                "api-keys",
                "organization",
                "tenants",
            ]),
            NavItem::new("settings", "Settings", "/settings").with_keywords(&[
                "config",
                "preferences",
                "options",
                "profile",
            ]),
            NavItem::new("system", "Kernel", "/system").with_keywords(&[
                "health",
                "status",
                "diagnostics",
                "infrastructure",
            ]),
        ],
        collapsed_by_default: true,
        show_in_taskbar: true,
        show_in_mobile: true,
    },
];

/// Primary profile navigation groups (focused runtime projection)
static NAV_GROUPS_PRIMARY: &[NavGroup] = &[
    // 1. Train (Alt+1) — Adapter Training
    NavGroup {
        id: "train",
        label: "Train",
        icon: ICON_FLAME,
        alt_shortcut: Some(1),
        items: &[NavItem::new("training", "Adapter Training", "/training")
            .with_keywords(&["train", "finetune", "jobs", "pipeline", "lora"])],
        collapsed_by_default: false,
        show_in_taskbar: true,
        show_in_mobile: true,
    },
    // 2. Infer (Alt+2) — Prompt Studio
    NavGroup {
        id: "infer",
        label: "Infer",
        icon: ICON_ZAP,
        alt_shortcut: Some(2),
        items: &[
            NavItem::new("chat", "Prompt Studio", "/chat").with_keywords(&[
                "inference",
                "generate",
                "prompt",
                "conversation",
            ]),
        ],
        collapsed_by_default: false,
        show_in_taskbar: true,
        show_in_mobile: true,
    },
    // 3. Verify (Alt+3) — Restore Points
    NavGroup {
        id: "verify",
        label: "Verify",
        icon: ICON_EYE,
        alt_shortcut: Some(3),
        items: &[
            NavItem::new("runs", "Restore Points", "/runs").with_keywords(&[
                "flight",
                "recorder",
                "traces",
                "provenance",
                "receipts",
                "runs",
            ]),
        ],
        collapsed_by_default: false,
        show_in_taskbar: true,
        show_in_mobile: true,
    },
    // 4. Promote (Alt+4) — Update Center
    NavGroup {
        id: "promote",
        label: "Promote",
        icon: ICON_ROCKET,
        alt_shortcut: Some(4),
        items: &[
            NavItem::new("update_center", "Update Center", "/update-center").with_keywords(&[
                "promote",
                "production",
                "draft",
                "reviewed",
                "rollback",
                "restore",
            ]),
        ],
        collapsed_by_default: false,
        show_in_taskbar: true,
        show_in_mobile: true,
    },
    // 5. Deploy (Alt+5) — Models + Adapters
    NavGroup {
        id: "deploy",
        label: "Deploy",
        icon: ICON_CUBE,
        alt_shortcut: Some(5),
        items: &[
            NavItem::new("models", "Base Model Registry", "/models").with_keywords(&[
                "llm",
                "foundation",
                "base",
                "weights",
            ]),
            NavItem::new("adapters", "Adapters", "/adapters").with_keywords(&[
                "lora",
                "finetune",
                "weights",
                "models",
                "lifecycle",
            ]),
        ],
        collapsed_by_default: false,
        show_in_taskbar: true,
        show_in_mobile: true,
    },
    // 6. Observe (Alt+6) — Inference Engines + Kernel
    NavGroup {
        id: "observe",
        label: "Observe",
        icon: ICON_EYE,
        alt_shortcut: Some(6),
        items: &[
            NavItem::new("workers", "Inference Engines", "/workers").with_keywords(&[
                "runtime",
                "instances",
                "compute",
                "nodes",
                "process",
            ]),
            NavItem::new("system", "Kernel", "/system").with_keywords(&[
                "health",
                "status",
                "diagnostics",
                "infrastructure",
            ]),
        ],
        collapsed_by_default: false,
        show_in_taskbar: true,
        show_in_mobile: true,
    },
];

// ═══════════════════════════════════════════════════════════════════════════
// PUBLIC API
// ═══════════════════════════════════════════════════════════════════════════

/// Get all navigation groups for the given profile
pub fn nav_groups(profile: UiProfile) -> Vec<&'static NavGroup> {
    match profile {
        UiProfile::Primary => NAV_GROUPS_PRIMARY.iter().collect(),
        UiProfile::Full => NAV_GROUPS_FULL.iter().collect(),
    }
}

/// Get the first route for a given alt shortcut (1-8)
pub fn route_for_alt_shortcut(profile: UiProfile, shortcut: u8) -> Option<&'static str> {
    nav_groups(profile)
        .into_iter()
        .find(|g| g.alt_shortcut == Some(shortcut))
        .and_then(|g| g.items.first())
        .map(|item| item.route)
}

/// Get all nav items (flattened) for the given profile, including dashboard
pub fn all_nav_items(profile: UiProfile) -> Vec<&'static NavItem> {
    let mut items = vec![&DASHBOARD_ITEM];
    for group in nav_groups(profile) {
        for item in group.items {
            if !item.hidden {
                items.push(item);
            }
        }
    }
    items
}

// ═══════════════════════════════════════════════════════════════════════════
// LEGACY API - For backward compatibility with existing taskbar/start menu
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct TaskbarModuleItem {
    pub label: &'static str,
    pub href: &'static str,
    pub icon: &'static str,
    pub routes: Vec<&'static str>,
}

#[derive(Debug, Clone)]
pub struct StartMenuModule {
    pub name: &'static str,
    pub icon: &'static str,
    pub items: Vec<(&'static str, &'static str)>,
    pub collapsed: bool,
}

#[derive(Debug, Clone)]
pub struct MobileNavItem {
    pub label: &'static str,
    pub href: &'static str,
    pub icon: &'static str,
}

/// Build taskbar modules from nav groups
pub fn build_taskbar_modules(profile: UiProfile) -> Vec<TaskbarModuleItem> {
    nav_groups(profile)
        .into_iter()
        .filter(|g| g.show_in_taskbar)
        .map(|group| {
            let routes: Vec<&'static str> = group.items.iter().map(|i| i.route).collect();
            TaskbarModuleItem {
                label: group.label,
                href: group.items.first().map(|i| i.route).unwrap_or("/"),
                icon: group.icon,
                routes,
            }
        })
        .collect()
}

/// Build start menu modules from nav groups
pub fn build_start_menu_modules(profile: UiProfile) -> Vec<StartMenuModule> {
    nav_groups(profile)
        .into_iter()
        .map(|group| {
            let items: Vec<(&'static str, &'static str)> = group
                .items
                .iter()
                .filter(|i| !i.hidden)
                .map(|i| (i.label, i.route))
                .collect();
            StartMenuModule {
                name: group.label,
                icon: group.icon,
                items,
                collapsed: group.collapsed_by_default,
            }
        })
        .collect()
}

/// Build mobile nav items from nav groups
pub fn build_mobile_nav_items(profile: UiProfile) -> Vec<MobileNavItem> {
    nav_groups(profile)
        .into_iter()
        .filter(|g| g.show_in_mobile)
        .map(|group| MobileNavItem {
            label: group.label,
            href: group.items.first().map(|i| i.route).unwrap_or("/"),
            icon: group.icon,
        })
        .collect()
}
