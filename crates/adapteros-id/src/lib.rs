//! Typed ID generation with word aliases for AdapterOS.
//!
//! Two-layer ID system:
//! - **Layer 1**: `{prefix}-{uuid-v7}` canonical IDs (stored, transmitted, queried)
//! - **Layer 2**: `{prefix}-{adjective}-{noun}` word aliases (display-only, derived via BLAKE3)

mod compat;
mod word_alias;
mod words;

pub use compat::{extract_uuid_from_legacy, is_legacy_id, is_readable_id};
pub use word_alias::word_alias;

/// Truncate any ID string for display.
///
/// - If the input is a valid `TypedId`, returns `TypedId::short()` (prefix + 8 hex).
/// - Otherwise, truncates to the first 12 characters with an ellipsis.
pub fn short_id(id: &str) -> String {
    if let Some(tid) = TypedId::parse(id) {
        tid.short().to_string()
    } else {
        let trimmed = id.trim();
        if trimmed.len() > 12 {
            format!("{}...", &trimmed[..12])
        } else {
            trimmed.to_string()
        }
    }
}

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Short prefix codes for each entity type.
///
/// These map 1:1 with the old `IdKind` variants in `adapteros-core`.
/// Prefix strings are 2-4 chars, lowercase, used as the first segment of a `TypedId`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IdPrefix {
    Tnt, // tenant
    Usr, // user
    Nod, // node
    Mdl, // model
    Adp, // adapter
    Pln, // plan
    Job, // job
    Wrk, // worker
    Dst, // dataset
    Doc, // document
    Chk, // chunk
    Fil, // file
    Col, // collection
    Stk, // stack
    Run, // run
    Trc, // trace
    Req, // request
    Ses, // session
    Msg, // message
    Pol, // policy
    Aud, // audit
    Inc, // incident
    Dec, // decision
    Err, // error
    Upl, // upload
    Rpt, // report
    Exp, // export
    Rep, // repository
    Wsp, // workspace
    Ver, // version
    Evt, // event
    Rpl, // replay
    Rvw, // review
    Bat, // batch
    Rot, // rotation
    Tok, // token
    Wak, // write-ack
}

impl IdPrefix {
    /// The short string form used in serialized IDs.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tnt => "tnt",
            Self::Usr => "usr",
            Self::Nod => "nod",
            Self::Mdl => "mdl",
            Self::Adp => "adp",
            Self::Pln => "pln",
            Self::Job => "job",
            Self::Wrk => "wrk",
            Self::Dst => "dst",
            Self::Doc => "doc",
            Self::Chk => "chk",
            Self::Fil => "fil",
            Self::Col => "col",
            Self::Stk => "stk",
            Self::Run => "run",
            Self::Trc => "trc",
            Self::Req => "req",
            Self::Ses => "ses",
            Self::Msg => "msg",
            Self::Pol => "pol",
            Self::Aud => "aud",
            Self::Inc => "inc",
            Self::Dec => "dec",
            Self::Err => "err",
            Self::Upl => "upl",
            Self::Rpt => "rpt",
            Self::Exp => "exp",
            Self::Rep => "rep",
            Self::Wsp => "wsp",
            Self::Ver => "ver",
            Self::Evt => "evt",
            Self::Rpl => "rpl",
            Self::Rvw => "rvw",
            Self::Bat => "bat",
            Self::Rot => "rot",
            Self::Tok => "tok",
            Self::Wak => "wak",
        }
    }

    /// Parse a prefix string back to the enum variant.
    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "tnt" => Self::Tnt,
            "usr" => Self::Usr,
            "nod" => Self::Nod,
            "mdl" => Self::Mdl,
            "adp" => Self::Adp,
            "pln" => Self::Pln,
            "job" => Self::Job,
            "wrk" => Self::Wrk,
            "dst" => Self::Dst,
            "doc" => Self::Doc,
            "chk" => Self::Chk,
            "fil" => Self::Fil,
            "col" => Self::Col,
            "stk" => Self::Stk,
            "run" => Self::Run,
            "trc" => Self::Trc,
            "req" => Self::Req,
            "ses" => Self::Ses,
            "msg" => Self::Msg,
            "pol" => Self::Pol,
            "aud" => Self::Aud,
            "inc" => Self::Inc,
            "dec" => Self::Dec,
            "err" => Self::Err,
            "upl" => Self::Upl,
            "rpt" => Self::Rpt,
            "exp" => Self::Exp,
            "rep" => Self::Rep,
            "wsp" => Self::Wsp,
            "ver" => Self::Ver,
            "evt" => Self::Evt,
            "rpl" => Self::Rpl,
            "rvw" => Self::Rvw,
            "bat" => Self::Bat,
            "rot" => Self::Rot,
            "tok" => Self::Tok,
            "wak" => Self::Wak,
            _ => return None,
        })
    }

    /// Whether this prefix type gets a user-facing word alias.
    pub fn has_word_alias(self) -> bool {
        matches!(
            self,
            Self::Wrk
                | Self::Adp
                | Self::Mdl
                | Self::Job
                | Self::Dst
                | Self::Col
                | Self::Rep
                | Self::Bat
                | Self::Stk
        )
    }
}

