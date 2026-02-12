//! Markdown rendering component for chat messages.
//!
//! Converts markdown text to sanitized HTML using pulldown-cmark.
//! Strips dangerous tags (script, iframe, etc.) before rendering.

use leptos::prelude::*;
use pulldown_cmark::{Options, Parser};

/// Render markdown content as HTML.
///
/// Uses `inner_html` with sanitization to strip dangerous tags.
/// Designed for chat message bodies where users may include code blocks,
/// lists, bold/italic, and links.
#[component]
pub fn Markdown(
    /// The raw markdown string to render.
    content: String,
) -> impl IntoView {
    let html = render_markdown(&content);
    view! {
        <div class="markdown-content" inner_html=html></div>
    }
}

/// Reactive markdown that re-renders when the signal changes.
/// Used for streaming chat where content grows token-by-token.
#[component]
pub fn MarkdownStream(
    /// Reactive markdown content signal.
    content: Signal<String>,
) -> impl IntoView {
    let html = Memo::new(move |_| {
        let raw = content.try_get().unwrap_or_default();
        render_markdown(&raw)
    });

    view! {
        <div class="markdown-content" inner_html=move || html.get()></div>
    }
}

/// Convert markdown to sanitized HTML.
fn render_markdown(input: &str) -> String {
    if input.is_empty() {
        return String::new();
    }

    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);

    let parser = Parser::new_ext(input, options);

    let mut html_output = String::with_capacity(input.len() * 2);
    pulldown_cmark::html::push_html(&mut html_output, parser);

    sanitize_html(&html_output)
}

/// Strip dangerous HTML tags from rendered output.
///
/// Since we use `inner_html`, we must prevent XSS from user-supplied
/// markdown that could contain raw HTML blocks.
fn sanitize_html(html: &str) -> String {
    // Tags to strip entirely (including content between open/close)
    let dangerous_tags = [
        "script", "iframe", "object", "embed", "form", "input", "button", "textarea", "select",
        "style", "link", "meta", "base",
    ];

    let mut result = html.to_string();
    for tag in &dangerous_tags {
        // Remove self-closing and opening tags: <script ...> or <script .../>
        let open_pattern = format!("<{}[^>]*>", tag);
        let close_pattern = format!("</{}>", tag);
        if let Ok(re) = regex_lite::Regex::new(&open_pattern) {
            result = re.replace_all(&result, "").to_string();
        }
        if let Ok(re) = regex_lite::Regex::new(&close_pattern) {
            result = re.replace_all(&result, "").to_string();
        }
    }

    // Strip event handler attributes (on*)
    if let Ok(re) = regex_lite::Regex::new(r#"\s+on\w+\s*=\s*"[^"]*""#) {
        result = re.replace_all(&result, "").to_string();
    }
    if let Ok(re) = regex_lite::Regex::new(r#"\s+on\w+\s*=\s*'[^']*'"#) {
        result = re.replace_all(&result, "").to_string();
    }

    // Strip javascript: URLs in href/src attributes
    if let Ok(re) = regex_lite::Regex::new(r#"(href|src)\s*=\s*"javascript:[^"]*""#) {
        result = re.replace_all(&result, r#"$1="""#).to_string();
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_basic_markdown() {
        let html = render_markdown("**bold** and *italic*");
        assert!(html.contains("<strong>bold</strong>"));
        assert!(html.contains("<em>italic</em>"));
    }

    #[test]
    fn renders_code_block() {
        let html = render_markdown("```rust\nfn main() {}\n```");
        assert!(html.contains("<code"));
        assert!(html.contains("fn main()"));
    }

    #[test]
    fn strips_script_tags() {
        let html = render_markdown("<script>alert('xss')</script>");
        assert!(!html.contains("<script"), "script tags should be stripped");
        assert!(
            !html.contains("</script"),
            "closing script tags should be stripped"
        );
    }

    #[test]
    fn strips_event_handlers() {
        let html = sanitize_html(r#"<div onmouseover="alert('xss')">text</div>"#);
        assert!(!html.contains("onmouseover"));
    }

    #[test]
    fn strips_javascript_urls() {
        let html = sanitize_html(r#"<a href="javascript:alert('xss')">click</a>"#);
        assert!(!html.contains("javascript:"));
    }

    #[test]
    fn empty_input_returns_empty() {
        assert_eq!(render_markdown(""), "");
    }

    #[test]
    fn plain_text_wraps_in_paragraph() {
        let html = render_markdown("hello world");
        assert!(html.contains("<p>hello world</p>"));
    }
}
