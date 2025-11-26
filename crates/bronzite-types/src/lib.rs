//! Shared types for Bronzite IPC protocol.
//!
//! This crate defines the query and response types used for communication
//! between proc-macros (clients) and the Bronzite daemon.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ============================================================================
// Requests and Responses
// ============================================================================

/// A request sent from a client to the Bronzite daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    /// Unique request ID for correlating responses
    pub id: u64,
    /// The crate being queried (e.g., "my_crate")
    pub crate_name: String,
    /// The query to execute
    pub query: Query,
}

/// Available queries for type system introspection.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Query {
    /// List all items in the crate
    ListItems,

    /// Get detailed information about a specific type
    GetType { path: String },

    /// Get all trait implementations for a type
    GetTraitImpls { type_path: String },

    /// Get inherent impl blocks for a type (impl Foo { ... })
    GetInherentImpls { type_path: String },

    /// Get all fields of a struct or enum variant
    GetFields { type_path: String },

    /// Get memory layout information for a type
    GetLayout { type_path: String },

    /// Get all traits defined in the crate
    GetTraits,

    /// Get detailed information about a trait
    GetTrait { path: String },

    /// Find types matching a path pattern (e.g., "bevy::prelude::*")
    FindTypes { pattern: String },

    /// Resolve a type alias to its underlying type
    ResolveAlias { path: String },

    /// Check if a type implements a trait
    CheckImpl {
        type_path: String,
        trait_path: String,
    },

    /// Get all types that implement a specific trait
    GetImplementors { trait_path: String },

    /// Ping to check if daemon is alive
    Ping,

    /// Request the daemon to shut down
    Shutdown,
}

/// A response from the Bronzite daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    /// The request ID this response corresponds to
    pub id: u64,
    /// The result of the query
    pub result: QueryResult,
}

/// The result of a query execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum QueryResult {
    /// Query executed successfully
    Success { data: QueryData },
    /// Query failed with an error
    Error { message: String },
}

/// Data returned from successful queries.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum QueryData {
    /// Response to ListItems
    Items { items: Vec<ItemInfo> },

    /// Response to GetType
    TypeInfo(TypeDetails),

    /// Response to GetTraitImpls
    TraitImpls { impls: Vec<TraitImplDetails> },

    /// Response to GetInherentImpls
    InherentImpls { impls: Vec<InherentImplDetails> },

    /// Response to GetFields
    Fields { fields: Vec<FieldInfo> },

    /// Response to GetLayout
    Layout(LayoutInfo),

    /// Response to GetTraits
    Traits { traits: Vec<TraitInfo> },

    /// Response to GetTrait
    TraitDetails(TraitDetails),

    /// Response to FindTypes
    Types { types: Vec<TypeSummary> },

    /// Response to ResolveAlias
    ResolvedType {
        original: String,
        resolved: String,
        chain: Vec<String>,
    },

    /// Response to CheckImpl
    ImplCheck {
        implements: bool,
        impl_info: Option<TraitImplDetails>,
    },

    /// Response to GetImplementors
    Implementors { types: Vec<TypeSummary> },

    /// Response to Ping
    Pong,

    /// Response to Shutdown
    ShuttingDown,
}

// ============================================================================
// Item Information
// ============================================================================

/// Basic information about an item in the crate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemInfo {
    /// The item's name
    pub name: String,
    /// The full path to the item
    pub path: String,
    /// What kind of item this is
    pub kind: ItemKind,
    /// Visibility of the item
    pub visibility: Visibility,
    /// Span information (file, line, column)
    pub span: Option<SpanInfo>,
}

/// The kind of an item.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    Struct,
    Enum,
    Union,
    Trait,
    Function,
    Const,
    Static,
    TypeAlias,
    Impl,
    Mod,
    Use,
    ExternCrate,
    Macro,
    TraitAlias,
    Other(String),
}

/// Visibility of an item.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    Public,
    Crate,
    Restricted { path: String },
    Private,
}

/// Source location information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanInfo {
    pub file: String,
    pub start_line: u32,
    pub start_col: u32,
    pub end_line: u32,
    pub end_col: u32,
}

// ============================================================================
// Type Information
// ============================================================================

/// Summary information about a type (for listings).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeSummary {
    pub name: String,
    pub path: String,
    pub kind: TypeKind,
    pub generics: Vec<GenericParam>,
}