impl fmt::Display for IdPrefix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A typed ID: `{prefix}-{uuid-v7}`.
///
/// The canonical form stored in databases, transmitted in APIs, and used in queries.
/// Use [`TypedId::word_alias`] to get the human-friendly display form.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TypedId(String);

impl TypedId {
    /// Generate a new ID with a UUIDv7 (time-ordered).
    pub fn new(prefix: IdPrefix) -> Self {
        let uuid = Uuid::now_v7();
        Self(format!("{}-{}", prefix.as_str(), uuid.as_simple()))
    }

    /// Construct from an existing prefix and UUID.
    pub fn from_parts(prefix: IdPrefix, uuid: Uuid) -> Self {
        Self(format!("{}-{}", prefix.as_str(), uuid.as_simple()))
    }

    /// Parse a `{prefix}-{hex32}` string into a `TypedId`.
    ///
    /// Returns `None` if the prefix is unknown or the UUID portion is invalid.
    pub fn parse(s: &str) -> Option<Self> {
        let dash = s.find('-')?;
        let prefix_str = &s[..dash];
        let uuid_hex = &s[dash + 1..];

        // Validate prefix
        let _prefix = IdPrefix::from_str(prefix_str)?;

        // Validate UUID (32 hex chars, simple format)
        if uuid_hex.len() != 32 || !uuid_hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }

        // Validate it actually parses as UUID
        Uuid::parse_str(uuid_hex).ok()?;

        Some(Self(s.to_string()))
    }

    /// The prefix portion of this ID.
    pub fn prefix(&self) -> IdPrefix {
        let dash = self.0.find('-').expect("TypedId always contains dash");
        IdPrefix::from_str(&self.0[..dash]).expect("TypedId always has valid prefix")
    }

    /// The UUID portion of this ID.
    pub fn uuid(&self) -> Uuid {
        let dash = self.0.find('-').expect("TypedId always contains dash");
        Uuid::parse_str(&self.0[dash + 1..]).expect("TypedId always has valid UUID")
    }

    /// Short form for logs: `{prefix}-{first 8 hex chars}`.
    pub fn short(&self) -> &str {
        // prefix (2-3 chars) + dash (1) + 8 hex = 12 chars max
        let dash = self.0.find('-').expect("TypedId always contains dash");
        let end = (dash + 1 + 8).min(self.0.len());
        &self.0[..end]
    }

    /// The full canonical string form.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Compute the word alias for this ID (if applicable).
    ///
    /// Returns `Some` only for entity types that have word aliases
    /// (workers, adapters, models, jobs, datasets, collections, repositories, batches, stacks).
    pub fn word_alias(&self) -> Option<String> {
        let prefix = self.prefix();
        if prefix.has_word_alias() {
            Some(word_alias::word_alias(prefix, &self.uuid()))
        } else {
            None
        }
    }
}

impl fmt::Display for TypedId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for TypedId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TypedId({})", self.0)
    }
}

impl AsRef<str> for TypedId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<TypedId> for String {
    fn from(id: TypedId) -> String {
        id.0
    }
}

