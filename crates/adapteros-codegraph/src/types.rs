//! Symbol type definitions and analysis
//!
//! Defines the core data structures for representing symbols,
//! their types, and relationships in the code graph.

use adapteros_core::B3Hash;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Unique identifier for a symbol
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SymbolId(B3Hash);

impl SymbolId {
    /// Create a new SymbolId from components
    pub fn new(file_id: &str, span: &str, name: &str) -> Self {
        let combined = format!("{}||{}||{}", file_id, span, name);
        Self(B3Hash::hash(combined.as_bytes()))
    }

    /// Get the underlying hash
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

    /// Convert to hex string
    pub fn to_hex(&self) -> String {
        self.0.to_hex()
    }

    /// Create from hex string
    pub fn from_hex(hex: &str) -> Result<Self, String> {
        let hash = B3Hash::from_hex(hex).map_err(|e| e.to_string())?;
        Ok(Self(hash))
    }
}

impl fmt::Display for SymbolId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

/// Kind of symbol
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SymbolKind {
    Function,
    Struct,
    Enum,
    Trait,
    Impl,
    Type,
    Const,
    Static,
    Macro,
    Module,
    Field,
    Variant,
    Method,
    AssociatedType,
    AssociatedConst,
}

impl fmt::Display for SymbolKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SymbolKind::Function => write!(f, "function"),
            SymbolKind::Struct => write!(f, "struct"),
            SymbolKind::Enum => write!(f, "enum"),
            SymbolKind::Trait => write!(f, "trait"),
            SymbolKind::Impl => write!(f, "impl"),
            SymbolKind::Type => write!(f, "type"),
            SymbolKind::Const => write!(f, "const"),
            SymbolKind::Static => write!(f, "static"),
            SymbolKind::Macro => write!(f, "macro"),
            SymbolKind::Module => write!(f, "module"),
            SymbolKind::Field => write!(f, "field"),
            SymbolKind::Variant => write!(f, "variant"),
            SymbolKind::Method => write!(f, "method"),
            SymbolKind::AssociatedType => write!(f, "associated_type"),
            SymbolKind::AssociatedConst => write!(f, "associated_const"),
        }
    }
}

/// Type annotation for a symbol
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeAnnotation {
    /// Declared type (from source code)
    pub declared_type: Option<String>,
    /// Inferred type (from analysis)
    pub inferred_type: Option<String>,
    /// Generic type parameters
    pub generic_params: Vec<String>,
    /// Lifetime parameters (ignored for now)
    pub lifetime_params: Vec<String>,
    /// Return type for functions
    pub return_type: Option<String>,
    /// Parameter types for functions
    pub parameter_types: Vec<String>,
}

impl TypeAnnotation {
    /// Create a new type annotation
    pub fn new() -> Self {
        Self {
            declared_type: None,
            inferred_type: None,
            generic_params: Vec::new(),
            lifetime_params: Vec::new(),
            return_type: None,
            parameter_types: Vec::new(),
        }
    }

    /// Add a generic parameter
    pub fn add_generic_param(&mut self, param: String) {
        self.generic_params.push(param);
    }

    /// Add a parameter type
    pub fn add_parameter_type(&mut self, param_type: String) {
        self.parameter_types.push(param_type);
    }

    /// Get the primary type (declared or inferred)
    pub fn primary_type(&self) -> Option<&String> {
        self.declared_type.as_ref().or(self.inferred_type.as_ref())
    }

    /// Convert to string representation
    pub fn to_string(&self) -> String {
        let mut parts = Vec::new();
        
        if let Some(ref declared) = self.declared_type {
            parts.push(format!("declared: {}", declared));
        }
        
        if let Some(ref inferred) = self.inferred_type {
            parts.push(format!("inferred: {}", inferred));
        }
        
        if !self.generic_params.is_empty() {
            parts.push(format!("generics: [{}]", self.generic_params.join(", ")));
        }
        
        if let Some(ref return_type) = self.return_type {
            parts.push(format!("returns: {}", return_type));
        }
        
        if !self.parameter_types.is_empty() {
            parts.push(format!("params: [{}]", self.parameter_types.join(", ")));
        }
        
        parts.join("; ")
    }
}

impl Default for TypeAnnotation {
    fn default() -> Self {
        Self::new()
    }
}

/// Visibility of a symbol
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Visibility {
    Public,
    Private,
    Crate,
    Super,
    InPath(String),
}

impl fmt::Display for Visibility {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Visibility::Public => write!(f, "pub"),
            Visibility::Private => write!(f, "private"),
            Visibility::Crate => write!(f, "pub(crate)"),
            Visibility::Super => write!(f, "pub(super)"),
            Visibility::InPath(path) => write!(f, "pub(in {})", path),
        }
    }
}

/// Source code span information
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    /// Start line (1-indexed)
    pub start_line: u32,
    /// Start column (1-indexed)
    pub start_column: u32,
    /// End line (1-indexed)
    pub end_line: u32,
    /// End column (1-indexed)
    pub end_column: u32,
    /// Byte offset in file
    pub byte_start: usize,
    /// Byte length
    pub byte_length: usize,
}