/// Detailed information about a type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeDetails {
    pub name: String,
    pub path: String,
    pub kind: TypeKind,
    pub visibility: Visibility,
    pub generics: Vec<GenericParam>,
    pub where_clause: Option<String>,
    /// Doc comments
    pub docs: Option<String>,
    /// Attributes (as strings)
    pub attributes: Vec<String>,
    /// For structs: fields
    pub fields: Option<Vec<FieldInfo>>,
    /// For enums: variants
    pub variants: Option<Vec<EnumVariantInfo>>,
    /// All trait implementations
    pub trait_impls: Vec<String>,
    /// Inherent methods
    pub inherent_methods: Vec<MethodSummary>,
    /// Layout information (if available)
    pub layout: Option<LayoutInfo>,
    /// Original source code (if available)
    pub source: Option<String>,
    pub span: Option<SpanInfo>,
}

/// The kind of a type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TypeKind {
    Struct,
    Enum,
    Union,
    Trait,
    TypeAlias,
    Primitive,
    Tuple,
    Array,
    Slice,
    Reference,
    Pointer,
    Function,
    Closure,
    Opaque,
}

/// A generic parameter (lifetime, type, or const).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenericParam {
    pub name: String,
    pub kind: GenericParamKind,
    /// Bounds on this parameter
    pub bounds: Vec<String>,
    /// Default value (if any)
    pub default: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GenericParamKind {
    Lifetime,
    Type,
    Const { ty: String },
}

// ============================================================================
// Field Information
// ============================================================================

/// Information about a struct field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldInfo {
    /// Field name (None for tuple struct fields)
    pub name: Option<String>,
    /// Field index
    pub index: usize,
    /// Type as a string
    pub ty: String,
    /// Resolved/canonical type
    pub resolved_ty: Option<String>,
    /// Visibility of the field
    pub visibility: Visibility,
    /// Doc comments
    pub docs: Option<String>,
    /// Attributes
    pub attributes: Vec<String>,
    /// Offset in bytes (if layout is known)
    pub offset: Option<usize>,
    /// Size in bytes (if layout is known)
    pub size: Option<usize>,
    pub span: Option<SpanInfo>,
}

/// Information about an enum variant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnumVariantInfo {
    pub name: String,
    pub index: usize,
    /// Fields of this variant
    pub fields: Vec<FieldInfo>,
    /// Discriminant value (if specified)
    pub discriminant: Option<String>,
    /// Doc comments
    pub docs: Option<String>,
    /// Attributes
    pub attributes: Vec<String>,
    pub span: Option<SpanInfo>,
}

// ============================================================================
// Layout Information
// ============================================================================

/// Memory layout information for a type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutInfo {
    /// Size in bytes
    pub size: usize,
    /// Alignment in bytes
    pub align: usize,
    /// Field offsets (for structs)
    pub field_offsets: Option<Vec<FieldLayoutInfo>>,
    /// Variant layouts (for enums)
    pub variants: Option<Vec<VariantLayoutInfo>>,
    /// Whether this type is sized
    pub is_sized: bool,
    /// Whether this type is Copy
    pub is_copy: bool,
    /// Whether this type is Send
    pub is_send: bool,
    /// Whether this type is Sync
    pub is_sync: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldLayoutInfo {
    pub name: Option<String>,
    pub index: usize,
    pub offset: usize,
    pub size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariantLayoutInfo {
    pub name: String,
    pub discriminant: Option<i128>,
    pub fields: Vec<FieldLayoutInfo>,
}

// ============================================================================
// Impl Information
// ============================================================================

/// Detailed information about a trait implementation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraitImplDetails {
    /// The implementing type
    pub self_ty: String,
    /// The trait being implemented
    pub trait_path: String,
    /// Generic parameters on the impl
    pub generics: Vec<GenericParam>,
    /// Where clause
    pub where_clause: Option<String>,
    /// Whether this is a negative impl (!Trait)
    pub is_negative: bool,
    /// Whether this is an unsafe impl
    pub is_unsafe: bool,
    /// Methods in this impl
    pub methods: Vec<MethodDetails>,
    /// Associated types
    pub assoc_types: Vec<AssocTypeInfo>,
    /// Associated constants
    pub assoc_consts: Vec<AssocConstInfo>,
    /// The full source code of the impl block
    pub source: Option<String>,
    pub span: Option<SpanInfo>,
}

