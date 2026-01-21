//! Repository list components

use super::helpers::{format_date, format_number};
use crate::api::RepositoryResponse;
use crate::components::{
    Badge, BadgeVariant, Card, Table, TableBody, TableCell, TableHead, TableHeader, TableRow,
};
use leptos::prelude::*;

/// Repository list table
#[component]
pub fn RepositoryList(
    repos: Vec<RepositoryResponse>,
    selected_id: RwSignal<Option<String>>,
) -> impl IntoView {
    if repos.is_empty() {
        return view! {
            <Card>
                <div class="py-8 text-center">
                    <p class="text-muted-foreground">"No repositories found. Register one to get started."</p>
                </div>
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
                        <TableHead>"Files"</TableHead>
                        <TableHead>"Updated"</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {repos
                        .into_iter()
                        .map(|repo| {
                            let repo_id = repo.id.clone();
                            let repo_id_for_click = repo_id.clone();
                            let languages_display = if repo.languages.len() > 3 {
                                format!("{} +{}", repo.languages[..3].join(", "), repo.languages.len() - 3)
                            } else {
                                repo.languages.join(", ")
                            };

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
                                            {repo.file_count.map(|c| format_number(c as u64)).unwrap_or_else(|| "-".to_string())}
                                        </span>
                                    </TableCell>
                                    <TableCell>
                                        <span class="text-sm text-muted-foreground">
                                            {format_date(&repo.updated_at)}
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
