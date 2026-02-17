//! Users section component with SplitPanel list-detail layout.

use crate::api::{ApiClient, UserResponse};
use crate::components::{
    use_split_panel_selection_state, Badge, BadgeVariant, Button, ButtonVariant, Card, EmptyState,
    EmptyStateVariant, ErrorDisplay, PaginationControls, SkeletonTable, SplitPanel, SplitRatio,
    Table, TableBody, TableCell, TableHead, TableHeader, TableRow,
};
use crate::hooks::{use_api_resource, use_polling, LoadingState};
use crate::signals::{use_refetch_signal, RefetchTopic};
use crate::utils::{format_datetime, humanize};
use leptos::prelude::*;
use std::sync::Arc;

const USERS_PER_PAGE: usize = 25;

/// Users section - fetches real user data from API with SplitPanel detail view.
#[component]
pub fn UsersSection() -> impl IntoView {
    let sel = use_split_panel_selection_state();
    let selected_id = sel.selected_id;

    // Pagination state
    let current_page = RwSignal::new(1usize);

    let (users, refetch) = use_api_resource(move |client: Arc<ApiClient>| async move {
        // Fetch a large page; client-side pagination handles display.
        client.list_users(Some(1), Some(500)).await
    });

    // Periodic polling (30s)
    let refetch_poll = refetch;
    let _cancel_poll = use_polling(30_000, move || {
        refetch_poll.run(());
        async {}
    });

    // SSE-driven refetch
    let users_counter = use_refetch_signal(RefetchTopic::Users);
    Effect::new(move || {
        let _ = users_counter.get();
        refetch.run(());
    });

    // Derived: all users vec
    let all_users = Signal::derive(move || -> Vec<UserResponse> {
        match users.get() {
            LoadingState::Loaded(data) => data.users,
            _ => Vec::new(),
        }
    });

    // Pagination math
    let total_users = Signal::derive(move || all_users.get().len());
    let total_pages = Signal::derive(move || {
        let count = total_users.get();
        if count == 0 {
            1
        } else {
            count.div_ceil(USERS_PER_PAGE)
        }
    });

    // Clamp page when data changes
    Effect::new(move || {
        let max = total_pages.get();
        if current_page.get_untracked() > max {
            current_page.set(max);
        }
    });

    let visible_users = Signal::derive(move || {
        let all = all_users.get();
        let page = current_page.get();
        let start = (page - 1) * USERS_PER_PAGE;
        all.into_iter()
            .skip(start)
            .take(USERS_PER_PAGE)
            .collect::<Vec<_>>()
    });

    view! {
        {move || {
            match users.get() {
                LoadingState::Idle | LoadingState::Loading => {
                    view! {
                        <Card>
                            <SkeletonTable rows=5 columns=4/>
                        </Card>
                    }.into_any()
                }
                LoadingState::Loaded(_) => {
                    let total = total_users.get();
                    if total == 0 {
                        return view! {
                            <Card>
                                <EmptyState
                                    title="No Users Found"
                                    description="No users are registered in the system yet."
                                    variant=EmptyStateVariant::Empty
                                />
                            </Card>
                        }.into_any();
                    }

                    view! {
                        <div class="space-y-4">
                            <div class="flex items-center justify-between">
                                <span class="text-sm text-muted-foreground">
                                    {move || format!("{} users total", total_users.get())}
                                </span>
                                <Button
                                    variant=ButtonVariant::Outline
                                    on_click=refetch.as_callback()
                                >
                                    "Refresh"
                                </Button>
                            </div>

                            <SplitPanel
                                has_selection=sel.has_selection
                                on_close=sel.on_close
                                back_label="Back to Users"
                                ratio=SplitRatio::TwoFifthsThreeFifths
                                list_panel=move || {
                                    let on_select = sel.on_select;
                                    view! {
                                        <Card>
                                            <Table>
                                                <TableHeader>
                                                    <TableRow>
                                                        <TableHead>"Email"</TableHead>
                                                        <TableHead>"Name"</TableHead>
                                                        <TableHead>"Role"</TableHead>
                                                        <TableHead>"Last Login"</TableHead>
                                                    </TableRow>
                                                </TableHeader>
                                                <TableBody>
                                                    {move || {
                                                        visible_users.get().into_iter().map(|user| {
                                                            let user_id = user.id.clone();
                                                            let user_id_click = user.id.clone();
                                                            let role_variant = match user.role.as_str() {
                                                                "admin" => BadgeVariant::Destructive,
                                                                "operator" => BadgeVariant::Warning,
                                                                _ => BadgeVariant::Secondary,
                                                            };
                                                            let last_login = user.last_login_at.clone()
                                                                .unwrap_or_else(|| "Never".to_string());
                                                            let user_id_key = user_id_click.clone();

                                                            view! {
                                                                <tr
                                                                    class="border-b transition-colors hover:bg-muted/50 cursor-pointer"
                                                                    class:bg-muted=move || selected_id.try_get().flatten().as_ref() == Some(&user_id)
                                                                    on:click=move |_| on_select.run(user_id_click.clone())
                                                                    on:keydown=move |e: web_sys::KeyboardEvent| {
                                                                        if e.key() == "Enter" || e.key() == " " {
                                                                            e.prevent_default();
                                                                            on_select.run(user_id_key.clone());
                                                                        }
                                                                    }
                                                                    role="button"
                                                                    tabindex=0
                                                                >
                                                                    <TableCell>
                                                                        <span class="font-medium">{user.email.clone()}</span>
                                                                    </TableCell>
                                                                    <TableCell>
                                                                        <span>{user.display_name.clone()}</span>
                                                                    </TableCell>
                                                                    <TableCell>
                                                                        <Badge variant=role_variant>{humanize(&user.role)}</Badge>
                                                                    </TableCell>
                                                                    <TableCell>
                                                                        <span class="text-sm text-muted-foreground">{last_login}</span>
                                                                    </TableCell>
                                                                </tr>
                                                            }
                                                        }).collect::<Vec<_>>()
                                                    }}
                                                </TableBody>
                                            </Table>

                                            // Pagination
                                            {move || {
                                                let tp = total_pages.get();
                                                let cp = current_page.get();
                                                let ti = total_users.get();
                                                view! {
                                                    <PaginationControls
                                                        current_page=cp
                                                        total_pages=tp
                                                        total_items=ti
                                                        on_prev=Callback::new(move |_| {
                                                            current_page.update(|p| *p = p.saturating_sub(1).max(1));
                                                        })
                                                        on_next=Callback::new(move |_| {
                                                            let max = total_pages.get();
                                                            current_page.update(|p| *p = (*p + 1).min(max));
                                                        })
                                                    />
                                                }
                                            }}
                                        </Card>
                                    }
                                }
                                detail_panel=move || {
                                    view! {
                                        {move || {
                                            let uid = selected_id.get();
                                            uid.and_then(|id| {
                                                all_users.get().into_iter().find(|u| u.id == id)
                                            }).map(|user| {
                                                view! {
                                                    <UserDetailPanel
                                                        user=user
                                                        on_close=move || selected_id.set(None)
                                                    />
                                                }
                                            })
                                        }}
                                    }
                                }
                            />
                        </div>
                    }.into_any()
                }
                LoadingState::Error(e) => {
                    view! {
                        <ErrorDisplay
                            error=e
                            on_retry=refetch.as_callback()
                        />
                    }.into_any()
                }
            }
        }}
    }
}

