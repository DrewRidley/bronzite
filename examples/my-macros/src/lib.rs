//! Example proc-macro crate demonstrating Bronzite compile-time reflection.
//!
//! This crate shows how to use bronzite-client to query type information
//! at compile time from within a proc-macro.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;

/// Generate a list of trait implementations for a type.
///
/// This macro queries the Bronzite daemon to discover which traits
/// a type implements, then generates code to expose that information.
///
/// # Usage
///
/// ```ignore
/// // In your types crate, define some types with trait impls
/// // Then in your application:
///
/// list_trait_impls!("my_types", User);
/// // Expands to something like:
/// // const USER_TRAITS: &[&str] = &["Serialize", "HasId", "Debug", "Clone"];
/// ```
#[proc_macro]
pub fn list_trait_impls(input: TokenStream) -> TokenStream {
    let input_str = input.to_string();
    let parts: Vec<&str> = input_str.split(',').map(|s| s.trim()).collect();

    if parts.len() != 2 {
        return quote! {
            compile_error!("Expected: list_trait_impls!(\"crate_name\", TypeName)")
        }
        .into();
    }

    let crate_name = parts[0].trim_matches('"');
    let type_name = parts[1].trim();

    // Ensure daemon is running and connect
    match bronzite_client::connect_or_start(None) {
        Ok(mut client) => {
            match client.get_trait_impls(crate_name, type_name) {
                Ok(impls) => {
                    let trait_names: Vec<String> = impls
                        .iter()
                        .map(|i| {
                            // Extract just the trait name from the full path
                            i.trait_path
                                .rsplit("::")
                                .next()
                                .unwrap_or(&i.trait_path)
                                .to_string()
                        })
                        .collect();

                    let const_name = syn::Ident::new(
                        &format!("{}_TRAITS", type_name.to_uppercase()),
                        proc_macro2::Span::call_site(),
                    );

                    let output = quote! {
                        const #const_name: &[&str] = &[#(#trait_names),*];
                    };
                    output.into()
                }
                Err(e) => {
                    let msg = format!("Bronzite query failed: {}", e);
                    quote! { compile_error!(#msg) }.into()
                }
            }
        }
        Err(e) => {
            let msg = format!(
                "Failed to connect to Bronzite daemon: {}. \
                 Make sure bronzite-daemon is installed and the toolchain nightly-2025-08-20 is available.",
                e
            );
            quote! { compile_error!(#msg) }.into()
        }
    }
}

/// Generate field accessor methods for a struct.
///
/// This macro queries Bronzite for the fields of a struct and generates
/// getter methods for each field.
///
/// # Usage
///
/// ```ignore
/// struct MyWrapper(my_types::User);
///
/// impl MyWrapper {
///     generate_getters!("my_types", User, inner);
/// }
/// // Generates: fn id(&self) -> &u64 { &self.inner.id }
/// //            fn name(&self) -> &String { &self.inner.name }
/// //            etc.
/// ```
#[proc_macro]
pub fn generate_getters(input: TokenStream) -> TokenStream {
    let input_str = input.to_string();
    let parts: Vec<&str> = input_str.split(',').map(|s| s.trim()).collect();

    if parts.len() != 3 {
        return quote! {
            compile_error!("Expected: generate_getters!(\"crate_name\", TypeName, field_name)")
        }
        .into();
    }

    let crate_name = parts[0].trim_matches('"');
    let type_name = parts[1].trim();
    let field_access = syn::Ident::new(parts[2].trim(), proc_macro2::Span::call_site());

    match bronzite_client::connect_or_start(None) {
        Ok(mut client) => match client.get_fields(crate_name, type_name) {
            Ok(fields) => {
                let getters: Vec<TokenStream2> = fields
                    .iter()
                    .filter_map(|f| {
                        let field_name = f.name.as_ref()?;
                        let field_ident =
                            syn::Ident::new(field_name, proc_macro2::Span::call_site());
                        let ty_str = &f.ty;

                        // Parse the type - simplified, just use the string as-is
                        // In a real impl you'd want proper type parsing
                        let ty: TokenStream2 = ty_str.parse().unwrap_or_else(|_| {
                            quote! { _ }
                        });

                        Some(quote! {
                            pub fn #field_ident(&self) -> &#ty {
                                &self.#field_access.#field_ident
                            }
                        })
                    })
                    .collect();

                let output = quote! {
                    #(#getters)*
                };
                output.into()
            }
            Err(e) => {
                let msg = format!("Bronzite query failed: {}", e);
                quote! { compile_error!(#msg) }.into()
            }
        },
        Err(e) => {
            let msg = format!("Failed to connect to Bronzite daemon: {}", e);
            quote! { compile_error!(#msg) }.into()
        }
    }
}

/// Check if a type implements a specific trait.
///
/// Returns `true` or `false` as a literal.
///
/// # Usage
///
/// ```ignore
/// const IS_SERIALIZABLE: bool = implements_trait!("my_types", User, "Serialize");
/// ```
#[proc_macro]
pub fn implements_trait(input: TokenStream) -> TokenStream {
    let input_str = input.to_string();
    let parts: Vec<&str> = input_str.split(',').map(|s| s.trim()).collect();

    if parts.len() != 3 {
        return quote! {
            compile_error!("Expected: implements_trait!(\"crate_name\", TypeName, \"TraitName\")")
        }
        .into();
    }

    let crate_name = parts[0].trim_matches('"');
    let type_name = parts[1].trim();
    let trait_name = parts[2].trim().trim_matches('"');

    match bronzite_client::connect_or_start(None) {
        Ok(mut client) => match client.check_impl(crate_name, type_name, trait_name) {
            Ok((implements, _)) => {
                if implements {
                    quote! { true }.into()
                } else {
                    quote! { false }.into()
                }
            }
            Err(e) => {
                let msg = format!("Bronzite query failed: {}", e);
                quote! { compile_error!(#msg) }.into()
            }
        },
        Err(e) => {
            let msg = format!("Failed to connect to Bronzite daemon: {}", e);
            quote! { compile_error!(#msg) }.into()
        }
    }
}

/// Get the method names from a type's inherent impl.
///
/// # Usage
///
/// ```ignore
/// const USER_METHODS: &[&str] = list_methods!("my_types", User);
/// // Expands to: &["new", "deactivate", "is_active"]
/// ```
#[proc_macro]
pub fn list_methods(input: TokenStream) -> TokenStream {
    let input_str = input.to_string();
    let parts: Vec<&str> = input_str.split(',').map(|s| s.trim()).collect();

    if parts.len() != 2 {
        return quote! {
            compile_error!("Expected: list_methods!(\"crate_name\", TypeName)")
        }
        .into();
    }

    let crate_name = parts[0].trim_matches('"');
    let type_name = parts[1].trim();

    match bronzite_client::connect_or_start(None) {
        Ok(mut client) => match client.get_inherent_impls(crate_name, type_name) {
            Ok(impls) => {
                let method_names: Vec<String> = impls
                    .iter()
                    .flat_map(|i| i.methods.iter())
                    .map(|m| m.name.clone())
                    .collect();

                let const_name = syn::Ident::new(
                    &format!("{}_METHODS", type_name.to_uppercase()),
                    proc_macro2::Span::call_site(),
                );

                let output = quote! {
                    const #const_name: &[&str] = &[#(#method_names),*];
                };
                output.into()
            }
            Err(e) => {
                let msg = format!("Bronzite query failed: {}", e);
                quote! { compile_error!(#msg) }.into()
            }
        },
        Err(e) => {
            let msg = format!("Failed to connect to Bronzite daemon: {}", e);
            quote! { compile_error!(#msg) }.into()
        }
    }
}