impl Span {
    /// Create a new span
    pub fn new(
        start_line: u32,
        start_column: u32,
        end_line: u32,
        end_column: u32,
        byte_start: usize,
        byte_length: usize,
    ) -> Self {
        Self {
            start_line,
            start_column,
            end_line,
            end_column,
            byte_start,
            byte_length,
        }
    }

    /// Convert to string representation
    pub fn to_string(&self) -> String {
        format!(
            "{}:{}:{}:{}",
            self.start_line, self.start_column, self.end_line, self.end_column
        )
    }
}

/// A symbol node in the code graph
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolNode {
    /// Unique identifier
    pub id: SymbolId,
    /// Symbol name
    pub name: String,
    /// Kind of symbol
    pub kind: SymbolKind,
    /// Type annotation
    pub type_annotation: Option<TypeAnnotation>,
    /// Function signature (for functions/methods)
    pub signature: Option<String>,
    /// Documentation comment
    pub docstring: Option<String>,
    /// Source code span
    pub span: Span,
    /// Visibility
    pub visibility: Visibility,
    /// File path
    pub file_path: String,
    /// Module path
    pub module_path: Vec<String>,
    /// Whether this is a recursive function
    pub is_recursive: bool,
    /// Whether this is async
    pub is_async: bool,
    /// Whether this is unsafe
    pub is_unsafe: bool,
}

impl SymbolNode {
    /// Create a new symbol node
    pub fn new(
        id: SymbolId,
        name: String,
        kind: SymbolKind,
        span: Span,
        file_path: String,
    ) -> Self {
        Self {
            id,
            name,
            kind,
            type_annotation: None,
            signature: None,
            docstring: None,
            span,
            visibility: Visibility::Private,
            file_path,
            module_path: Vec::new(),
            is_recursive: false,
            is_async: false,
            is_unsafe: false,
        }
    }

    /// Set type annotation
    pub fn with_type_annotation(mut self, type_annotation: TypeAnnotation) -> Self {
        self.type_annotation = Some(type_annotation);
        self
    }

    /// Set signature
    pub fn with_signature(mut self, signature: String) -> Self {
        self.signature = Some(signature);
        self
    }

    /// Set docstring
    pub fn with_docstring(mut self, docstring: String) -> Self {
        self.docstring = Some(docstring);
        self
    }

    /// Set visibility
    pub fn with_visibility(mut self, visibility: Visibility) -> Self {
        self.visibility = visibility;
        self
    }

    /// Set module path
    pub fn with_module_path(mut self, module_path: Vec<String>) -> Self {
        self.module_path = module_path;
        self
    }

    /// Mark as recursive
    pub fn mark_recursive(mut self) -> Self {
        self.is_recursive = true;
        self
    }

    /// Mark as async
    pub fn mark_async(mut self) -> Self {
        self.is_async = true;
        self
    }

    /// Mark as unsafe
    pub fn mark_unsafe(mut self) -> Self {
        self.is_unsafe = true;
        self
    }

    /// Get full qualified name
    pub fn qualified_name(&self) -> String {
        if self.module_path.is_empty() {
            self.name.clone()
        } else {
            format!("{}::{}", self.module_path.join("::"), self.name)
        }
    }

    /// Get display name with kind
    pub fn display_name(&self) -> String {
        format!("{} ({})", self.name, self.kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_id_creation() {
        let id1 = SymbolId::new("file.rs", "1:1:1:10", "test_function");
        let id2 = SymbolId::new("file.rs", "1:1:1:10", "test_function");
        let id3 = SymbolId::new("file.rs", "2:1:2:10", "test_function");
        
        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
    }

    #[test]
    fn test_type_annotation() {
        let mut type_annotation = TypeAnnotation::new();
        type_annotation.declared_type = Some("i32".to_string());
        type_annotation.return_type = Some("String".to_string());
        type_annotation.add_parameter_type("&str".to_string());
        
        assert_eq!(type_annotation.primary_type(), Some(&"i32".to_string()));
        assert!(type_annotation.to_string().contains("declared: i32"));
        assert!(type_annotation.to_string().contains("returns: String"));
    }

    #[test]
    fn test_symbol_node() {
        let id = SymbolId::new("test.rs", "1:1:1:20", "test_fn");
        let span = Span::new(1, 1, 1, 20, 0, 20);
        
        let symbol = SymbolNode::new(
            id,
            "test_fn".to_string(),
            SymbolKind::Function,
            span,
            "test.rs".to_string(),
        )
        .with_visibility(Visibility::Public)
        .mark_async();
        
        assert_eq!(symbol.name, "test_fn");
        assert_eq!(symbol.kind, SymbolKind::Function);
        assert_eq!(symbol.visibility, Visibility::Public);
        assert!(symbol.is_async);
        assert!(!symbol.is_unsafe);
    }
}
