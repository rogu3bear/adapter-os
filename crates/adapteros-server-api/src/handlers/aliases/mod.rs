use axum::{
    http::{header, HeaderValue},
    response::Response,
};

pub mod run_evidence;
pub mod workspaces;

pub(crate) fn add_alias_headers(mut response: Response, canonical: &str) -> Response {
    let headers = response.headers_mut();
    headers.insert("Deprecation", HeaderValue::from_static("true"));
    if let Ok(value) = HeaderValue::from_str(&format!("<{}>; rel=\"canonical\"", canonical)) {
        headers.insert(header::LINK, value);
    }
    response
}