/// Detailed information about an inherent impl block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InherentImplDetails {
    /// The type this impl is for
    pub self_ty: String,
    /// Generic parameters
    pub generics: Vec<GenericParam>,
    /// Where clause
    pub where_clause: Option<String>,
    /// Whether this is an unsafe impl
    pub is_unsafe: bool,
    /// Methods in this impl
    pub methods: Vec<MethodDetails>,
    /// Associated constants
    pub assoc_consts: Vec<AssocConstInfo>,
    /// Associated types
    pub assoc_types: Vec<AssocTypeInfo>,
    /// The full source code
    pub source: Option<String>,
    pub span: Option<SpanInfo>,
}

/// Summary of a method (for listings).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MethodSummary {
    pub name: String,
    pub path: String,
    pub signature: String,
    pub is_unsafe: bool,
    pub is_const: bool,
    pub is_async: bool,
}

/// Detailed information about a method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MethodDetails {
    pub name: String,
    pub path: String,
    /// Full signature as a string
    pub signature: String,
    /// Parsed signature components
    pub parsed_signature: FunctionSignature,
    /// Whether this has a default implementation
    pub has_body: bool,
    /// Method body source code (if available)
    pub body_source: Option<String>,
    /// Body as tokens (simplified AST)
    pub body_tokens: Option<Vec<Token>>,
    /// Whether this is unsafe
    pub is_unsafe: bool,
    /// Whether this is const
    pub is_const: bool,
    /// Whether this is async
    pub is_async: bool,
    /// Doc comments
    pub docs: Option<String>,
    /// Attributes
    pub attributes: Vec<String>,
    pub span: Option<SpanInfo>,
}

/// Parsed function signature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionSignature {
    /// Receiver (self, &self, &mut self, etc.)
    pub receiver: Option<ReceiverInfo>,
    /// Parameters (excluding self)
    pub params: Vec<ParamInfo>,
    /// Return type
    pub return_ty: Option<String>,
    /// Generic parameters
    pub generics: Vec<GenericParam>,
    /// Where clause
    pub where_clause: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceiverInfo {
    /// "self", "&self", "&mut self", "self: Pin<&mut Self>", etc.
    pub kind: String,
    /// Whether it's mutable
    pub is_mut: bool,
    /// Whether it's a reference
    pub is_ref: bool,
    /// Lifetime (if reference)
    pub lifetime: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamInfo {
    pub name: String,
    pub ty: String,
    pub is_mut: bool,
}

/// Associated type information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssocTypeInfo {
    pub name: String,
    /// The concrete type (in an impl)
    pub ty: Option<String>,
    /// Bounds (in a trait definition)
    pub bounds: Vec<String>,
    /// Default type (in a trait definition)
    pub default: Option<String>,
    pub docs: Option<String>,
    pub span: Option<SpanInfo>,
}

/// Associated constant information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssocConstInfo {
    pub name: String,
    pub ty: String,
    /// The value (if specified)
    pub value: Option<String>,
    pub docs: Option<String>,
    pub span: Option<SpanInfo>,
}

// ============================================================================
// Trait Information
// ============================================================================

/// Summary information about a trait.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraitInfo {
    pub name: String,
    pub path: String,
    pub generics: Vec<GenericParam>,
    /// Number of required methods
    pub required_methods: usize,
    /// Number of provided methods
    pub provided_methods: usize,
    /// Supertraits
    pub supertraits: Vec<String>,
}

/// Detailed information about a trait.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraitDetails {
    pub name: String,
    pub path: String,
    pub visibility: Visibility,
    pub generics: Vec<GenericParam>,
    pub where_clause: Option<String>,
    /// Is this an auto trait?
    pub is_auto: bool,
    /// Is this an unsafe trait?
    pub is_unsafe: bool,
    /// Supertraits
    pub supertraits: Vec<String>,
    /// Methods defined in this trait
    pub methods: Vec<TraitMethodInfo>,
    /// Associated types
    pub assoc_types: Vec<AssocTypeInfo>,
    /// Associated constants
    pub assoc_consts: Vec<AssocConstInfo>,
    /// Doc comments
    pub docs: Option<String>,
    /// Attributes
    pub attributes: Vec<String>,
    /// Full source code
    pub source: Option<String>,
    /// Types that implement this trait (in the current crate)
    pub implementors: Vec<String>,
    pub span: Option<SpanInfo>,
}

