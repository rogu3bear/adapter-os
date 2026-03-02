//! Users section component — flat user list with role badges.

use crate::api::{ApiClient, UserResponse};
use crate::components::{
    Badge, BadgeVariant, Button, ButtonVariant, Card, EmptyState, EmptyStateVariant, ErrorDisplay,
    PaginationControls, SkeletonTable, Table, TableBody, TableCell, TableHead, TableHeader,
    TableRow,
};
use crate::hooks::{use_api_resource, use_polling, LoadingState};
use crate::signals::{use_refetch_signal, RefetchTopic};
use crate::utils::humanize;
use leptos::prelude::*;
use std::sync::Arc;

const USERS_PER_PAGE: usize = 25;

/// Users section — flat table with role badge, search, and pagination.
#[component]
pub fn UsersSection() -> impl IntoView {
    // Search/filter state
    let search_query = RwSignal::new(String::new());

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

    // Derived: all users vec (unfiltered)
    let all_users = Signal::derive(move || -> Vec<UserResponse> {
        match users.get() {
            LoadingState::Loaded(data) => data.users,
            _ => Vec::new(),
        }
    });

    // Filtered users based on search query
    let filtered_users = Signal::derive(move || -> Vec<UserResponse> {
        let query = search_query.get();
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return all_users.get();
        }
        all_users
            .get()
            .into_iter()
            .filter(|u| {
                u.email.to_lowercase().contains(&query)
                    || u.display_name.to_lowercase().contains(&query)
            })
            .collect()
    });

    // Reset to page 1 when search changes
    Effect::new(move || {
        let _ = search_query.get();
        current_page.set(1);
    });

    // Pagination math
    let total_users = Signal::derive(move || filtered_users.get().len());
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
        let all = filtered_users.get();
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
                            <div class="flex items-center gap-3">
                                <input
                                    type="search"
                                    class="form-input flex-1"
                                    placeholder="Filter by name or email…"
                                    aria_label="Filter users by name or email"
                                    prop:value=move || search_query.get()
                                    on:input=move |e| {
                                        use wasm_bindgen::JsCast;
                                        let val = e.target()
                                            .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
                                            .map(|el| el.value())
                                            .unwrap_or_default();
                                        search_query.set(val);
                                    }
                                />
                                <span class="text-sm text-muted-foreground whitespace-nowrap">
                                    {move || {
                                        let visible = total_users.get();
                                        let total = all_users.get().len();
                                        if search_query.get().trim().is_empty() {
                                            format!("{} users", total)
                                        } else {
                                            format!("{} of {}", visible, total)
                                        }
                                    }}
                                </span>
                                <Button
                                    variant=ButtonVariant::Outline
                                    on_click=refetch.as_callback()
                                >
                                    "Refresh"
                                </Button>
                            </div>

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
                                                let role_variant = match user.role.as_str() {
                                                    "admin" => BadgeVariant::Destructive,
                                                    "operator" => BadgeVariant::Warning,
                                                    _ => BadgeVariant::Secondary,
                                                };
                                                let last_login = user.last_login_at.clone()
                                                    .unwrap_or_else(|| "Never".to_string());

                                                view! {
                                                    <TableRow>
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
                                                    </TableRow>
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
