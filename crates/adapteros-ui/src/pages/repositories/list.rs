//! Repository list components

use super::helpers::format_date;
use crate::api::RepositoryInfo;
use crate::components::{
    Badge, BadgeVariant, Card, EmptyState, EmptyStateVariant, Table, TableBody, TableCell,
    TableHead, TableHeader, TableRow,
};
use crate::constants::urls::docs_link;
use leptos::prelude::*;

/// Repository list table
#[component]
pub fn RepositoryList(
    repos: Vec<RepositoryInfo>,
    selected_id: RwSignal<Option<String>>,
) -> impl IntoView {
    if repos.is_empty() {
        return view! {
            <Card>
                <EmptyState
                    variant=EmptyStateVariant::Empty
                    title="No repositories found"
                    description="Register a code repository to enable code intelligence and adapter training from your codebase."
                    secondary_label="Learn about Code Intelligence"
                    secondary_href=docs_link("code-intelligence")
                />
            </Card>
        }
        .into_any();
    }

    view! {
        <Card>
            <Table>
                <TableHeader>
                    <TableRow>
                        <TableHead>"Repository"</TableHead>
                        <TableHead>"Languages"</TableHead>
                        <TableHead>"Status"</TableHead>
                        <TableHead>"Last Scan"</TableHead>
                        <TableHead>"Created"</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {repos
                        .into_iter()
                        .map(|repo| {
                            let repo_id = repo.repo_id.clone();
                            let repo_id_for_click = repo_id.clone();
                            let languages_display = if repo.languages.len() > 3 {
                                format!("{} +{}", repo.languages[..3].join(", "), repo.languages.len() - 3)
                            } else {
                                repo.languages.join(", ")
                            };
                            let last_scan = repo
                                .latest_scan_at
                                .as_deref()
                                .map(format_date)
                                .unwrap_or_else(|| "Never".to_string());

                            view! {
                                <tr
                                    class="border-b transition-colors hover:bg-muted/50 cursor-pointer"
                                    class:bg-muted=move || selected_id.get().as_ref() == Some(&repo_id)
                                    on:click=move |_| selected_id.set(Some(repo_id_for_click.clone()))
                                >
                                    <TableCell>
                                        <div>
                                            <p class="font-medium">{repo.repo_id.clone()}</p>
                                            <p class="text-xs text-muted-foreground truncate max-w-xs">
                                                {repo.path.clone()}
                                            </p>
                                        </div>
                                    </TableCell>
                                    <TableCell>
                                        <span class="text-sm">{languages_display}</span>
                                    </TableCell>
                                    <TableCell>
                                        <RepoStatusBadge status=repo.status.clone()/>
                                    </TableCell>
                                    <TableCell>
                                        <span class="text-sm text-muted-foreground">
                                            {last_scan}
                                        </span>
                                    </TableCell>
                                    <TableCell>
                                        <span class="text-sm text-muted-foreground">
                                            {format_date(&repo.created_at)}
                                        </span>
                                    </TableCell>
                                </tr>
                            }
                        })
                        .collect::<Vec<_>>()}
                </TableBody>
            </Table>
        </Card>
    }
    .into_any()
}

/// Repository status badge
#[component]
pub fn RepoStatusBadge(status: String) -> impl IntoView {
    let (variant, label) = match status.as_str() {
        "active" => (BadgeVariant::Success, "Active"),
        "scanning" => (BadgeVariant::Default, "Scanning"),
        "pending" => (BadgeVariant::Secondary, "Pending"),
        "error" => (BadgeVariant::Destructive, "Error"),
        "syncing" => (BadgeVariant::Default, "Syncing"),
        _ => (BadgeVariant::Secondary, "Unknown"),
    };

    view! {
        <Badge variant=variant>
            {label}
        </Badge>
    }
}
