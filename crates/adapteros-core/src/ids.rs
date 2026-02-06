//! ID system re-exports.
//!
//! All ID generation has moved to the `adapteros-id` crate.
//! This module re-exports for backward compatibility.

pub use adapteros_id::{is_readable_id, IdPrefix, TypedId};
