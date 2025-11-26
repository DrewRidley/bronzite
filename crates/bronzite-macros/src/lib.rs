//! Proc-macros for compile-time type reflection using Bronzite.
//!
//! This crate provides proc-macros that query type information from a running
//! Bronzite daemon. The daemon is automatically started if not already running.
//!
//! # Usage
//!
//! ```ignore
//! // Get trait implementations as a const array of trait names
//! const IMPLS: &[&str] = bronzite_trait_names!("my_crate", "MyStruct");
//! ```
//!
//! # Note for Proc-Macro Authors
//!
//! If you're writing a proc-macro and want to query Bronzite programmatically,
//! use the `bronzite-client` crate directly instead of this one:
//!
//! ```ignore
//! use bronzite_client::{connect_or_start, BronziteClient};
//!
//! let mut client = connect_or_start(None)?;
//! let impls = client.get_trait_impls("my_crate", "MyType")?;
//! ```

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use std::sync::OnceLock;

use bronzite_types::{
    FieldInfo, InherentImplDetails, TraitDetails, TraitImplDetails, TraitInfo, TypeDetails,
    TypeSummary,
};

/// Global flag tracking daemon initialization.
static DAEMON_INITIALIZED: OnceLock<bool> = OnceLock::new();

/// Ensure the daemon is running. Called automatically by query functions.
fn ensure_daemon() -> Result<(), String> {
    DAEMON_INITIALIZED.get_or_init(|| {
        match bronzite_client::ensure_daemon_running(None) {
            Ok(()) => true,
            Err(e) => {
                // We can't return an error from get_or_init, so we store false
                // and check it later
                eprintln!("[bronzite] Warning: Failed to start daemon: {}", e);
                false
            }
        }
    });

    if *DAEMON_INITIALIZED.get().unwrap_or(&false) {
        Ok(())
    } else {
        Err("Failed to initialize bronzite daemon".to_string())
    }
}

/// Get a connected client.
fn get_client() -> Result<bronzite_client::BronziteClient, String> {
    ensure_daemon()?;
    bronzite_client::connect().map_err(|e| e.to_string())
}

// ============================================================================
// Internal Query Functions
// ============================================================================

fn query_trait_impls(crate_name: &str, type_path: &str) -> Result<Vec<TraitImplDetails>, String> {
    let mut client = get_client()?;
    client
        .get_trait_impls(crate_name, type_path)
        .map_err(|e| e.to_string())
}

fn query_inherent_impls(
    crate_name: &str,
    type_path: &str,
) -> Result<Vec<InherentImplDetails>, String> {
    let mut client = get_client()?;
    client
        .get_inherent_impls(crate_name, type_path)
        .map_err(|e| e.to_string())
}

fn query_fields(crate_name: &str, type_path: &str) -> Result<Vec<FieldInfo>, String> {
    let mut client = get_client()?;
    client
        .get_fields(crate_name, type_path)
        .map_err(|e| e.to_string())
}

#[allow(dead_code)]
fn query_type(crate_name: &str, type_path: &str) -> Result<TypeDetails, String> {
    let mut client = get_client()?;
    client
        .get_type(crate_name, type_path)
        .map_err(|e| e.to_string())
}

fn query_check_impl(
    crate_name: &str,
    type_path: &str,
    trait_path: &str,
) -> Result<(bool, Option<TraitImplDetails>), String> {
    let mut client = get_client()?;
    client
        .check_impl(crate_name, type_path, trait_path)
        .map_err(|e| e.to_string())
}

fn query_traits(crate_name: &str) -> Result<Vec<TraitInfo>, String> {
    let mut client = get_client()?;
    client.get_traits(crate_name).map_err(|e| e.to_string())
}

#[allow(dead_code)]
fn query_trait(crate_name: &str, trait_path: &str) -> Result<TraitDetails, String> {
    let mut client = get_client()?;
    client
        .get_trait(crate_name, trait_path)
        .map_err(|e| e.to_string())
}

#[allow(dead_code)]
fn query_find_types(crate_name: &str, pattern: &str) -> Result<Vec<TypeSummary>, String> {
    let mut client = get_client()?;
    client
        .find_types(crate_name, pattern)
        .map_err(|e| e.to_string())
}