impl Serialize for TypedId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for TypedId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        TypedId::parse(&s).ok_or_else(|| serde::de::Error::custom(format!("invalid TypedId: {s}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_generates_valid_id() {
        let id = TypedId::new(IdPrefix::Wrk);
        assert!(id.as_str().starts_with("wrk-"));
        // prefix (3) + dash (1) + 32 hex = 36
        assert_eq!(id.as_str().len(), 36);
    }

    #[test]
    fn parse_roundtrip() {
        let id = TypedId::new(IdPrefix::Adp);
        let parsed = TypedId::parse(id.as_str()).expect("should parse");
        assert_eq!(id, parsed);
    }

    #[test]
    fn parse_rejects_invalid() {
        assert!(TypedId::parse("").is_none());
        assert!(TypedId::parse("wrk").is_none());
        assert!(TypedId::parse("wrk-").is_none());
        assert!(TypedId::parse("wrk-tooshort").is_none());
        assert!(TypedId::parse("xxx-01234567890123456789012345678901").is_none());
        assert!(TypedId::parse("wrk-0123456789012345678901234567890g").is_none());
    }

    #[test]
    fn prefix_extraction() {
        let id = TypedId::new(IdPrefix::Mdl);
        assert_eq!(id.prefix(), IdPrefix::Mdl);
    }

    #[test]
    fn uuid_extraction() {
        let uuid = Uuid::now_v7();
        let id = TypedId::from_parts(IdPrefix::Job, uuid);
        assert_eq!(id.uuid(), uuid);
    }

    #[test]
    fn short_form() {
        let id = TypedId::new(IdPrefix::Wrk);
        let short = id.short();
        assert!(short.starts_with("wrk-"));
        // 3 + 1 + 8 = 12
        assert_eq!(short.len(), 12);
    }

    #[test]
    fn display_and_debug() {
        let id = TypedId::new(IdPrefix::Req);
        let display = format!("{id}");
        let debug = format!("{id:?}");
        assert!(display.starts_with("req-"));
        assert!(debug.starts_with("TypedId(req-"));
    }

    #[test]
    fn serde_roundtrip() {
        let id = TypedId::new(IdPrefix::Trc);
        let json = serde_json::to_string(&id).unwrap();
        let parsed: TypedId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn serde_transparent() {
        let id = TypedId::new(IdPrefix::Ses);
        let json = serde_json::to_string(&id).unwrap();
        // Should be a plain string, not an object
        assert!(json.starts_with('"'));
        assert!(json.ends_with('"'));
    }

    #[test]
    fn word_alias_for_applicable_types() {
        let id = TypedId::new(IdPrefix::Wrk);
        assert!(id.word_alias().is_some());
        let alias = id.word_alias().unwrap();
        assert!(alias.starts_with("wrk-"));
        // wrk-{adj}-{noun}
        assert_eq!(alias.matches('-').count(), 2);
    }

    #[test]
    fn no_word_alias_for_internal_types() {
        let id = TypedId::new(IdPrefix::Trc);
        assert!(id.word_alias().is_none());

        let id = TypedId::new(IdPrefix::Req);
        assert!(id.word_alias().is_none());

        let id = TypedId::new(IdPrefix::Msg);
        assert!(id.word_alias().is_none());
    }

    #[test]
    fn word_alias_deterministic() {
        let uuid = Uuid::now_v7();
        let id1 = TypedId::from_parts(IdPrefix::Wrk, uuid);
        let id2 = TypedId::from_parts(IdPrefix::Wrk, uuid);
        assert_eq!(id1.word_alias(), id2.word_alias());
    }

    #[test]
    fn all_prefixes_roundtrip() {
        let prefixes = [
            IdPrefix::Tnt,
            IdPrefix::Usr,
            IdPrefix::Nod,
            IdPrefix::Mdl,
            IdPrefix::Adp,
            IdPrefix::Pln,
            IdPrefix::Job,
            IdPrefix::Wrk,
            IdPrefix::Dst,
            IdPrefix::Doc,
            IdPrefix::Chk,
            IdPrefix::Fil,
            IdPrefix::Col,
            IdPrefix::Stk,
            IdPrefix::Run,
            IdPrefix::Trc,
            IdPrefix::Req,
            IdPrefix::Ses,
            IdPrefix::Msg,
            IdPrefix::Pol,
            IdPrefix::Aud,
            IdPrefix::Inc,
            IdPrefix::Dec,
            IdPrefix::Err,
            IdPrefix::Upl,
            IdPrefix::Rpt,
            IdPrefix::Exp,
            IdPrefix::Rep,
            IdPrefix::Wsp,
            IdPrefix::Ver,
            IdPrefix::Evt,
            IdPrefix::Rpl,
            IdPrefix::Rvw,
            IdPrefix::Bat,
            IdPrefix::Rot,
            IdPrefix::Tok,
            IdPrefix::Wak,
        ];
        for p in prefixes {
            assert_eq!(IdPrefix::from_str(p.as_str()), Some(p));
            let id = TypedId::new(p);
            assert!(TypedId::parse(id.as_str()).is_some());
        }
    }

    #[test]
    fn from_parts_and_into_string() {
        let uuid = Uuid::now_v7();
        let id = TypedId::from_parts(IdPrefix::Adp, uuid);
        let s: String = id.into();
        assert!(s.starts_with("adp-"));
    }
}
