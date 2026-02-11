//! Users section component

use crate::api::{ApiClient, UserResponse};
use crate::components::{
    Badge, BadgeVariant, Button, ButtonVariant, Card, EmptyState, EmptyStateVariant, ErrorDisplay,
    Spinner, Table, TableBody, TableCell, TableHead, TableHeader, TableRow,
};
use crate::hooks::{use_api_resource, LoadingState};
use leptos::prelude::*;
use std::sync::Arc;

/// Users section - fetches real user data from API
#[component]
pub fn UsersSection() -> impl IntoView {
    // Fetch users from API
    let (users, refetch) = use_api_resource(move |client: Arc<ApiClient>| async move {
        client.list_users(Some(1), Some(50)).await
    });

    view! {
        <Card>
            {move || {
                match users.get() {
                    LoadingState::Idle | LoadingState::Loading => {
                        view! {
                            <div class="flex items-center justify-center py-12">
                                <Spinner/>
                            </div>
                        }.into_any()
                    }
                    LoadingState::Loaded(data) => {
                        if data.users.is_empty() {
                            view! {
                                <EmptyState
                                    title="No Users Found"
                                    description="No users are registered in the system yet."
                                    variant=EmptyStateVariant::Empty
                                />
                            }.into_any()
                        } else {
                            let users_list: Vec<UserResponse> = data.users;
                            view! {
                                <div>
                                    <div class="flex items-center justify-between mb-4">
                                        <span class="text-sm text-muted-foreground">
                                            {format!("{} users total", data.total)}
                                        </span>
                                        <Button
                                            variant=ButtonVariant::Outline
                                            on_click=refetch.as_callback()
                                        >
                                            "Refresh"
                                        </Button>
                                    </div>
                                    <Table>
                                        <TableHeader>
                                            <TableRow>
                                                <TableHead>"Email"</TableHead>
                                                <TableHead>"Display Name"</TableHead>
                                                <TableHead>"Role"</TableHead>
                                                <TableHead>"Last Login"</TableHead>
                                            </TableRow>
                                        </TableHeader>
                                        <TableBody>
                                            {users_list.into_iter().map(|user| {
                                                let role_variant = match user.role.as_str() {
                                                    "admin" => BadgeVariant::Destructive,
                                                    "operator" => BadgeVariant::Warning,
                                                    _ => BadgeVariant::Secondary,
                                                };
                                                let last_login = user.last_login_at.clone().unwrap_or_else(|| "Never".to_string());
                                                view! {
                                                    <TableRow>
                                                        <TableCell>
                                                            <span>{user.email.clone()}</span>
                                                        </TableCell>
                                                        <TableCell>
                                                            <span>{user.display_name.clone()}</span>
                                                        </TableCell>
                                                        <TableCell>
                                                            <Badge variant=role_variant>{user.role.clone()}</Badge>
                                                        </TableCell>
                                                        <TableCell>
                                                            <span class="text-sm text-muted-foreground">{last_login}</span>
                                                        </TableCell>
                                                    </TableRow>
                                                }
                                            }).collect::<Vec<_>>()}
                                        </TableBody>
                                    </Table>
                                </div>
                            }.into_any()
                        }
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
        </Card>
    }
}
