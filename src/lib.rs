//! Bronzite - Compile-time type reflection for Rust
//!
//! Bronzite provides compile-time access to type information, trait implementations,
//! method bodies, and more. It works by running a daemon that uses rustc internals
//! to extract type information, which can then be queried from proc-macros.
//!
//! # Quick Start
//!
//! 1. Install bronzite: `cargo install bronzite`
//! 2. The daemon auto-starts when you use the macros
//!
//! # Usage in Proc-Macros
//!
//! If you're writing a proc-macro that needs type reflection, use `bronzite-client`:
//!
//! ```ignore
//! use bronzite_client::{connect_or_start, BronziteClient};
//!
//! #[proc_macro]
//! pub fn my_macro(input: TokenStream) -> TokenStream {
//!     let mut client = connect_or_start(None).unwrap();
//!     let impls = client.get_trait_impls("my_crate", "MyType").unwrap();
//!     // Generate code based on the implementations...
//! }
//! ```
//!
//! # Direct Usage
//!
//! ```ignore
//! use bronzite::bronzite_trait_names;
//!
//! // Get trait implementations as a const array
//! const IMPLS: &[&str] = bronzite_trait_names!("my_crate", "MyType");
//! ```
//!
//! # Architecture
//!
//! Bronzite consists of several components:
//!
//! - **bronzite-daemon**: A background daemon that caches rustc compilation results
//! - **bronzite-client**: A client library for connecting to the daemon
//! - **bronzite-macros**: Proc-macros that use the client to query type information
//! - **bronzite-query**: A rustc plugin that extracts type information
//! - **bronzite-types**: Shared types for the IPC protocol

// Re-export the client for programmatic use
pub use bronzite_client::{
    BronziteClient, Error, Result, connect, connect_for_workspace, connect_or_start,
    ensure_daemon_running, ensure_daemon_running_with_timeout, is_daemon_running,
};

// Re-export the proc-macros
pub use bronzite_macros::{
    bronzite_crate_traits, bronzite_field_names, bronzite_implementors, bronzite_implements,
    bronzite_method_names, bronzite_resolve_alias, bronzite_trait_names,
};

// Re-export types for working with query results
pub use bronzite_types::{
    AssocConstInfo, AssocTypeInfo, EnumVariantInfo, FieldInfo, GenericParam, GenericParamKind,
    InherentImplDetails, ItemInfo, ItemKind, LayoutInfo, MethodDetails, MethodSummary, Query,
    QueryData, QueryResult, Token, TraitDetails, TraitImplDetails, TraitInfo, TraitMethodInfo,
    TypeAliasInfo, TypeDetails, TypeKind, TypeSummary, Visibility,
};
