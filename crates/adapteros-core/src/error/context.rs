use std::backtrace::{Backtrace, BacktraceStatus};
use std::fmt;
use std::panic::Location;
use std::path::Path;

use super::{AosError, Result};

#[derive(Debug, Clone)]
pub struct ContextFrame {
    message: String,
    file: &'static str,
    line: u32,
    function: String,
}

impl ContextFrame {
    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn file(&self) -> &'static str {
        self.file
    }

    pub fn line(&self) -> u32 {
        self.line
    }

    pub fn function(&self) -> &str {
        &self.function
    }

    fn new(message: String, location: &'static Location<'static>, function: String) -> Self {
        Self {
            message,
            file: location.file(),
            line: location.line(),
            function,
        }
    }
}

impl fmt::Display for ContextFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} ({} at {}:{})",
            self.message, self.function, self.file, self.line
        )
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ContextFrames<'a> {
    pub(crate) next: Option<&'a AosError>,
}

impl<'a> Iterator for ContextFrames<'a> {
    type Item = &'a ContextFrame;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.next?;
        match current {
            AosError::Contextual {
                context, source, ..
            } => {
                self.next = Some(source.as_ref());
                Some(context)
            }
            _ => {
                self.next = None;
                None
            }
        }
    }
}

#[derive(Debug)]
pub(crate) struct CapturedContext {
    pub frame: ContextFrame,
    pub backtrace: Option<Backtrace>,
}

#[track_caller]
pub(crate) fn capture_context(message: String) -> CapturedContext {
    let location = Location::caller();
    let backtrace = Backtrace::capture();
    let (function, backtrace) = match backtrace.status() {
        BacktraceStatus::Captured => {
            let function = infer_function(&backtrace).unwrap_or_else(|| "<unknown>".to_string());
            (function, Some(backtrace))
        }
        _ => ("<unknown>".to_string(), None),
    };

    CapturedContext {
        frame: ContextFrame::new(message, location, function),
        backtrace,
    }
}

pub trait AosContext<T> {
    #[track_caller]
    fn context(self, context: impl Into<String>) -> Result<T>;

    #[track_caller]
    fn with_context<F>(self, f: F) -> Result<T>
    where
        F: FnOnce() -> String;
}

impl<T, E> AosContext<T> for std::result::Result<T, E>
where
    E: Into<AosError>,
{
    #[track_caller]
    fn context(self, context: impl Into<String>) -> Result<T> {
        match self {
            Ok(value) => Ok(value),
            Err(err) => {
                let err: AosError = err.into();
                Err(err.context(context.into()))
            }
        }
    }

    #[track_caller]
    fn with_context<F>(self, f: F) -> Result<T>
    where
        F: FnOnce() -> String,
    {
        match self {
            Ok(value) => Ok(value),
            Err(err) => {
                let err: AosError = err.into();
                Err(err.with_context(f))
            }
        }
    }
}

fn infer_function(backtrace: &Backtrace) -> Option<String> {
    let this_file = Path::new(file!());
    let this_file_str = this_file.to_string_lossy();
    for line in backtrace.to_string().lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.contains(this_file_str.as_ref()) {
            continue;
        }
        return Some(trimmed.to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_chain_order_preserved() {
        let err = Err::<(), _>(AosError::Internal("boom".into()))
            .context("outer layer")
            .with_context(|| "id=42".to_string())
            .unwrap_err();

        let mut contexts = err.contexts();
        let first = contexts.next().expect("missing newest context");
        assert_eq!(first.message(), "id=42");

        let second = contexts.next().expect("missing parent context");
        assert_eq!(second.message(), "outer layer");

        assert!(contexts.next().is_none());
    }

    #[test]
    fn context_captures_location_metadata() {
        fn trigger() -> AosError {
            Err::<(), _>(AosError::Internal("boom".into()))
                .context("location failure")
                .unwrap_err()
        }

        let err = trigger();
        let frame = err.contexts().next().expect("missing context frame");
        assert_eq!(frame.message(), "location failure");
        assert!(!frame.file().is_empty());
        assert!(frame.line() > 0);
        assert!(!frame.function().is_empty());
    }

    #[test]
    fn with_context_closure_is_lazy() {
        let mut invoked = false;
        let result: Result<()> = Ok(());
        let result = result.with_context(|| {
            invoked = true;
            "should not run".to_string()
        });

        assert!(result.is_ok());
        assert!(!invoked);
    }
}
