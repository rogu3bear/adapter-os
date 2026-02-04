//! Update field type for PATCH-style operations.
//!
//! This module provides [`UpdateField<T>`], a more explicit alternative to
//! `Option<Option<T>>` for tri-state update semantics in PATCH requests.
//!
//! # JSON Semantics
//!
//! When used with serde:
//! - **Absent field** → `UpdateField::Unchanged` (field not in JSON)
//! - **`null` value** → `UpdateField::Clear` (field is `null`)
//! - **Present value** → `UpdateField::Set(value)` (field has a value)
//!
//! # Example
//!
//! ```rust
//! use adapteros_api_types::UpdateField;
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Serialize, Deserialize)]
//! struct UpdateUserRequest {
//!     #[serde(default, skip_serializing_if = "UpdateField::is_unchanged")]
//!     name: UpdateField<String>,
//!     #[serde(default, skip_serializing_if = "UpdateField::is_unchanged")]
//!     nickname: UpdateField<String>,
//! }
//!
//! // JSON: {} → name: Unchanged, nickname: Unchanged
//! // JSON: {"name": "Alice"} → name: Set("Alice"), nickname: Unchanged
//! // JSON: {"nickname": null} → name: Unchanged, nickname: Clear
//! // JSON: {"name": "Bob", "nickname": null} → name: Set("Bob"), nickname: Clear
//! ```
//!
//! # Migration from `Option<Option<T>>`
//!
//! Use the conversion methods to migrate incrementally:
//!
//! ```rust
//! use adapteros_api_types::UpdateField;
//!
//! // Old pattern:
//! // Some(Some(value)) = set, Some(None) = clear, None = unchanged
//! let old: Option<Option<String>> = Some(Some("value".into()));
//!
//! // Convert to UpdateField
//! let new = UpdateField::from_option(old);
//! assert!(matches!(new, UpdateField::Set(v) if v == "value"));
//!
//! // Convert back if needed for legacy APIs
//! let back = new.into_option();
//! assert_eq!(back, Some(Some("value".into())));
//! ```

use serde::{de::Deserializer, ser::Serializer, Deserialize, Serialize};

/// Represents a field update in PATCH-style operations.
///
/// This is a more explicit alternative to `Option<Option<T>>` for tri-state
/// update semantics:
///
/// | Variant      | JSON                | Meaning                    |
/// |--------------|---------------------|----------------------------|
/// | `Unchanged`  | field absent        | Leave existing value as-is |
/// | `Clear`      | `"field": null`     | Set to NULL/unset          |
/// | `Set(value)` | `"field": "value"`  | Set to new value           |
///
/// # Serde Attributes
///
/// For correct JSON behavior, use these attributes on fields:
///
/// ```ignore
/// #[serde(default, skip_serializing_if = "UpdateField::is_unchanged")]
/// field: UpdateField<T>,
/// ```
///
/// - `default` ensures absent fields become `Unchanged`
/// - `skip_serializing_if` prevents serializing unchanged fields
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UpdateField<T> {
    /// Leave the field unchanged (field absent in JSON)
    Unchanged,
    /// Clear/unset the field (set to NULL)
    Clear,
    /// Set to a new value
    Set(T),
}

impl<T> UpdateField<T> {
    /// Returns `true` if the field should remain unchanged.
    ///
    /// This is useful as a `skip_serializing_if` predicate:
    /// ```ignore
    /// #[serde(default, skip_serializing_if = "UpdateField::is_unchanged")]
    /// field: UpdateField<T>,
    /// ```
    #[inline]
    pub fn is_unchanged(&self) -> bool {
        matches!(self, Self::Unchanged)
    }

    /// Returns `true` if the field should be cleared/unset.
    #[inline]
    pub fn is_clear(&self) -> bool {
        matches!(self, Self::Clear)
    }

    /// Returns `true` if the field has a new value to set.
    #[inline]
    pub fn is_set(&self) -> bool {
        matches!(self, Self::Set(_))
    }

