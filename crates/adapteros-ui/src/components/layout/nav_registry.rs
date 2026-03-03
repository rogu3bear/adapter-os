//! Shared navigation registry for sidebar, command palette, and mobile nav.
//!
//! Canonical navigation groups:
//! Home -> Build -> Chat -> Evidence -> Versions -> System

use adapteros_api_types::UiProfile;

/// A navigation group representing a workflow phase
#[derive(Debug, Clone)]
pub struct NavGroup {
    pub id: &'static str,
    pub label: &'static str,
    /// Power-user alias shown as secondary text/tooltips in advanced profile.
    pub legacy_label: Option<&'static str>,
    pub icon: &'static str,
    pub alt_shortcut: Option<u8>,
    pub items: &'static [NavItem],
    pub collapsed_by_default: bool,
    pub show_in_taskbar: bool,
    pub show_in_mobile: bool,
}

/// A single navigation item within a group
#[derive(Debug, Clone, Copy)]
pub struct NavItem {
    pub id: &'static str,
    pub label: &'static str,
    pub route: &'static str,
    pub icon: Option<&'static str>,
    pub keywords: &'static [&'static str],
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
}

// ===========================================================================
// ICONS
// ===========================================================================

const ICON_HOME: &str = "M3 12l2-2m0 0l7-7 7 7M5 10v10a1 1 0 001 1h3m10-11l2 2m-2-2v10a1 1 0 01-1 1h-3m-6 0a1 1 0 001-1v-4a1 1 0 011-1h2a1 1 0 011 1v4a1 1 0 001 1m-6 0h6";
const ICON_FLAME: &str = "M17.657 18.657A8 8 0 016.343 7.343S7 9 9 10c0-2 .5-5 2.986-7C14 5 16.09 5.777 17.656 7.343A7.975 7.975 0 0120 13a7.975 7.975 0 01-2.343 5.657z M9.879 16.121A3 3 0 1012.015 11L11 14H9c0 .768.293 1.536.879 2.121z";
const ICON_ROCKET: &str = "M15.59 14.37a6 6 0 01-5.84 7.38v-4.8m5.84-2.58a14.98 14.98 0 006.16-12.12A14.98 14.98 0 009.631 8.41m5.96 5.96a14.926 14.926 0 01-5.841 2.58m-.119-8.54a6 6 0 00-7.381 5.84h4.8m2.581-5.84a14.927 14.927 0 00-2.58 5.84m2.699 2.7c-.103.021-.207.041-.311.06a15.09 15.09 0 01-2.448-2.448 14.9 14.9 0 01.06-.312m-2.24 2.39a4.493 4.493 0 00-1.757 4.306 4.493 4.493 0 004.306-1.758M16.5 9a1.5 1.5 0 11-3 0 1.5 1.5 0 013 0z";
const ICON_EYE: &str = "M15 12a3 3 0 11-6 0 3 3 0 016 0z M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z";
const ICON_SHIELD: &str = "M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z";
pub const ICON_CHAT: &str = "M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z";

// ===========================================================================
// NAVIGATION GROUPS
// ===========================================================================

pub const DASHBOARD_ITEM: NavItem = NavItem {
    id: "dashboard",
    label: "Home",
    route: "/",
    icon: Some(ICON_HOME),
    keywords: &["home", "overview", "main", "index", "metrics"],
    hidden: false,
};

