# 🔮 Bronzite

**Compile-time type reflection for Rust** ✨

[![Crates.io](https://img.shields.io/crates/v/bronzite.svg)](https://crates.io/crates/bronzite)
[![Documentation](https://docs.rs/bronzite/badge.svg)](https://docs.rs/bronzite)
[![License](https://img.shields.io/crates/l/bronzite.svg)](LICENSE)

> 🪄 Ever wished you could inspect types, traits, and method bodies at compile time? Now you can!

Bronzite lets your proc-macros see *everything* about your types - trait implementations, field names, method signatures, even the source code of method bodies. All at compile time. 🚀

## 🌟 Features

- 🔍 **Discover trait implementations** - Find out what traits a type implements
- 📋 **Inspect struct fields** - Get field names, types, and visibility
- 🔧 **Examine methods** - See method signatures and even their bodies
- 🔗 **Resolve type aliases** - Follow the chain to the underlying type
- ⚡ **Fast & cached** - A background daemon caches compilation results
- 🤝 **Proc-macro friendly** - Designed to be used from your own macros

## 📦 Installation

```sh
# Install the daemon and tools
cargo install bronzite

# Make sure you have the required nightly toolchain
rustup toolchain install nightly-2025-08-20
```

## 🚀 Quick Start

### Using in Your Proc-Macro

```rust
use bronzite_client::{connect_or_start, BronziteClient};

#[proc_macro]
pub fn my_reflection_macro(input: TokenStream) -> TokenStream {
    // 🔌 Connect to daemon (auto-starts if needed!)
    let mut client = connect_or_start(None).unwrap();
    
    // 🔍 Query trait implementations
    let impls = client.get_trait_impls("my_crate", "MyStruct").unwrap();
    
    // 🏗️ Generate code based on what you discovered
    let trait_names: Vec<_> = impls.iter()
        .map(|i| &i.trait_path)
        .collect();
    
    quote! {
        const TRAITS: &[&str] = &[#(#trait_names),*];
    }.into()
}
```

### Using the Built-in Macros

```rust
use bronzite::{bronzite_trait_names, bronzite_field_names};

// 🎯 Get all traits implemented by a type
const USER_TRAITS: &[&str] = bronzite_trait_names!("my_crate", "User");

// 📝 Get all field names of a struct  
const USER_FIELDS: &[&str] = bronzite_field_names!("my_crate", "User");
```

## 🏗️ Architecture

```
┌─────────────────┐     ┌──────────────────┐     ┌─────────────────┐
│  Your Proc-     │────▶│  bronzite-daemon │────▶│  bronzite-query │
│  Macro          │     │  (cached)        │     │  (rustc plugin) │
└─────────────────┘     └──────────────────┘     └─────────────────┘
        │                       │                        │
        │    Unix Socket        │    Compiles &          │
        │    IPC 🔌             │    Extracts 📊         │
        ▼                       ▼                        ▼
   TokenStream            Type Info Cache         rustc Internals
```

1. 🔌 **bronzite-client** - Your proc-macro connects to the daemon
2. 🏠 **bronzite-daemon** - Background service that caches compilation
3. 🔬 **bronzite-query** - Rustc plugin that extracts type information
4. 📦 **bronzite-types** - Shared protocol types

## 📚 Available Queries

| Query | Description |
|-------|-------------|
| `get_trait_impls` | 🔗 Get all trait implementations for a type |
| `get_inherent_impls` | 🔧 Get inherent impl blocks (`impl Foo { ... }`) |
| `get_fields` | 📋 Get struct/enum fields |
| `get_type` | 📖 Get detailed type information |
| `get_traits` | 📚 List all traits in a crate |
| `get_trait` | 🔍 Get detailed trait information |
| `check_impl` | ✅ Check if a type implements a trait |
| `resolve_alias` | 🔗 Resolve type aliases |
| `find_types` | 🔎 Find types matching a pattern |
| `get_layout` | 📐 Get memory layout information |

## 🎮 Example

Check out the `examples/` directory for a complete working example:

```sh
cd examples

# Start the daemon pointing at the types crate
../target/release/bronzite-daemon --manifest-path my-types &

# Run the example app
cd my-app && cargo run
```

Output:
```
=== Bronzite Compile-Time Reflection Demo ===

Traits implemented by User (discovered at compile time):
  - Debug
  - Clone
  - Serialize
  - HasId

Methods on User (discovered at compile time):
  - new()
  - deactivate()
  - is_active()

=== Demo Complete ===
```

## ⚙️ How It Works

1. 🚀 When your proc-macro runs, it calls `connect_or_start()`
2. 🏠 This ensures a bronzite-daemon is running (starts one if needed)
3. 📬 Your query is sent to the daemon over a Unix socket
4. 🔨 The daemon compiles your target crate using a special rustc plugin
5. 📊 Type information is extracted and cached
6. 📨 Results are returned to your proc-macro
7. ✨ You generate code based on the reflection data!

## 🔧 Requirements

- **Rust nightly-2025-08-20** - Required for the rustc plugin (the daemon handles this automatically)
- **Unix-like OS** - Uses Unix sockets for IPC (macOS, Linux)

## 🤔 Why "Bronzite"?

Bronzite is a mineral known for its reflective, bronze-like sheen. Just like how bronzite reflects light, this crate reflects your types! 🪨✨

## 📄 License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## 🤝 Contributing

Contributions are welcome! Feel free to:

- 🐛 Report bugs
- 💡 Suggest features
- 🔧 Submit PRs

---

Made with 💜 and a lot of ☕