use super::workspace::{DOCUMENT_UPLOAD_MAX_FILE_SIZE, DOCUMENT_UPLOAD_SUPPORTED_EXTENSIONS};
use wasm_bindgen::JsCast;

pub(super) fn selected_file_from_event(ev: &web_sys::Event) -> Option<web_sys::File> {
    let target = ev
        .target()
        .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())?;
    let files = target.files()?;
    files.get(0)
}

pub(super) fn validate_attach_upload_file(file: &web_sys::File) -> Result<(), String> {
    let size = file.size() as u64;
    if size > DOCUMENT_UPLOAD_MAX_FILE_SIZE {
        return Err(format!(
            "File too large. Maximum size is {} MB.",
            DOCUMENT_UPLOAD_MAX_FILE_SIZE / 1024 / 1024
        ));
    }

    let file_name = file.name().to_lowercase();
    let supported = DOCUMENT_UPLOAD_SUPPORTED_EXTENSIONS
        .iter()
        .any(|ext| file_name.ends_with(ext));
    if !supported {
        return Err(format!(
            "Unsupported file type. Supported: {}",
            DOCUMENT_UPLOAD_SUPPORTED_EXTENSIONS.join(", ")
        ));
    }

    Ok(())
}

pub(super) fn reset_file_input_value(ev: &web_sys::Event) {
    if let Some(input) = ev
        .target()
        .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
    {
        input.set_value("");
    }
}

#[cfg(target_arch = "wasm32")]
pub(super) fn set_timeout_simple<F: FnOnce() + 'static>(f: F, ms: i32) {
    use wasm_bindgen::prelude::*;

    if let Some(window) = web_sys::window() {
        let closure = Closure::once(f);
        let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
            closure.as_ref().unchecked_ref(),
            ms,
        );
        closure.forget();
    } else {
        tracing::error!("set_timeout_simple: no window object available");
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn set_timeout_simple<F: FnOnce() + 'static>(f: F, _ms: i32) {
    f();
}
