//! Shared navigation registry for taskbar, start menu, and mobile nav.

use adapteros_api_types::UiProfile;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NavScope {
    /// Show only in Primary profile
    PrimaryOnly,
    /// Show only in Full profile
    FullOnly,
}

impl NavScope {
    fn allows(self, profile: UiProfile) -> bool {
        matches!(
            (self, profile),
            (NavScope::PrimaryOnly, UiProfile::Primary) | (NavScope::FullOnly, UiProfile::Full)
        )
    }
}

#[derive(Debug, Clone)]
pub struct TaskbarModuleItem {
    pub label: &'static str,
    pub href: &'static str,
    pub icon: &'static str,
    pub routes: &'static [&'static str],
}

#[derive(Debug, Clone)]
pub struct StartMenuModule {
    pub name: &'static str,
    pub icon: &'static str,
    pub items: &'static [(&'static str, &'static str)],
    pub collapsed: bool,
}

#[derive(Debug, Clone)]
pub struct MobileNavItem {
    pub label: &'static str,
    pub href: &'static str,
    pub icon: &'static str,
}

#[derive(Debug, Clone)]
struct NavModuleDefinition {
    label: &'static str,
    href: &'static str,
    icon: &'static str,
    routes: &'static [&'static str],
    start_menu_items: &'static [(&'static str, &'static str)],
    start_menu_collapsed: bool,
    include_in_taskbar: bool,
    include_in_mobile: bool,
    scope: NavScope,
}

const CHAT_ICON: &str = "M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z";

