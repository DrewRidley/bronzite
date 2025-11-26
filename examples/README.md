# Bronzite Examples

This directory contains example crates demonstrating how to use Bronzite for compile-time type reflection.

## Structure

- **my-types/** - A library crate with example types and traits to be introspected
- **my-macros/** - A proc-macro crate that uses `bronzite-client` to query type information
- **my-app/** - An application that uses the macros to demonstrate reflection

## Prerequisites

1. Install the required nightly toolchain:
   ```sh
   rustup toolchain install nightly-2025-08-20
   ```

2. Build and install the bronzite daemon from the workspace root:
   ```sh
   cd /path/to/bronzite
   cargo build --release
   ```

## Running the Example

### Step 1: Start the Bronzite Daemon

The daemon needs to be pointed at the crate you want to introspect (`my-types`):

```sh
cd /path/to/bronzite/examples

# Start daemon in background (or in a separate terminal)
../target/release/bronzite-daemon --manifest-path my-types &
```

Or use the `--ensure` flag which will start a daemon if one isn't already running:

```sh
../target/release/bronzite-daemon --ensure --manifest-path my-types
```

### Step 2: Build and Run the Application

```sh
cd my-app
cargo run
```

You should see output like:

```
=== Bronzite Compile-Time Reflection Demo ===

Traits implemented by User (discovered at compile time):
  - Serialize
  - HasId
  - Debug
  - Clone

Methods on User (discovered at compile time):
  - new()
  - deactivate()
  - is_active()

Compile-time trait checks:
  User implements Serialize: true
  Product implements Serialize: true

Using the types at runtime:
  User: User { id: 1, name: "Alice", email: "alice@example.com", active: true }
  Serialized: {"id":1,"name":"Alice","email":"alice@example.com","active":true}
  Product: Product { sku: "SKU-001", name: "Widget", price: 29.99 }
  Serialized: {"sku":"SKU-001","name":"Widget","price":29.99}

=== Demo Complete ===
```

## How It Works

1. **my-types** defines regular Rust types with trait implementations
2. **my-macros** provides proc-macros that:
   - Use `bronzite_client::connect_or_start()` to connect to the daemon
   - Query type information using methods like `get_trait_impls()`, `get_fields()`, etc.
   - Generate code based on the discovered information
3. **my-app** uses these macros, which execute at compile time to generate constants and code

## Writing Your Own Reflection Macros

Here's a minimal example of a proc-macro that uses Bronzite:

```rust
use proc_macro::TokenStream;
use quote::quote;

#[proc_macro]
pub fn get_field_count(input: TokenStream) -> TokenStream {
    let input_str = input.to_string();
    let parts: Vec<&str> = input_str.split(',').collect();
    let crate_name = parts[0].trim().trim_matches('"');
    let type_name = parts[1].trim();

    // Connect to daemon (starts one if needed)
    let mut client = bronzite_client::connect_or_start(None)
        .expect("Failed to connect to bronzite daemon");

    // Query field information
    let fields = client.get_fields(crate_name, type_name)
        .expect("Failed to query fields");

    let count = fields.len();
    quote! { #count }.into()
}
```

## Troubleshooting

### "Failed to connect to Bronzite daemon"

Make sure the daemon is running and pointed at the correct crate:
```sh
bronzite-daemon --ensure --manifest-path /path/to/your/types/crate
```

### "Crate not found in compilation output"

The daemon caches compilation results. If you've changed the target crate:
- Restart the daemon, or
- The cache will auto-refresh on errors

### Toolchain errors

Bronzite requires a specific nightly toolchain (`nightly-2025-08-20`). The daemon handles this automatically using `rustup run`, but the toolchain must be installed.