//! Search data structures for the command palette.

/// Search result types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchResultType {
    Page,
    Adapter,
    Model,
    Worker,
    Stack,
    Action,
}

impl SearchResultType {
    pub fn icon_path(&self) -> &'static str {
        match self {
            Self::Page => "M4 6h16M4 12h16M4 18h16",
            Self::Adapter => "M12 2l4 4-4 4-4-4 4-4zm0 8l4 4-4 4-4-4 4-4z",
            Self::Model => "M3 7h18M3 12h18M3 17h18",
            Self::Worker => "M12 4v16m8-8H4",
            Self::Stack => "M4 7l8-4 8 4-8 4-8-4zm0 5l8 4 8-4",
            Self::Action => "M12 8v8m4-4H8",
        }
    }
}

/// Search action type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchAction {
    Navigate(String),
    Execute(String),
}

/// Search result item.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub result_type: SearchResultType,
    pub action: SearchAction,
    pub score: f32,
    pub shortcut: Option<String>,
}

impl SearchResult {
    pub fn new(id: &str, title: &str, result_type: SearchResultType, action: SearchAction) -> Self {
        Self {
            id: id.to_string(),
            title: title.to_string(),
            subtitle: None,
            result_type,
            action,
            score: 1.0,
            shortcut: None,
        }
    }

    pub fn path(&self) -> Option<&str> {
        match &self.action {
            SearchAction::Navigate(path) => Some(path.as_str()),
            _ => None,
        }
    }

    pub fn matches(&self, query: &str) -> bool {
        let query = query.to_lowercase();
        let title = self.title.to_lowercase();
        let subtitle = self
            .subtitle
            .as_ref()
            .map(|s| s.to_lowercase())
            .unwrap_or_default();

        if title.contains(&query) || subtitle.contains(&query) {
            return true;
        }

        // Light boost for action keywords
        if let SearchAction::Execute(command) = &self.action {
            if command.to_lowercase().contains(&query) {
                return true;
            }
        }

        false
    }
}

/// Recent item types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecentItemType {
    Page,
    Adapter,
    Model,
    Worker,
    Action,
}

impl RecentItemType {
    pub fn icon_path(&self) -> &'static str {
        match self {
            Self::Page => "M4 6h16M4 12h16M4 18h16",
            Self::Adapter => "M12 2l4 4-4 4-4-4 4-4zm0 8l4 4-4 4-4-4 4-4z",
            Self::Model => "M3 7h18M3 12h18M3 17h18",
            Self::Worker => "M12 4v16m8-8H4",
            Self::Action => "M12 8v8m4-4H8",
        }
    }
}

/// Recently selected item.
#[derive(Debug, Clone)]
pub struct RecentItem {
    pub id: String,
    pub item_type: RecentItemType,
    pub label: String,
    pub subtitle: Option<String>,
    pub path: String,
}

impl RecentItem {
    pub fn new(item_type: RecentItemType, id: &str, label: &str, path: &str) -> Self {
        Self {
            id: id.to_string(),
            item_type,
            label: label.to_string(),
            subtitle: None,
            path: path.to_string(),
        }
    }
}