fn query_resolve_alias(
    crate_name: &str,
    alias_path: &str,
) -> Result<(String, String, Vec<String>), String> {
    let mut client = get_client()?;
    client
        .resolve_alias(crate_name, alias_path)
        .map_err(|e| e.to_string())
}

fn query_implementors(crate_name: &str, trait_path: &str) -> Result<Vec<TypeSummary>, String> {
    let mut client = get_client()?;
    client
        .get_implementors(crate_name, trait_path)
        .map_err(|e| e.to_string())
}

// ============================================================================
// Helper for parsing macro input
// ============================================================================

struct MacroArgs {
    crate_name: String,
    type_path: String,
    trait_path: Option<String>,
}

fn parse_two_args(input: TokenStream) -> Result<MacroArgs, TokenStream2> {
    let input_str = input.to_string();
    let parts: Vec<&str> = input_str.split(',').map(|s| s.trim()).collect();

    if parts.len() != 2 {
        return Err(quote! {
            compile_error!("Expected two arguments: crate_name, type_path")
        });
    }

    let crate_name = parts[0].trim_matches('"').to_string();
    let type_path = parts[1].trim_matches('"').to_string();

    Ok(MacroArgs {
        crate_name,
        type_path,
        trait_path: None,
    })
}

fn parse_three_args(input: TokenStream) -> Result<MacroArgs, TokenStream2> {
    let input_str = input.to_string();
    let parts: Vec<&str> = input_str.split(',').map(|s| s.trim()).collect();

    if parts.len() != 3 {
        return Err(quote! {
            compile_error!("Expected three arguments: crate_name, type_path, trait_path")
        });
    }

    let crate_name = parts[0].trim_matches('"').to_string();
    let type_path = parts[1].trim_matches('"').to_string();
    let trait_path = parts[2].trim_matches('"').to_string();

    Ok(MacroArgs {
        crate_name,
        type_path,
        trait_path: Some(trait_path),
    })
}

fn parse_one_arg(input: TokenStream) -> Result<String, TokenStream2> {
    let input_str = input.to_string();
    let crate_name = input_str.trim().trim_matches('"').to_string();

    if crate_name.is_empty() {
        return Err(quote! {
            compile_error!("Expected one argument: crate_name")
        });
    }

    Ok(crate_name)
}

// ============================================================================
// Proc-Macros
// ============================================================================

