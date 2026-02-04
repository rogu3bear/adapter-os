//! Human-readable ID generation utilities.

use rand::RngCore;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IdKind {
    Tenant,
    User,
    Node,
    Model,
    Adapter,
    Plan,
    Job,
    Worker,
    Dataset,
    Document,
    Chunk,
    File,
    Collection,
    Stack,
    Run,
    Trace,
    Request,
    Session,
    Message,
    Policy,
    Audit,
    Incident,
    Decision,
    Error,
    Upload,
    Report,
    Export,
    Repository,
    Workspace,
    Version,
    Event,
    Replay,
}

impl IdKind {
    pub fn prefix(self) -> &'static str {
        match self {
            IdKind::Tenant => "tenant",
            IdKind::User => "user",
            IdKind::Node => "node",
            IdKind::Model => "model",
            IdKind::Adapter => "adapter",
            IdKind::Plan => "plan",
            IdKind::Job => "job",
            IdKind::Worker => "worker",
            IdKind::Dataset => "dataset",
            IdKind::Document => "doc",
            IdKind::Chunk => "chunk",
            IdKind::File => "file",
            IdKind::Collection => "coll",
            IdKind::Stack => "stack",
            IdKind::Run => "run",
            IdKind::Trace => "trace",
            IdKind::Request => "req",
            IdKind::Session => "session",
            IdKind::Message => "msg",
            IdKind::Policy => "policy",
            IdKind::Audit => "audit",
            IdKind::Incident => "incident",
            IdKind::Decision => "decision",
            IdKind::Error => "error",
            IdKind::Upload => "upload",
            IdKind::Report => "report",
            IdKind::Export => "export",
            IdKind::Repository => "repo",
            IdKind::Workspace => "ws",
            IdKind::Version => "ver",
            IdKind::Event => "event",
            IdKind::Replay => "replay",
        }
    }

    pub fn from_prefix(prefix: &str) -> Option<Self> {
        Some(match prefix {
            "tenant" => IdKind::Tenant,
            "user" => IdKind::User,
            "node" => IdKind::Node,
            "model" => IdKind::Model,
            "adapter" => IdKind::Adapter,
            "plan" => IdKind::Plan,
            "job" => IdKind::Job,
            "worker" => IdKind::Worker,
            "dataset" => IdKind::Dataset,
            "doc" => IdKind::Document,
            "chunk" => IdKind::Chunk,
            "file" => IdKind::File,
            "coll" => IdKind::Collection,
            "stack" => IdKind::Stack,
            "run" => IdKind::Run,
            "trace" => IdKind::Trace,
            "req" => IdKind::Request,
            "session" => IdKind::Session,
            "msg" => IdKind::Message,
            "policy" => IdKind::Policy,
            "audit" => IdKind::Audit,
            "incident" => IdKind::Incident,
            "decision" => IdKind::Decision,
            "error" => IdKind::Error,
            "upload" => IdKind::Upload,
            "report" => IdKind::Report,
            "export" => IdKind::Export,
            "repo" => IdKind::Repository,
            "ws" => IdKind::Workspace,
            "ver" => IdKind::Version,
            "event" => IdKind::Event,
            "replay" => IdKind::Replay,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedId {
    pub kind: IdKind,
    pub slug: String,
    pub suffix: String,
}

pub fn slugify(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut prev_dash = false;
    for ch in input.chars() {
        let lower = ch.to_ascii_lowercase();
        if lower.is_ascii_alphanumeric() {
            out.push(lower);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "item".to_string()
    } else {
        trimmed
    }
}

pub fn generate_suffix(len: usize) -> String {
    const ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";
    let mut rng = rand::thread_rng();
    let mut out = String::with_capacity(len);
    for _ in 0..len {
        let idx = (rng.next_u32() % 32) as usize;
        out.push(ALPHABET[idx] as char);
    }
    out
}

pub fn generate_id(kind: IdKind, slug_source: &str) -> String {
    generate_id_with_suffix_len(kind, slug_source, 6)
}

pub fn generate_id_with_suffix_len(kind: IdKind, slug_source: &str, suffix_len: usize) -> String {
    let slug = slugify(slug_source);
    let suffix = generate_suffix(suffix_len);
    format!("{}.{}.{}", kind.prefix(), slug, suffix)
}

pub fn parse_id(input: &str) -> Option<ParsedId> {
    let mut parts = input.split('.');
    let prefix = parts.next()?;
    let slug = parts.next()?;
    let suffix = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    let kind = IdKind::from_prefix(prefix)?;
    if slug.is_empty() || suffix.is_empty() {
        return None;
    }
    Some(ParsedId {
        kind,
        slug: slug.to_string(),
        suffix: suffix.to_string(),
    })
}

pub fn is_readable_id(input: &str) -> bool {
    parse_id(input).is_some()
}