const NAV_MODULES: &[NavModuleDefinition] = &[
    NavModuleDefinition {
        label: "Operate",
        href: "/",
        icon: "M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z",
        routes: &["/", "/dashboard", "/system", "/workers", "/monitoring", "/errors"],
        start_menu_items: &[
            ("Dashboard", "/"),
            ("System", "/system"),
            ("Workers", "/workers"),
            ("Monitoring", "/monitoring"),
            ("Errors", "/errors"),
        ],
        start_menu_collapsed: false,
        include_in_taskbar: true,
        include_in_mobile: true,
        scope: NavScope::FullOnly,
    },
    NavModuleDefinition {
        label: "Build",
        href: "/training",
        icon: "M19.428 15.428a2 2 0 00-1.022-.547l-2.387-.477a6 6 0 00-3.86.517l-.318.158a6 6 0 01-3.86.517L6.05 15.21a2 2 0 00-1.806.547M8 4h8l-1 1v5.172a2 2 0 00.586 1.414l5 5c1.26 1.26.367 3.414-1.415 3.414H4.828c-1.782 0-2.674-2.154-1.414-3.414l5-5A2 2 0 009 10.172V5L8 4z",
        routes: &["/training", "/agents"],
        start_menu_items: &[("Training", "/training"), ("Agents", "/agents")],
        start_menu_collapsed: false,
        include_in_taskbar: true,
        include_in_mobile: true,
        scope: NavScope::FullOnly,
    },
    NavModuleDefinition {
        label: "Configure",
        href: "/adapters",
        icon: "M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z",
        routes: &["/adapters", "/stacks", "/policies", "/models"],
        start_menu_items: &[
            ("Adapters", "/adapters"),
            ("Runtime Stacks", "/stacks"),
            ("Policies", "/policies"),
            ("Models", "/models"),
        ],
        start_menu_collapsed: false,
        include_in_taskbar: true,
        include_in_mobile: true,
        scope: NavScope::FullOnly,
    },
    NavModuleDefinition {
        label: "Data",
        href: "/datasets",
        icon: "M4 7v10c0 2.21 3.582 4 8 4s8-1.79 8-4V7M4 7c0 2.21 3.582 4 8 4s8-1.79 8-4M4 7c0-2.21 3.582-4 8-4s8 1.79 8 4m0 5c0 2.21-3.582 4-8 4s-8-1.79-8-4",
        routes: &["/datasets", "/documents", "/repositories", "/collections"],
        start_menu_items: &[
            ("Datasets", "/datasets"),
            ("Documents", "/documents"),
            ("Collections", "/collections"),
            ("Repositories", "/repositories"),
        ],
        start_menu_collapsed: false,
        include_in_taskbar: true,
        include_in_mobile: true,
        scope: NavScope::FullOnly,
    },
    NavModuleDefinition {
        label: "Verify",
        href: "/audit",
        icon: "M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z",
        routes: &["/audit", "/runs", "/reviews"],
        start_menu_items: &[("Audit", "/audit"), ("Runs", "/runs"), ("Reviews", "/reviews")],
        start_menu_collapsed: false,
        include_in_taskbar: true,
        include_in_mobile: true,
        scope: NavScope::FullOnly,
    },
    NavModuleDefinition {
        label: "Org",
        href: "/admin",
        icon: "M19 21V5a2 2 0 00-2-2H7a2 2 0 00-2 2v16m14 0h2m-2 0h-5m-9 0H3m2 0h5M9 7h1m-1 4h1m4-4h1m-1 4h1m-5 10v-5a1 1 0 011-1h2a1 1 0 011 1v5m-4 0h4",
        routes: &["/admin"],
        start_menu_items: &[
            ("Users", "/admin"),
            ("Roles", "/admin?tab=roles"),
            ("API Keys", "/admin?tab=keys"),
            ("Organization", "/admin?tab=org"),
        ],
        start_menu_collapsed: false,
        include_in_taskbar: true,
        include_in_mobile: true,
        scope: NavScope::FullOnly,
    },
    NavModuleDefinition {
        label: "Chat",
        href: "/chat",
        icon: CHAT_ICON,
        routes: &["/chat"],
        start_menu_items: &[("Chat", "/chat")],
        start_menu_collapsed: false,
        include_in_taskbar: true,
        include_in_mobile: true,
        scope: NavScope::FullOnly,
    },
    NavModuleDefinition {
        label: "Debug",
        href: "/routing",
        icon: "M10.343 3.94c.09-.542.56-.94 1.11-.94h1.093c.55 0 1.02.398 1.11.94l.149.894c.07.424.384.764.78.93.398.164.855.142 1.205-.108l.737-.527a1.125 1.125 0 011.45.12l.773.774c.39.389.44 1.002.12 1.45l-.527.737c-.25.35-.272.806-.107 1.204.165.397.505.71.93.78l.893.15c.543.09.94.56.94 1.109v1.094c0 .55-.397 1.02-.94 1.11l-.893.149c-.425.07-.765.383-.93.78-.165.398-.143.854.107 1.204l.527.738c.32.447.269 1.06-.12 1.45l-.774.773a1.125 1.125 0 01-1.449.12l-.738-.527c-.35-.25-.806-.272-1.203-.107-.397.165-.71.505-.781.929l-.149.894c-.09.542-.56.94-1.11.94h-1.094c-.55 0-1.019-.398-1.11-.94l-.148-.894c-.071-.424-.384-.764-.781-.93-.398-.164-.854-.142-1.204.108l-.738.527c-.447.32-1.06.269-1.45-.12l-.773-.774a1.125 1.125 0 01-.12-1.45l.527-.737c.25-.35.273-.806.108-1.204-.165-.397-.505-.71-.93-.78l-.894-.15c-.542-.09-.94-.56-.94-1.109v-1.094c0-.55.398-1.02.94-1.11l.894-.149c.424-.07.765-.383.93-.78.165-.398.143-.854-.107-1.204l-.527-.738a1.125 1.125 0 01.12-1.45l.773-.773a1.125 1.125 0 011.45-.12l.737.527c.35.25.807.272 1.204.107.397-.165.71-.505.78-.929l.15-.894z",
        routes: &["/routing", "/diff"],
        start_menu_items: &[("Routing Debug", "/routing"), ("Run Diff", "/diff")],
        start_menu_collapsed: true,
        include_in_taskbar: false,
        include_in_mobile: false,
        scope: NavScope::FullOnly,
    },
    NavModuleDefinition {
        label: "Account",
        href: "/settings",
        icon: "M5.121 17.804A13.937 13.937 0 0112 16c2.5 0 4.847.655 6.879 1.804M15 10a3 3 0 11-6 0 3 3 0 016 0zm6 2a9 9 0 11-18 0 9 9 0 0118 0z",
        routes: &["/user", "/settings"],
        start_menu_items: &[("Profile", "/user"), ("Preferences", "/settings")],
        start_menu_collapsed: true,
        include_in_taskbar: false,
        include_in_mobile: false,
        scope: NavScope::FullOnly,
    },
    NavModuleDefinition {
        label: "Chat",
        href: "/chat",
        icon: CHAT_ICON,
        routes: &["/chat"],
        start_menu_items: &[("Chat", "/chat")],
        start_menu_collapsed: false,
        include_in_taskbar: true,
        include_in_mobile: true,
        scope: NavScope::PrimaryOnly,
    },
    NavModuleDefinition {
        label: "Runs",
        href: "/runs",
        icon: "M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z",
        routes: &["/runs"],
        start_menu_items: &[("Runs", "/runs")],
        start_menu_collapsed: false,
        include_in_taskbar: true,
        include_in_mobile: true,
        scope: NavScope::PrimaryOnly,
    },
    NavModuleDefinition {
        label: "Models",
        href: "/models",
        icon: "M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z",
        routes: &["/models"],
        start_menu_items: &[("Models", "/models")],
        start_menu_collapsed: false,
        include_in_taskbar: true,
        include_in_mobile: true,
        scope: NavScope::PrimaryOnly,
    },
];

