//! # 🔮 Bronzite - Compile-time Type Reflection for Rust
//!
//! Bronzite provides powerful compile-time access to type information, trait implementations,
//! method bodies, and more. It enables proc-macros to introspect Rust code using a daemon
//! that leverages rustc internals.
//!
//! ## ✨ Quick Start
//!
//! ### High-Level Reflection API (Recommended)
//!
//! The v0.2 API provides an ergonomic, type-safe interface for exploring types:
//!
//! ```ignore
//! use bronzite_client::Crate;
//!
//! #[proc_macro]
//! pub fn my_macro(input: TokenStream) -> TokenStream {
//!     // Connect to daemon and reflect on a crate
//!     let krate = Crate::reflect("my_crate").unwrap();
//!
//!     // Get a struct and explore it
//!     let user = krate.get_struct("User").unwrap();
//!
//!     // Navigate to fields
//!     for field in user.fields().unwrap() {
//!         println!("{}: {}", field.name.unwrap(), field.ty);
//!
//!         // Navigate to field's type definition
//!         if let Some(field_type) = field.type_def().unwrap() {
//!             println!("  -> defined in: {}", field_type.path());
//!         }
//!     }
//!
//!     // Check trait implementations
//!     if user.implements("Debug").unwrap() {
//!         println!("User implements Debug!");
//!     }
//!
//!     // Get methods
//!     for method in user.methods().unwrap() {
//!         println!("Method: {}", method.name);
//!         if let Some(body) = &method.body_source {
//!             println!("  Body: {}", body);
//!         }
//!     }
//!
//!     // Generate code based on discoveries
//!     quote! { /* ... */ }.into()
//! }
//! ```
//!
//! ### Pattern Matching
//!
//! Query types using intuitive glob patterns:
//!
//! ```ignore
//! use bronzite_client::Crate;
//!
//! let krate = Crate::reflect("my_crate")?;
//!
//! // Exact match
//! let user = krate.get_struct("User")?;
//!
//! // Single-level glob: matches "foo::Bar" but not "foo::bar::Baz"
//! let items = krate.items("mymod::*")?;
//!
//! // Recursive glob: matches all descendants
//! let all_items = krate.items("mymod::**")?;
//!
//! // Get only specific types
//! let structs = krate.structs("*")?;
//! let enums = krate.enums("*")?;
//! let traits = krate.traits("*")?;
//! ```
//!
//! ### Built-in Proc-Macros
//!
//! For simple use cases, use the built-in macros:
//!
//! ```ignore
//! use bronzite::bronzite_trait_names;
//!
//! // Get trait implementations as a const array
//! const USER_TRAITS: &[&str] = bronzite_trait_names!("my_crate", "User");
//! // Expands to: &["Debug", "Clone", "Serialize", ...]
//! ```
//!
//! ## 🏗️ Architecture
//!
//! Bronzite consists of several components working together:
//!
//! - **[`bronzite_client`]**: High-level reflection API and low-level RPC client
//! - **`bronzite-daemon`**: Background daemon that caches rustc compilation results
//! - **[`bronzite_macros`]**: Ready-to-use proc-macros for common reflection tasks
//! - **`bronzite-query`**: Rustc plugin that extracts type information
//! - **[`bronzite_types`]**: Shared types for the IPC protocol
//!
//! ## 📦 Installation
//!
//! Add to your `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! bronzite = "0.2"
//! bronzite-client = "0.2"  # For proc-macro development
//! ```
//!
//! Install the daemon:
//!
//! ```bash
//! cargo install bronzite
//! rustup toolchain install nightly-2025-08-20
//! ```
//!
//! The daemon auto-starts when you use the reflection API.
//!
//! ## 🔍 API Overview
//!
//! ### Navigation Methods
//!
//! The high-level API allows fluent navigation between related types:
//!
//! - **Struct → Fields → Field Types**: `struct.fields()` → `field.type_def()`
//! - **Struct → Methods → Return Types**: `struct.methods()` → `method.return_type_def()`
//! - **Trait → Implementors**: `trait.implementors()`
//! - **Type Alias → Concrete Type**: `alias.resolve()`
//!
//! ### Type Information
//!
//! Access comprehensive type metadata:
//!
//! - Struct fields with types, visibility, and layout
//! - Enum variants with discriminants
//! - Trait methods with signatures and default implementations
//! - Method bodies as source code
//! - Generic parameters and where clauses
//! - Documentation comments
//!
//! ## 📚 Learn More
//!
//! - [High-level API documentation](bronzite_client::reflection)
//! - [Built-in macros](bronzite_macros)
//! - [GitHub Repository](https://github.com/drewridley/bronzite)
//!
//! ## 🎯 Use Cases
//!
//! - **Derive macro helpers**: Generate implementations based on field types
//! - **Validation**: Check trait bounds at compile time
//! - **Code generation**: Generate boilerplate based on type structure
//! - **Static analysis**: Analyze type relationships in proc-macros
//! - **Documentation tools**: Extract and process doc comments

// Re-export the high-level reflection API
pub use bronzite_client::reflection::{
    Crate, EnumDef, Field, Item, Method, StructDef, TraitDef, TraitImpl, TraitMethod, TypeAliasDef,
    UnionDef,
};

// Re-export the low-level client for advanced use
pub use bronzite_client::{
    BronziteClient, Error, Result, connect, connect_for_workspace, connect_or_start,
    ensure_daemon_running, ensure_daemon_running_with_timeout, is_daemon_running,
};

// Re-export the built-in proc-macros
pub use bronzite_macros::{
    bronzite_crate_traits, bronzite_field_names, bronzite_implementors, bronzite_implements,
    bronzite_method_names, bronzite_resolve_alias, bronzite_trait_names,
};

// Re-export common types for working with query results
pub use bronzite_types::{
    AssocConstInfo, AssocTypeInfo, EnumVariantInfo, FieldInfo, FunctionSignature, GenericParam,
    GenericParamKind, InherentImplDetails, ItemInfo, ItemKind, LayoutInfo, MethodDetails,
    MethodSummary, TraitDetails, TraitImplDetails, TraitInfo, TraitMethodInfo, TypeAliasInfo,
    TypeDetails, TypeKind, TypeSummary, Visibility,
};