/// Full profile navigation groups.
static NAV_GROUPS_FULL: &[NavGroup] = &[
    NavGroup {
        id: "chat",
        label: "Chat",
        legacy_label: Some("Infer"),
        icon: ICON_CHAT,
        alt_shortcut: Some(1),
        items: &[NavItem::new("chat", "Chat", "/chat").with_keywords(&[
            "inference",
            "generate",
            "conversation",
            "stream",
            "chat",
            "history",
        ])],
        collapsed_by_default: false,
        show_in_taskbar: true,
        show_in_mobile: true,
    },
    NavGroup {
        id: "build",
        label: "Build",
        legacy_label: None,
        icon: ICON_FLAME,
        alt_shortcut: Some(2),
        items: &[
            NavItem::new("training", "Build", "/training?open_wizard=1").with_keywords(&[
                "create",
                "adapter",
                "train",
                "files",
                "knowledge",
            ]),
            NavItem::new("adapters", "Adapters", "/adapters").with_keywords(&[
                "lora",
                "finetune",
                "weights",
                "models",
                "lifecycle",
            ]),
            NavItem::new("models", "Models", "/models").with_keywords(&[
                "llm",
                "foundation",
                "base",
                "weights",
                "load",
            ]),
            NavItem::new("documents", "Documents", "/documents").with_keywords(&[
                "files",
                "upload",
                "corpus",
                "ingest",
                "documents",
            ]),
            NavItem::new("datasets", "Datasets", "/datasets").with_keywords(&[
                "dataset",
                "datasets",
                "training data",
                "validation",
                "trust",
            ]),
        ],
        collapsed_by_default: false,
        show_in_taskbar: true,
        show_in_mobile: true,
    },
    NavGroup {
        id: "evidence",
        label: "Execution Records",
        legacy_label: Some("Verify"),
        icon: ICON_EYE,
        alt_shortcut: Some(3),
        items: &[
            NavItem::new("runs", "Execution Records", "/runs").with_keywords(&[
                "flight",
                "recorder",
                "traces",
                "provenance",
                "receipts",
                "runs",
                "evidence",
            ]),
        ],
        collapsed_by_default: false,
        show_in_taskbar: true,
        show_in_mobile: true,
    },
    NavGroup {
        id: "versions",
        label: "Versions",
        legacy_label: Some("Promote"),
        icon: ICON_ROCKET,
        alt_shortcut: Some(4),
        items: &[
            NavItem::new("update_center", "Versions", "/update-center").with_keywords(&[
                "promote",
                "run promote",
                "production",
                "draft",
                "reviewed",
                "checkout",
                "run checkout",
                "version history",
                "feed-dataset",
                "versions",
            ]),
        ],
        collapsed_by_default: false,
        show_in_taskbar: true,
        show_in_mobile: true,
    },
    NavGroup {
        id: "system",
        label: "System",
        legacy_label: Some("Deploy"),
        icon: ICON_SHIELD,
        alt_shortcut: Some(5),
        items: &[
            NavItem::new("system", "System", "/system").with_keywords(&[
                "health",
                "status",
                "diagnostics",
                "infrastructure",
                "system",
            ]),
            NavItem::new("workers", "Workers", "/workers").with_keywords(&[
                "runtime",
                "instances",
                "compute",
                "nodes",
                "process",
                "engines",
                "workers",
            ]),
            NavItem::new("settings", "Settings", "/settings").with_keywords(&[
                "config",
                "preferences",
                "options",
                "profile",
            ]),
            NavItem::new("policies", "Policies", "/policies").with_keywords(&[
                "rules",
                "constraints",
                "enforcement",
                "determinism",
                "safety",
            ]),
            NavItem::new("audit", "Audit Log", "/audit").with_keywords(&[
                "logs",
                "history",
                "events",
                "compliance",
                "trail",
            ]),
            NavItem::new("admin", "Admin", "/admin").with_keywords(&[
                "users",
                "roles",
                "api-keys",
                "organization",
                "tenants",
            ]),
        ],
        collapsed_by_default: true,
        show_in_taskbar: true,
        show_in_mobile: true,
    },
];

