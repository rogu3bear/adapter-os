/// Marker in the HTML shell where server-rendered app markup is injected.
pub const SSR_OUTLET_MARKER: &str = "<!--AOS_SSR_OUTLET-->";

#[derive(Debug)]
pub enum SsrRenderError {
    MissingOutlet,
}

impl std::fmt::Display for SsrRenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SsrRenderError::MissingOutlet => {
                write!(f, "SSR outlet marker not found in HTML shell")
            }
        }
    }
}

impl std::error::Error for SsrRenderError {}

/// Renders the Leptos app for the requested path and injects it into the
/// existing HTML shell.
pub fn render_document(
    shell_html: &str,
    request_path_and_query: &str,
) -> Result<String, SsrRenderError> {
    let route = normalize_request_path(request_path_and_query);
    let app_html = adapteros_ui::render_app_html(&route);
    inject_into_shell(shell_html, &app_html)
}

fn normalize_request_path(path_and_query: &str) -> String {
    let mut normalized = if path_and_query.is_empty() {
        "/".to_string()
    } else {
        path_and_query.to_string()
    };

    if normalized == "/index.html" {
        normalized = "/".to_string();
    } else if normalized.starts_with("/index.html?") {
        normalized = normalized.replacen("/index.html", "/", 1);
    }

    normalized
}

fn inject_into_shell(shell_html: &str, app_html: &str) -> Result<String, SsrRenderError> {
    if shell_html.contains(SSR_OUTLET_MARKER) {
        return Ok(shell_html.replacen(SSR_OUTLET_MARKER, app_html, 1));
    }
    if shell_html.contains(r#"<div id="aos-app-root"></div>"#) {
        return Ok(shell_html.replacen(r#"<div id="aos-app-root"></div>"#, app_html, 1));
    }
    if shell_html.contains(r#"<div id="root"></div>"#) {
        return Ok(shell_html.replacen(r#"<div id="root"></div>"#, app_html, 1));
    }
    Err(SsrRenderError::MissingOutlet)
}

#[cfg(test)]
mod tests {
    use super::{inject_into_shell, normalize_request_path, SsrRenderError, SSR_OUTLET_MARKER};

    #[test]
    fn normalizes_index_html_route_to_root() {
        assert_eq!(normalize_request_path("/index.html"), "/");
        assert_eq!(
            normalize_request_path("/index.html?foo=bar"),
            "/?foo=bar".to_string()
        );
    }

    #[test]
    fn injects_into_marker() -> Result<(), Box<dyn std::error::Error>> {
        let shell = format!("<html><body>{}</body></html>", SSR_OUTLET_MARKER);
        let out = inject_into_shell(&shell, "<main>ok</main>")?;
        assert!(out.contains("<main>ok</main>"));
        Ok(())
    }

    #[test]
    fn injects_into_legacy_outlet_markers() -> Result<(), Box<dyn std::error::Error>> {
        let aos_shell = r#"<html><body><div id="aos-app-root"></div></body></html>"#;
        let root_shell = r#"<html><body><div id="root"></div></body></html>"#;

        let aos_out = inject_into_shell(aos_shell, "<main>aos</main>")?;
        let root_out = inject_into_shell(root_shell, "<main>root</main>")?;

        assert!(aos_out.contains("<main>aos</main>"));
        assert!(root_out.contains("<main>root</main>"));
        Ok(())
    }

    #[test]
    fn missing_outlet_returns_contract_error() {
        let shell = "<html><body><main>no outlet</main></body></html>";
        let err = inject_into_shell(shell, "<main>app</main>").unwrap_err();
        assert!(matches!(err, SsrRenderError::MissingOutlet));
    }
}