// Primary lane contract (acceptance checklist)
// - Auth to Chat: `/login` -> `/chat` with `data-testid="chat-input"` ready.
//   Implementation: `crates/adapteros-ui/src/pages/chat.rs`.
//   Test: `tests/playwright/ui/primary.lane.spec.ts`.
// - Inference (streaming + non-streaming as available) completes with response.
//   Implementation: `crates/adapteros-ui/src/signals/chat.rs`.
//   Test: `tests/playwright/ui/primary.lane.spec.ts`.
// - Run detail view shows provenance summary + receipt verification widget.
//   Implementation: `crates/adapteros-ui/src/pages/flight_recorder.rs`,
//   `crates/adapteros-ui/src/components/trace_viewer.rs`.
//   Test: `tests/playwright/ui/primary.lane.spec.ts`.
// - Token decisions view supports safe paging with render cap + "Show more".
//   Implementation: `crates/adapteros-ui/src/components/trace_viewer.rs`.
//   Test: `tests/playwright/ui/primary.lane.spec.ts`.
// - Model readiness path is available when inference is blocked.
//   Implementation: `crates/adapteros-ui/src/components/inference_guidance.rs`,
//   `crates/adapteros-ui/src/pages/chat.rs`.
//   Test: `tests/playwright/ui/primary.lane.spec.ts`.
const PRIMARY_NAV_ORDER: &[&str] = &["Chat", "Runs", "Models"];

fn ordered_modules(profile: UiProfile) -> Vec<&'static NavModuleDefinition> {
    let mut modules: Vec<_> = NAV_MODULES
        .iter()
        .filter(|module| module.scope.allows(profile))
        .collect();
    if profile == UiProfile::Primary {
        modules.sort_by_key(|module| {
            PRIMARY_NAV_ORDER
                .iter()
                .position(|label| *label == module.label)
                .unwrap_or(usize::MAX)
        });
    }
    modules
}

pub fn build_taskbar_modules(profile: UiProfile) -> Vec<TaskbarModuleItem> {
    ordered_modules(profile)
        .into_iter()
        .filter(|module| module.include_in_taskbar)
        .map(|module| TaskbarModuleItem {
            label: module.label,
            href: module.href,
            icon: module.icon,
            routes: module.routes,
        })
        .collect()
}

pub fn build_start_menu_modules(profile: UiProfile) -> Vec<StartMenuModule> {
    ordered_modules(profile)
        .into_iter()
        .filter(|module| !module.start_menu_items.is_empty())
        .map(|module| StartMenuModule {
            name: module.label,
            icon: module.icon,
            items: module.start_menu_items,
            collapsed: module.start_menu_collapsed,
        })
        .collect()
}

pub fn build_mobile_nav_items(profile: UiProfile) -> Vec<MobileNavItem> {
    ordered_modules(profile)
        .into_iter()
        .filter(|module| module.include_in_mobile)
        .map(|module| MobileNavItem {
            label: module.label,
            href: module.href,
            icon: module.icon,
        })
        .collect()
}