/// Primary profile navigation groups.
static NAV_GROUPS_PRIMARY: &[NavGroup] = &[
    NavGroup {
        id: "chat",
        label: "Chat",
        legacy_label: Some("Infer"),
        icon: ICON_CHAT,
        alt_shortcut: Some(1),
        items: &[NavItem::new("chat", "Chat", "/chat").with_keywords(&[
            "inference",
            "generate",
            "prompt",
            "conversation",
            "chat",
            "history",
        ])],
        collapsed_by_default: false,
        show_in_taskbar: true,
        show_in_mobile: true,
    },
    NavGroup {
        id: "build",
        label: "Build",
        legacy_label: None,
        icon: ICON_FLAME,
        alt_shortcut: Some(2),
        items: &[
            NavItem::new("training", "Build", "/training?open_wizard=1").with_keywords(&[
                "create",
                "adapter",
                "train",
                "files",
                "knowledge",
            ]),
            NavItem::new("adapters", "Adapters", "/adapters").with_keywords(&[
                "lora",
                "finetune",
                "weights",
                "models",
                "lifecycle",
            ]),
            NavItem::new("models", "Models", "/models").with_keywords(&[
                "llm",
                "foundation",
                "base",
                "weights",
            ]),
            NavItem::new("datasets", "Datasets", "/datasets").with_keywords(&[
                "dataset",
                "datasets",
                "training data",
                "validation",
                "trust",
            ]),
        ],
        collapsed_by_default: false,
        show_in_taskbar: true,
        show_in_mobile: true,
    },
    NavGroup {
        id: "evidence",
        label: "Execution Records",
        legacy_label: Some("Verify"),
        icon: ICON_EYE,
        alt_shortcut: Some(3),
        items: &[
            NavItem::new("runs", "Execution Records", "/runs").with_keywords(&[
                "flight",
                "recorder",
                "traces",
                "provenance",
                "receipts",
                "runs",
                "evidence",
            ]),
        ],
        collapsed_by_default: false,
        show_in_taskbar: true,
        show_in_mobile: true,
    },
    NavGroup {
        id: "versions",
        label: "Versions",
        legacy_label: Some("Promote"),
        icon: ICON_ROCKET,
        alt_shortcut: Some(4),
        items: &[
            NavItem::new("update_center", "Versions", "/update-center").with_keywords(&[
                "promote",
                "run promote",
                "production",
                "draft",
                "reviewed",
                "checkout",
                "run checkout",
                "version history",
                "feed-dataset",
                "versions",
            ]),
        ],
        collapsed_by_default: false,
        show_in_taskbar: true,
        show_in_mobile: true,
    },
    NavGroup {
        id: "system",
        label: "System",
        legacy_label: Some("Deploy"),
        icon: ICON_SHIELD,
        alt_shortcut: Some(5),
        items: &[
            NavItem::new("system", "System", "/system").with_keywords(&[
                "health",
                "status",
                "diagnostics",
                "infrastructure",
                "system",
            ]),
            NavItem::new("workers", "Workers", "/workers").with_keywords(&[
                "runtime",
                "instances",
                "compute",
                "nodes",
                "process",
            ]),
            NavItem::new("settings", "Settings", "/settings").with_keywords(&[
                "config",
                "preferences",
                "options",
                "profile",
            ]),
        ],
        collapsed_by_default: false,
        show_in_taskbar: true,
        show_in_mobile: true,
    },
];

// ===========================================================================
// PUBLIC API
// ===========================================================================

pub fn nav_groups(profile: UiProfile) -> Vec<&'static NavGroup> {
    match profile {
        UiProfile::Primary => NAV_GROUPS_PRIMARY.iter().collect(),
        UiProfile::Full => NAV_GROUPS_FULL.iter().collect(),
        UiProfile::Hud => vec![],
    }
}

pub fn route_for_alt_shortcut(profile: UiProfile, shortcut: u8) -> Option<&'static str> {
    nav_groups(profile)
        .into_iter()
        .find(|g| g.alt_shortcut == Some(shortcut))
        .and_then(|g| g.items.first())
        .map(|item| item.route)
}

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

fn normalize_route(route: &str) -> &str {
    route.split('?').next().unwrap_or(route)
}

/// Find the owning nav group for a route in the current profile.
pub fn nav_group_for_route(profile: UiProfile, route: &str) -> Option<&'static NavGroup> {
    let route = normalize_route(route);
    nav_groups(profile).into_iter().find(|group| {
        group
            .items
            .iter()
            .any(|item| normalize_route(item.route) == route)
    })
}

/// Resolve the profile-aware top-level label for a route.
pub fn nav_group_label_for_route(profile: UiProfile, route: &str) -> Option<&'static str> {
    nav_group_for_route(profile, route).map(|group| group.label)
}

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

#[derive(Debug, Clone)]
pub struct MobileNavItem {
    pub label: &'static str,
    pub href: &'static str,
    pub icon: &'static str,
}

pub fn is_route_active(current_path: &str, target_route: &str) -> bool {
    let target_route = normalize_route(target_route);
    if target_route == "/" {
        current_path == "/" || current_path == "/dashboard"
    } else {
        current_path == target_route || current_path.starts_with(&format!("{}/", target_route))
    }
}