/// Get the names of all traits implemented by a type as a const slice.
///
/// # Example
///
/// ```ignore
/// const TRAIT_NAMES: &[&str] = bronzite_trait_names!("my_crate", "MyStruct");
/// // Expands to: &["Debug", "Clone", "MyTrait", ...]
/// ```
#[proc_macro]
pub fn bronzite_trait_names(input: TokenStream) -> TokenStream {
    let args = match parse_two_args(input) {
        Ok(a) => a,
        Err(e) => return e.into(),
    };

    match query_trait_impls(&args.crate_name, &args.type_path) {
        Ok(impls) => {
            let names: Vec<String> = impls.iter().map(|i| i.trait_path.clone()).collect();

            let output = quote! {
                &[#(#names),*]
            };
            output.into()
        }
        Err(e) => {
            let msg = format!("bronzite error: {}", e);
            quote! { compile_error!(#msg) }.into()
        }
    }
}

/// Check if a type implements a trait at compile time.
///
/// Returns `true` or `false` as a literal.
///
/// # Example
///
/// ```ignore
/// const IMPLEMENTS_DEBUG: bool = bronzite_implements!("my_crate", "MyStruct", "Debug");
/// ```
#[proc_macro]
pub fn bronzite_implements(input: TokenStream) -> TokenStream {
    let args = match parse_three_args(input) {
        Ok(a) => a,
        Err(e) => return e.into(),
    };

    let trait_path = args.trait_path.unwrap();

    match query_check_impl(&args.crate_name, &args.type_path, &trait_path) {
        Ok((implements, _)) => {
            let output = if implements {
                quote! { true }
            } else {
                quote! { false }
            };
            output.into()
        }
        Err(e) => {
            let msg = format!("bronzite error: {}", e);
            quote! { compile_error!(#msg) }.into()
        }
    }
}

/// Get field names of a struct as a const slice.
///
/// # Example
///
/// ```ignore
/// const FIELD_NAMES: &[&str] = bronzite_field_names!("my_crate", "MyStruct");
/// // Expands to: &["field1", "field2", ...]
/// ```
#[proc_macro]
pub fn bronzite_field_names(input: TokenStream) -> TokenStream {
    let args = match parse_two_args(input) {
        Ok(a) => a,
        Err(e) => return e.into(),
    };

    match query_fields(&args.crate_name, &args.type_path) {
        Ok(fields) => {
            let names: Vec<String> = fields.iter().filter_map(|f| f.name.clone()).collect();

            let output = quote! {
                &[#(#names),*]
            };
            output.into()
        }
        Err(e) => {
            let msg = format!("bronzite error: {}", e);
            quote! { compile_error!(#msg) }.into()
        }
    }
}

/// Get method names from a type's inherent impl as a const slice.
///
/// # Example
///
/// ```ignore
/// const METHOD_NAMES: &[&str] = bronzite_method_names!("my_crate", "MyStruct");
/// // Expands to: &["new", "do_thing", ...]
/// ```
#[proc_macro]
pub fn bronzite_method_names(input: TokenStream) -> TokenStream {
    let args = match parse_two_args(input) {
        Ok(a) => a,
        Err(e) => return e.into(),
    };

    match query_inherent_impls(&args.crate_name, &args.type_path) {
        Ok(impls) => {
            let names: Vec<String> = impls
                .iter()
                .flat_map(|i| i.methods.iter())
                .map(|m| m.name.clone())
                .collect();

            let output = quote! {
                &[#(#names),*]
            };
            output.into()
        }
        Err(e) => {
            let msg = format!("bronzite error: {}", e);
            quote! { compile_error!(#msg) }.into()
        }
    }
}

/// Get all trait names defined in a crate as a const slice.
///
/// # Example
///
/// ```ignore
/// const CRATE_TRAITS: &[&str] = bronzite_crate_traits!("my_crate");
/// ```
#[proc_macro]
pub fn bronzite_crate_traits(input: TokenStream) -> TokenStream {
    let crate_name = match parse_one_arg(input) {
        Ok(c) => c,
        Err(e) => return e.into(),
    };

    match query_traits(&crate_name) {
        Ok(traits) => {
            let names: Vec<String> = traits.iter().map(|t| t.name.clone()).collect();

            let output = quote! {
                &[#(#names),*]
            };
            output.into()
        }
        Err(e) => {
            let msg = format!("bronzite error: {}", e);
            quote! { compile_error!(#msg) }.into()
        }
    }
}

/// Resolve a type alias and get the underlying type as a string literal.
///
/// # Example
///
/// ```ignore
/// const RESOLVED: &str = bronzite_resolve_alias!("my_crate", "MyAlias");
/// // Expands to: "actual::Type"
/// ```
#[proc_macro]
pub fn bronzite_resolve_alias(input: TokenStream) -> TokenStream {
    let args = match parse_two_args(input) {
        Ok(a) => a,
        Err(e) => return e.into(),
    };

    match query_resolve_alias(&args.crate_name, &args.type_path) {
        Ok((_, resolved, _)) => {
            let output = quote! { #resolved };
            output.into()
        }
        Err(e) => {
            let msg = format!("bronzite error: {}", e);
            quote! { compile_error!(#msg) }.into()
        }
    }
}

/// Get types that implement a trait as a const slice of type path strings.
///
/// # Example
///
/// ```ignore
/// const IMPLEMENTORS: &[&str] = bronzite_implementors!("my_crate", "MyTrait");
/// ```
#[proc_macro]
pub fn bronzite_implementors(input: TokenStream) -> TokenStream {
    let args = match parse_two_args(input) {
        Ok(a) => a,
        Err(e) => return e.into(),
    };

    match query_implementors(&args.crate_name, &args.type_path) {
        Ok(types) => {
            let paths: Vec<String> = types.iter().map(|t| t.path.clone()).collect();

            let output = quote! {
                &[#(#paths),*]
            };
            output.into()
        }
        Err(e) => {
            let msg = format!("bronzite error: {}", e);
            quote! { compile_error!(#msg) }.into()
        }
    }
}