    /// Returns the inner value if `Set`, otherwise `None`.
    #[inline]
    pub fn as_set(&self) -> Option<&T> {
        match self {
            Self::Set(v) => Some(v),
            _ => None,
        }
    }

    /// Converts into the inner value if `Set`, otherwise `None`.
    #[inline]
    pub fn into_set(self) -> Option<T> {
        match self {
            Self::Set(v) => Some(v),
            _ => None,
        }
    }

    /// Maps the inner value using the provided function.
    ///
    /// - `Unchanged` → `Unchanged`
    /// - `Clear` → `Clear`
    /// - `Set(v)` → `Set(f(v))`
    #[inline]
    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> UpdateField<U> {
        match self {
            Self::Unchanged => UpdateField::Unchanged,
            Self::Clear => UpdateField::Clear,
            Self::Set(v) => UpdateField::Set(f(v)),
        }
    }

    /// Converts to `Option<Option<T>>` for compatibility with legacy APIs.
    ///
    /// - `Unchanged` → `None`
    /// - `Clear` → `Some(None)`
    /// - `Set(v)` → `Some(Some(v))`
    #[inline]
    pub fn into_option(self) -> Option<Option<T>> {
        match self {
            Self::Unchanged => None,
            Self::Clear => Some(None),
            Self::Set(v) => Some(Some(v)),
        }
    }

    /// Creates from `Option<Option<T>>` for migration from legacy patterns.
    ///
    /// - `None` → `Unchanged`
    /// - `Some(None)` → `Clear`
    /// - `Some(Some(v))` → `Set(v)`
    #[inline]
    pub fn from_option(opt: Option<Option<T>>) -> Self {
        match opt {
            None => Self::Unchanged,
            Some(None) => Self::Clear,
            Some(Some(v)) => Self::Set(v),
        }
    }

    /// Applies this update to an existing optional value.
    ///
    /// - `Unchanged` → returns the existing value unchanged
    /// - `Clear` → returns `None`
    /// - `Set(v)` → returns `Some(v)`
    #[inline]
    pub fn apply(self, existing: Option<T>) -> Option<T> {
        match self {
            Self::Unchanged => existing,
            Self::Clear => None,
            Self::Set(v) => Some(v),
        }
    }

    /// Applies this update to an existing value, using a default if clearing.
    ///
    /// - `Unchanged` → returns the existing value unchanged
    /// - `Clear` → returns the default value
    /// - `Set(v)` → returns the new value
    #[inline]
    pub fn apply_or(self, existing: T, default: T) -> T {
        match self {
            Self::Unchanged => existing,
            Self::Clear => default,
            Self::Set(v) => v,
        }
    }
}

impl<T> Default for UpdateField<T> {
    /// Default is `Unchanged`, which is correct for serde's `#[serde(default)]`.
    #[inline]
    fn default() -> Self {
        Self::Unchanged
    }
}

impl<T> From<T> for UpdateField<T> {
    /// Converts a value into `Set(value)`.
    #[inline]
    fn from(value: T) -> Self {
        Self::Set(value)
    }
}

impl<T> From<Option<T>> for UpdateField<T> {
    /// Converts an `Option<T>`:
    /// - `Some(v)` → `Set(v)`
    /// - `None` → `Clear`
    ///
    /// Note: This is different from `from_option(Option<Option<T>>)`.
    #[inline]
    fn from(opt: Option<T>) -> Self {
        match opt {
            Some(v) => Self::Set(v),
            None => Self::Clear,
        }
    }
}

// Custom Serialize implementation
// - Unchanged: should not be serialized (use skip_serializing_if)
// - Clear: serialize as null
// - Set(v): serialize the value
impl<T: Serialize> Serialize for UpdateField<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Unchanged => {
                // If we're being serialized, skip_serializing_if wasn't used.
                // Serialize as null to be safe (semantically closest to "no change").
                serializer.serialize_none()
            }
            Self::Clear => serializer.serialize_none(),
            Self::Set(v) => v.serialize(serializer),
        }
    }
}