/// Detail panel for a selected user.
#[component]
fn UserDetailPanel(user: UserResponse, on_close: impl Fn() + Copy + 'static) -> impl IntoView {
    let role_variant = match user.role.as_str() {
        "admin" => BadgeVariant::Destructive,
        "operator" => BadgeVariant::Warning,
        _ => BadgeVariant::Secondary,
    };

    let last_login = user
        .last_login_at
        .as_deref()
        .map(format_datetime)
        .unwrap_or_else(|| "Never".to_string());

    let created = format_datetime(&user.created_at);
    let mfa_label = user
        .mfa_enabled
        .map(|enabled| if enabled { "Enabled" } else { "Disabled" })
        .unwrap_or("Unknown");

    let permissions = user.permissions.clone().unwrap_or_default();

    view! {
        <div class="space-y-4">
            // Header with close button
            <div class="flex items-center justify-between">
                <h2 class="heading-3">"User Details"</h2>
                <button
                    class="text-muted-foreground hover:text-foreground"
                    on:click=move |_| on_close()
                    aria-label="Close"
                >
                    <svg
                        xmlns="http://www.w3.org/2000/svg"
                        width="24"
                        height="24"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                    >
                        <path d="M18 6 6 18"/>
                        <path d="m6 6 12 12"/>
                    </svg>
                </button>
            </div>

            <Card>
                <div class="space-y-4">
                    // Name + email
                    <div>
                        <h3 class="heading-4">{user.display_name.clone()}</h3>
                        <p class="text-sm text-muted-foreground">{user.email.clone()}</p>
                    </div>

                    // Role badge
                    <div class="flex items-center gap-2">
                        <span class="text-sm font-medium">"Role:"</span>
                        <Badge variant=role_variant>{humanize(&user.role)}</Badge>
                    </div>

                    // Info grid
                    <div class="grid grid-cols-2 gap-3 text-sm">
                        <div>
                            <span class="text-muted-foreground">"Created"</span>
                            <p class="font-medium">{created}</p>
                        </div>
                        <div>
                            <span class="text-muted-foreground">"Last Login"</span>
                            <p class="font-medium">{last_login}</p>
                        </div>
                        <div>
                            <span class="text-muted-foreground">"Tenant"</span>
                            <p class="font-mono text-xs">{user.tenant_id.clone()}</p>
                        </div>
                        <div>
                            <span class="text-muted-foreground">"MFA"</span>
                            <p class="font-medium">{mfa_label}</p>
                        </div>
                    </div>

                    // Permissions list
                    {if !permissions.is_empty() {
                        view! {
                            <div>
                                <span class="text-sm font-medium">"Permissions"</span>
                                <div class="flex flex-wrap gap-1 mt-1">
                                    {permissions.into_iter().map(|p| view! {
                                        <Badge variant=BadgeVariant::Outline>{p}</Badge>
                                    }).collect::<Vec<_>>()}
                                </div>
                            </div>
                        }.into_any()
                    } else {
                        view! {}.into_any()
                    }}
                </div>
            </Card>
        </div>
    }
}
