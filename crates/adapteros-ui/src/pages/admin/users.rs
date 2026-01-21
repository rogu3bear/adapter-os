//! Users section component

use crate::api::{ApiClient, UserResponse};
use crate::components::{
    Badge, BadgeVariant, Button, ButtonVariant, Card, ErrorDisplay, Spinner, Table, TableBody,
    TableCell, TableHead, TableHeader, TableRow,
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
                                <div class="py-8 text-center">
                                    <div class="rounded-full bg-muted p-3 mx-auto w-fit mb-4">
                                        <svg
                                            xmlns="http://www.w3.org/2000/svg"
                                            class="h-8 w-8 text-muted-foreground"
                                            viewBox="0 0 24 24"
                                            fill="none"
                                            stroke="currentColor"
                                            stroke-width="1.5"
                                        >
                                            <path stroke-linecap="round" stroke-linejoin="round" d="M15 19.128a9.38 9.38 0 002.625.372 9.337 9.337 0 004.121-.952 4.125 4.125 0 00-7.533-2.493M15 19.128v-.003c0-1.113-.285-2.16-.786-3.07M15 19.128v.106A12.318 12.318 0 018.624 21c-2.331 0-4.512-.645-6.374-1.766l-.001-.109a6.375 6.375 0 0111.964-3.07M12 6.375a3.375 3.375 0 11-6.75 0 3.375 3.375 0 016.75 0zm8.25 2.25a2.625 2.625 0 11-5.25 0 2.625 2.625 0 015.25 0z"/>
                                        </svg>
                                    </div>
                                    <h3 class="text-lg font-medium mb-2">"No Users Found"</h3>
                                    <p class="text-muted-foreground max-w-md mx-auto">
                                        "No users are registered in the system yet."
                                    </p>
                                </div>
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
                                            on_click=refetch
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
                                on_retry=refetch
                            />
                        }.into_any()
                    }
                }
            }}
        </Card>
    }
}