// Custom Deserialize implementation
// - null → Clear
// - value → Set(value)
// - absent → Unchanged (handled by serde's #[serde(default)])
impl<'de, T: Deserialize<'de>> Deserialize<'de> for UpdateField<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Option<T> deserializes null as None, value as Some(value)
        let opt: Option<T> = Option::deserialize(deserializer)?;
        Ok(match opt {
            None => Self::Clear,
            Some(v) => Self::Set(v),
        })
    }
}

// Conditionally derive ToSchema when server feature is enabled.
// OpenAPI doesn't natively support tri-state (absent/null/value) semantics,
// so we represent this as a nullable version of T with documentation.
#[cfg(feature = "server")]
impl<T: utoipa::PartialSchema + utoipa::__dev::ComposeSchema> utoipa::PartialSchema
    for UpdateField<T>
{
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        // Simply delegate to Option<T>'s schema, which is nullable T.
        // The tri-state semantics (absent = unchanged, null = clear, value = set)
        // must be documented at the API level since OpenAPI can't express this.
        <Option<T> as utoipa::PartialSchema>::schema()
    }
}

#[cfg(feature = "server")]
impl<T: utoipa::ToSchema + utoipa::__dev::ComposeSchema> utoipa::ToSchema for UpdateField<T> {
    fn name() -> std::borrow::Cow<'static, str> {
        // Use a descriptive name that hints at the update semantics
        std::borrow::Cow::Owned(format!("UpdateField_{}", T::name()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{from_str, json, to_string, to_value, Value};

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct TestStruct {
        #[serde(default, skip_serializing_if = "UpdateField::is_unchanged")]
        name: UpdateField<String>,
        #[serde(default, skip_serializing_if = "UpdateField::is_unchanged")]
        age: UpdateField<u32>,
    }

    // ============== Deserialization Tests ==============

    #[test]
    fn deserialize_absent_field_is_unchanged() {
        let json = r#"{}"#;
        let result: TestStruct = from_str(json).unwrap();
        assert!(result.name.is_unchanged());
        assert!(result.age.is_unchanged());
    }

    #[test]
    fn deserialize_null_is_clear() {
        let json = r#"{"name": null, "age": null}"#;
        let result: TestStruct = from_str(json).unwrap();
        assert!(result.name.is_clear());
        assert!(result.age.is_clear());
    }

    #[test]
    fn deserialize_value_is_set() {
        let json = r#"{"name": "Alice", "age": 30}"#;
        let result: TestStruct = from_str(json).unwrap();
        assert_eq!(result.name.as_set(), Some(&"Alice".to_string()));
        assert_eq!(result.age.as_set(), Some(&30));
    }

    #[test]
    fn deserialize_mixed() {
        let json = r#"{"name": "Bob"}"#;
        let result: TestStruct = from_str(json).unwrap();
        assert_eq!(result.name.as_set(), Some(&"Bob".to_string()));
        assert!(result.age.is_unchanged());

        let json = r#"{"age": null}"#;
        let result: TestStruct = from_str(json).unwrap();
        assert!(result.name.is_unchanged());
        assert!(result.age.is_clear());

        let json = r#"{"name": null, "age": 25}"#;
        let result: TestStruct = from_str(json).unwrap();
        assert!(result.name.is_clear());
        assert_eq!(result.age.as_set(), Some(&25));
    }

    // ============== Serialization Tests ==============

    #[test]
    fn serialize_unchanged_is_absent() {
        let test = TestStruct {
            name: UpdateField::Unchanged,
            age: UpdateField::Unchanged,
        };
        let value: Value = to_value(&test).unwrap();
        assert_eq!(value, json!({}));
    }

    #[test]
    fn serialize_clear_is_null() {
        let test = TestStruct {
            name: UpdateField::Clear,
            age: UpdateField::Clear,
        };
        let value: Value = to_value(&test).unwrap();
        assert_eq!(value, json!({"name": null, "age": null}));
    }

    #[test]
    fn serialize_set_is_value() {
        let test = TestStruct {
            name: UpdateField::Set("Carol".to_string()),
            age: UpdateField::Set(40),
        };
        let value: Value = to_value(&test).unwrap();
        assert_eq!(value, json!({"name": "Carol", "age": 40}));
    }

    #[test]
    fn serialize_mixed() {
        let test = TestStruct {
            name: UpdateField::Set("Dave".to_string()),
            age: UpdateField::Unchanged,
        };
        let value: Value = to_value(&test).unwrap();
        assert_eq!(value, json!({"name": "Dave"}));

        let test = TestStruct {
            name: UpdateField::Unchanged,
            age: UpdateField::Clear,
        };
        let value: Value = to_value(&test).unwrap();
        assert_eq!(value, json!({"age": null}));
    }

    // ============== Round-trip Tests ==============

    #[test]
    fn roundtrip_set_value() {
        let original = TestStruct {
            name: UpdateField::Set("Eve".to_string()),
            age: UpdateField::Set(50),
        };
        let json_str = to_string(&original).unwrap();
        let parsed: TestStruct = from_str(&json_str).unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn roundtrip_clear() {
        let original = TestStruct {
            name: UpdateField::Clear,
            age: UpdateField::Clear,
        };
        let json_str = to_string(&original).unwrap();
        let parsed: TestStruct = from_str(&json_str).unwrap();
        assert_eq!(original, parsed);
    }

    // Note: Unchanged cannot round-trip because it serializes as absent,
    // but that's the correct behavior for PATCH semantics.

    // ============== Conversion Tests ==============

    #[test]
    fn into_option_conversion() {
        let unchanged: UpdateField<String> = UpdateField::Unchanged;
        assert_eq!(unchanged.into_option(), None);

        let clear: UpdateField<String> = UpdateField::Clear;
        assert_eq!(clear.into_option(), Some(None));

        let set: UpdateField<String> = UpdateField::Set("test".to_string());
        assert_eq!(set.into_option(), Some(Some("test".to_string())));
    }

    #[test]
    fn from_option_conversion() {
        let none: Option<Option<String>> = None;
        assert!(UpdateField::from_option(none).is_unchanged());

        let some_none: Option<Option<String>> = Some(None);
        assert!(UpdateField::from_option(some_none).is_clear());

        let some_some: Option<Option<String>> = Some(Some("test".to_string()));
        assert_eq!(
            UpdateField::from_option(some_some).into_set(),
            Some("test".to_string())
        );
    }

    #[test]
    fn from_value() {
        let field: UpdateField<i32> = 42.into();
        assert_eq!(field.into_set(), Some(42));
    }

    #[test]
    fn from_option_single() {
        let field: UpdateField<i32> = Some(42).into();
        assert_eq!(field.into_set(), Some(42));

        let field: UpdateField<i32> = None.into();
        assert!(field.is_clear());
    }

    // ============== Method Tests ==============

    #[test]
    fn map_transformation() {
        let unchanged: UpdateField<i32> = UpdateField::Unchanged;
        let mapped = unchanged.map(|x| x.to_string());
        assert!(mapped.is_unchanged());

        let clear: UpdateField<i32> = UpdateField::Clear;
        let mapped = clear.map(|x| x.to_string());
        assert!(mapped.is_clear());

        let set: UpdateField<i32> = UpdateField::Set(42);
        let mapped = set.map(|x| x.to_string());
        assert_eq!(mapped.into_set(), Some("42".to_string()));
    }

    #[test]
    fn apply_to_existing() {
        let existing = Some("old".to_string());

        let unchanged: UpdateField<String> = UpdateField::Unchanged;
        assert_eq!(unchanged.apply(existing.clone()), existing);

        let clear: UpdateField<String> = UpdateField::Clear;
        assert_eq!(clear.apply(existing.clone()), None);

        let set: UpdateField<String> = UpdateField::Set("new".to_string());
        assert_eq!(set.apply(existing), Some("new".to_string()));
    }

    #[test]
    fn apply_to_none() {
        let existing: Option<String> = None;

        let unchanged: UpdateField<String> = UpdateField::Unchanged;
        assert_eq!(unchanged.apply(existing.clone()), None);

        let set: UpdateField<String> = UpdateField::Set("new".to_string());
        assert_eq!(set.apply(existing), Some("new".to_string()));
    }

    #[test]
    fn apply_or_with_default() {
        let existing = "old".to_string();
        let default = "default".to_string();

        let unchanged: UpdateField<String> = UpdateField::Unchanged;
        assert_eq!(unchanged.apply_or(existing.clone(), default.clone()), "old");

        let clear: UpdateField<String> = UpdateField::Clear;
        assert_eq!(clear.apply_or(existing.clone(), default.clone()), "default");

        let set: UpdateField<String> = UpdateField::Set("new".to_string());
        assert_eq!(set.apply_or(existing, default), "new");
    }

    #[test]
    fn default_is_unchanged() {
        let field: UpdateField<String> = UpdateField::default();
        assert!(field.is_unchanged());
    }

    // ============== Complex Type Tests ==============

    #[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
    struct Address {
        street: String,
        city: String,
    }

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct PersonUpdate {
        #[serde(default, skip_serializing_if = "UpdateField::is_unchanged")]
        address: UpdateField<Address>,
        #[serde(default, skip_serializing_if = "UpdateField::is_unchanged")]
        tags: UpdateField<Vec<String>>,
    }

    #[test]
    fn complex_type_deserialize() {
        let json = r#"{"address": {"street": "123 Main", "city": "Boston"}}"#;
        let result: PersonUpdate = from_str(json).unwrap();
        assert_eq!(
            result.address.as_set(),
            Some(&Address {
                street: "123 Main".to_string(),
                city: "Boston".to_string()
            })
        );
        assert!(result.tags.is_unchanged());

        let json = r#"{"address": null, "tags": ["a", "b"]}"#;
        let result: PersonUpdate = from_str(json).unwrap();
        assert!(result.address.is_clear());
        assert_eq!(
            result.tags.as_set(),
            Some(&vec!["a".to_string(), "b".to_string()])
        );
    }

    #[test]
    fn complex_type_serialize() {
        let update = PersonUpdate {
            address: UpdateField::Set(Address {
                street: "456 Oak".to_string(),
                city: "Seattle".to_string(),
            }),
            tags: UpdateField::Clear,
        };
        let value: Value = to_value(&update).unwrap();
        assert_eq!(
            value,
            json!({
                "address": {"street": "456 Oak", "city": "Seattle"},
                "tags": null
            })
        );
    }

    // ============== Edge Cases ==============

    #[test]
    fn empty_string_is_not_clear() {
        let json = r#"{"name": ""}"#;
        let result: TestStruct = from_str(json).unwrap();
        assert_eq!(result.name.as_set(), Some(&"".to_string()));
    }

    #[test]
    fn zero_is_not_clear() {
        let json = r#"{"age": 0}"#;
        let result: TestStruct = from_str(json).unwrap();
        assert_eq!(result.age.as_set(), Some(&0));
    }

    #[test]
    fn false_is_not_clear() {
        #[derive(Debug, Serialize, Deserialize)]
        struct BoolTest {
            #[serde(default, skip_serializing_if = "UpdateField::is_unchanged")]
            active: UpdateField<bool>,
        }

        let json = r#"{"active": false}"#;
        let result: BoolTest = from_str(json).unwrap();
        assert_eq!(result.active.as_set(), Some(&false));
    }

    #[test]
    fn empty_array_is_not_clear() {
        #[derive(Debug, Serialize, Deserialize)]
        struct ArrayTest {
            #[serde(default, skip_serializing_if = "UpdateField::is_unchanged")]
            items: UpdateField<Vec<i32>>,
        }

        let json = r#"{"items": []}"#;
        let result: ArrayTest = from_str(json).unwrap();
        assert_eq!(result.items.as_set(), Some(&vec![]));
    }
}
