use adapteros_api_types::filesystem::{EntryType, FileBrowseEntry};
use leptos::prelude::*;

#[component]
pub fn TreeView(
    entries: Vec<FileBrowseEntry>,
    selected_path: Signal<Option<String>>,
    on_open_dir: Callback<String>,
    on_open_file: Callback<String>,
) -> impl IntoView {
    view! {
        <div class="files-tree">
            <div class="files-tree-header">"Explorer"</div>
            <div class="files-tree-list">
                {entries
                    .into_iter()
                    .map(|entry| {
                        let entry_path = entry.path.clone();
                        let selected_entry_path = entry.path.clone();
                        let entry_name = entry.name.clone();
                        let is_dir = entry.entry_type == EntryType::Directory;
                        let on_open_dir = on_open_dir.clone();
                        let on_open_file = on_open_file.clone();
                        let selected_path = selected_path;
                        view! {
                            <button
                                type="button"
                                class=move || {
                                    let is_selected = selected_path
                                        .get()
                                        .as_ref()
                                        .map(|p| p == &selected_entry_path)
                                        .unwrap_or(false);
                                    if is_selected {
                                        "files-tree-item is-selected"
                                    } else {
                                        "files-tree-item"
                                    }
                                }
                                on:click=move |_| {
                                    if is_dir {
                                        on_open_dir.run(entry_path.clone());
                                    } else {
                                        on_open_file.run(entry_path.clone());
                                    }
                                }
                            >
                                <span class="files-tree-icon">
                                    {if is_dir { "📁" } else { "📄" }}
                                </span>
                                <span class="files-tree-name">{entry_name}</span>
                            </button>
                        }
                    })
                    .collect::<Vec<_>>()}
            </div>
        </div>
    }
}