/// Information about a trait method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraitMethodInfo {
    pub name: String,
    pub signature: String,
    pub parsed_signature: FunctionSignature,
    /// Does this method have a default implementation?
    pub has_default: bool,
    /// Default implementation source
    pub default_body: Option<String>,
    pub is_unsafe: bool,
    pub docs: Option<String>,
    pub attributes: Vec<String>,
    pub span: Option<SpanInfo>,
}

// ============================================================================
// Token/AST Information
// ============================================================================

/// Simplified token representation for method bodies.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "token_type", rename_all = "snake_case")]
pub enum Token {
    /// An identifier
    Ident { name: String },
    /// A literal value
    Literal { kind: LiteralKind, value: String },
    /// A punctuation symbol
    Punct { ch: char },
    /// A keyword
    Keyword { name: String },
    /// A group (delimited tokens)
    Group {
        delimiter: Delimiter,
        tokens: Vec<Token>,
    },
    /// A path (e.g., std::collections::HashMap)
    Path { segments: Vec<String> },
    /// A method call
    MethodCall {
        receiver: Box<Token>,
        method: String,
        args: Vec<Token>,
    },
    /// A function call
    FnCall { path: Vec<String>, args: Vec<Token> },
    /// A field access
    FieldAccess { base: Box<Token>, field: String },
    /// A binary operation
    BinOp {
        lhs: Box<Token>,
        op: String,
        rhs: Box<Token>,
    },
    /// A unary operation
    UnaryOp { op: String, expr: Box<Token> },
    /// An if expression
    If {
        cond: Box<Token>,
        then_branch: Vec<Token>,
        else_branch: Option<Vec<Token>>,
    },
    /// A match expression
    Match {
        expr: Box<Token>,
        arms: Vec<MatchArm>,
    },
    /// A let binding
    Let {
        pattern: String,
        ty: Option<String>,
        init: Option<Box<Token>>,
    },
    /// A return statement
    Return { expr: Option<Box<Token>> },
    /// A block
    Block { stmts: Vec<Token> },
    /// A closure
    Closure {
        params: Vec<String>,
        body: Box<Token>,
    },
    /// Raw source when we can't parse further
    Raw { source: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchArm {
    pub pattern: String,
    pub guard: Option<String>,
    pub body: Vec<Token>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LiteralKind {
    String,
    ByteString,
    Char,
    Byte,
    Int,
    Float,
    Bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Delimiter {
    Paren,
    Bracket,
    Brace,
    None,
}

// ============================================================================
// Cached/Extracted Type Information
// ============================================================================

/// Complete extracted type information for a crate.
/// This is what the daemon caches after compilation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CrateTypeInfo {
    /// Name of the crate
    pub crate_name: String,
    /// Crate version (if known)
    pub crate_version: Option<String>,

    /// All items in the crate
    pub items: Vec<ItemInfo>,

    /// Detailed type information (structs, enums, unions)
    pub types: HashMap<String, TypeDetails>,

    /// All trait definitions
    pub traits: HashMap<String, TraitDetails>,

    /// Trait implementations (keyed by implementing type)
    pub trait_impls: HashMap<String, Vec<TraitImplDetails>>,

    /// Inherent impls (keyed by type)
    pub inherent_impls: HashMap<String, Vec<InherentImplDetails>>,

    /// Type aliases (path -> resolved type)
    pub type_aliases: HashMap<String, TypeAliasInfo>,

    /// Layout information (keyed by type path)
    pub layouts: HashMap<String, LayoutInfo>,

    /// Module tree for path matching
    pub modules: HashMap<String, ModuleInfo>,
}

/// Information about a type alias.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeAliasInfo {
    pub name: String,
    pub path: String,
    pub generics: Vec<GenericParam>,
    /// The aliased type
    pub ty: String,
    /// Fully resolved type (following all aliases)
    pub resolved_ty: String,
    pub visibility: Visibility,
    pub docs: Option<String>,
    pub span: Option<SpanInfo>,
}

/// Information about a module.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleInfo {
    pub name: String,
    pub path: String,
    pub visibility: Visibility,
    /// Child items (names only)
    pub items: Vec<String>,
    /// Re-exports
    pub reexports: Vec<ReexportInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReexportInfo {
    /// The name as exported
    pub name: String,
    /// The original path
    pub original_path: String,
    pub visibility: Visibility,
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Default socket path for the Bronzite daemon.
pub fn default_socket_path() -> std::path::PathBuf {
    std::env::temp_dir().join("bronzite.sock")
}

/// Socket path for a specific crate/workspace.
pub fn socket_path_for_workspace(workspace_root: &std::path::Path) -> std::path::PathBuf {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    workspace_root.hash(&mut hasher);
    let hash = hasher.finish();

    std::env::temp_dir().join(format!("bronzite-{:x}.sock", hash))
}

// ============================================================================
// Pattern Matching for FindTypes
// ============================================================================

/// Check if a path matches a pattern.
/// Supports:
/// - Exact match: "foo::Bar"
/// - Glob suffix: "foo::*"
/// - Recursive glob: "foo::**"
/// - Wildcards: "foo::Bar*"
pub fn path_matches_pattern(path: &str, pattern: &str) -> bool {
    let pattern = pattern.trim();
    let path = path.trim();

    // Recursive glob: foo::** matches foo::bar::baz
    if pattern.ends_with("::**") {
        let prefix = &pattern[..pattern.len() - 4];
        return path == prefix || path.starts_with(&format!("{}::", prefix));
    }

    // Single level glob: foo::* matches foo::bar but not foo::bar::baz
    if pattern.ends_with("::*") {
        let prefix = &pattern[..pattern.len() - 3];
        if !path.starts_with(&format!("{}::", prefix)) {
            return false;
        }
        let suffix = &path[prefix.len() + 2..];
        return !suffix.contains("::");
    }

    // Wildcard in name: foo::Bar* matches foo::BarBaz
    if pattern.contains('*') {
        let parts: Vec<&str> = pattern.split('*').collect();
        if parts.len() != 2 {
            return false; // Only support single wildcard for now
        }
        return path.starts_with(parts[0]) && path.ends_with(parts[1]);
    }

    // Exact match
    path == pattern
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_matching() {
        // Exact match
        assert!(path_matches_pattern("foo::Bar", "foo::Bar"));
        assert!(!path_matches_pattern("foo::Baz", "foo::Bar"));

        // Single level glob
        assert!(path_matches_pattern("foo::Bar", "foo::*"));
        assert!(path_matches_pattern("foo::Baz", "foo::*"));
        assert!(!path_matches_pattern("foo::bar::Baz", "foo::*"));

        // Recursive glob
        assert!(path_matches_pattern("foo::Bar", "foo::**"));
        assert!(path_matches_pattern("foo::bar::Baz", "foo::**"));
        assert!(path_matches_pattern("foo::bar::baz::Qux", "foo::**"));
        assert!(!path_matches_pattern("bar::Baz", "foo::**"));

        // Wildcard
        assert!(path_matches_pattern("foo::BarBaz", "foo::Bar*"));
        assert!(path_matches_pattern("foo::Bar", "foo::Bar*"));
        assert!(!path_matches_pattern("foo::Baz", "foo::Bar*"));
    }

    #[test]
    fn test_query_serialization() {
        let query = Query::CheckImpl {
            type_path: "Foo".to_string(),
            trait_path: "MyTrait".to_string(),
        };
        let json = serde_json::to_string(&query).unwrap();
        let parsed: Query = serde_json::from_str(&json).unwrap();

        match parsed {
            Query::CheckImpl {
                type_path,
                trait_path,
            } => {
                assert_eq!(type_path, "Foo");
                assert_eq!(trait_path, "MyTrait");
            }
            _ => panic!("Wrong query type"),
        }
    }

    #[test]
    fn test_response_serialization() {
        let response = Response {
            id: 42,
            result: QueryResult::Success {
                data: QueryData::ImplCheck {
                    implements: true,
                    impl_info: None,
                },
            },
        };
        let json = serde_json::to_string(&response).unwrap();
        let parsed: Response = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, 42);
    }
}
