//! Example application demonstrating Bronzite compile-time reflection.
//!
//! This application uses proc-macros from `my-macros` that query the Bronzite
//! daemon to introspect types from `my-types` at compile time.
//!
//! # Running this example
//!
//! 1. Make sure the bronzite daemon is installed and the required toolchain is available:
//!    ```sh
//!    rustup toolchain install nightly-2025-08-20
//!    cargo install --path /path/to/bronzite
//!    ```
//!
//! 2. Start the daemon pointing at the my-types crate:
//!    ```sh
//!    bronzite-daemon --manifest-path ../my-types
//!    ```
//!
//! 3. Build and run this example:
//!    ```sh
//!    cargo run
//!    ```

use my_macros::{implements_trait, list_methods, list_trait_impls};
use my_types::{Product, Serialize, User};

// Use Bronzite to discover traits implemented by User at compile time
list_trait_impls!("my_types", User);
// This expands to: const USER_TRAITS: &[&str] = &["Serialize", "HasId", "Debug", "Clone"];

// Discover methods on User
list_methods!("my_types", User);
// This expands to: const USER_METHODS: &[&str] = &["new", "deactivate", "is_active"];

// Check trait implementation at compile time
const USER_IS_SERIALIZABLE: bool = implements_trait!("my_types", User, "Serialize");
const PRODUCT_IS_SERIALIZABLE: bool = implements_trait!("my_types", Product, "Serialize");

fn main() {
    println!("=== Bronzite Compile-Time Reflection Demo ===\n");

    // Show discovered trait implementations
    println!("Traits implemented by User (discovered at compile time):");
    for trait_name in USER_TRAITS {
        println!("  - {}", trait_name);
    }
    println!();

    // Show discovered methods
    println!("Methods on User (discovered at compile time):");
    for method_name in USER_METHODS {
        println!("  - {}()", method_name);
    }
    println!();

    // Show compile-time trait checks
    println!("Compile-time trait checks:");
    println!("  User implements Serialize: {}", USER_IS_SERIALIZABLE);
    println!(
        "  Product implements Serialize: {}",
        PRODUCT_IS_SERIALIZABLE
    );
    println!();

    // Actually use the types
    println!("Using the types at runtime:");
    let user = User::new(1, "Alice".to_string(), "alice@example.com".to_string());
    println!("  User: {:?}", user);
    println!("  Serialized: {}", user.serialize());

    let product = Product::new("SKU-001".to_string(), "Widget".to_string(), 29.99);
    println!("  Product: {:?}", product);
    println!("  Serialized: {}", product.serialize());

    println!("\n=== Demo Complete ===");
}
