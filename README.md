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
- 🧭 **Navigate type relationships** - Fluently explore from types to fields to their definitions
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

### High-Level Reflection API (Recommended)

The new v0.2 API provides an ergonomic, navigation-focused interface:

```rust
use bronzite_client::Crate;

#[proc_macro]
pub fn my_reflection_macro(input: TokenStream) -> TokenStream {
    // 🔌 Reflect on a crate (auto-starts daemon if needed!)
    let krate = Crate::reflect("my_crate").unwrap();
    
    // 🔍 Query items with pattern matching
    let items = krate.items("bevy::prelude::*").unwrap();
    
    // 🏗️ Get a struct and explore it
    let user = krate.get_struct("User").unwrap();
    
    // 📋 Navigate to fields
    for field in user.fields().unwrap() {
        println!("{}: {}", field.name.unwrap(), field.ty);
        
        // 🔗 Navigate to field's type definition
        if let Some(field_type) = field.type_def().unwrap() {
            println!("  -> defined in: {}", field_type.path());
        }
    }
    
    // ✅ Check trait implementations
    if user.implements("Debug").unwrap() {
        println!("User implements Debug!");
    }
    
    // 🔧 Get methods with their signatures
    for method in user.methods().unwrap() {
        println!("Method: {} -> {:?}", 
            method.name, 
            method.parsed_signature.return_ty
        );
        
        // 📖 Even get the method body source!
        if let Some(body) = method.body_source {
            println!("Body: {}", body);
        }
    }
    
    // ... generate code based on what you discovered
    quote! { /* generated code */ }.into()
}
```

### Pattern Matching

The new API supports intuitive glob patterns:

```rust
let krate = Crate::reflect("my_crate")?;

// Exact match
let user = krate.get_struct("User")?;

// Single-level wildcard: matches "foo::Bar" but not "foo::bar::Baz"
let items = krate.items("mymod::*")?;

// Recursive wildcard: matches all descendants
let all_items = krate.items("mymod::**")?;

// Prefix matching
let items = krate.items("MyType*")?; // matches MyTypeA, MyTypeB, etc.
```

### Type-Specific Queries

```rust
let krate = Crate::reflect("my_crate")?;

// Get only structs
let structs = krate.structs("*")?;

// Get only enums
let enums = krate.enums("*")?;

// Get only traits
let traits = krate.traits("*")?;
```

### Unified Item Enum

All items are represented by a unified `Item` enum:

```rust
use bronzite_client::Item;

for item in krate.items("*")? {
    match item {
        Item::Struct(s) => {
            println!("Struct: {}", s.name);
            for field in s.fields()? {
                println!("  - {}: {}", field.name.unwrap(), field.ty);
            }
        }
        Item::Enum(e) => {
            println!("Enum: {}", e.name);
            if let Some(variants) = e.variants() {
                for variant in variants {
                    println!("  - {}", variant.name);
                }
            }
        }
        Item::Trait(t) => {
            println!("Trait: {}", t.name);
            for method in t.methods() {
                println!("  - {}", method.name);
            }
        }
        Item::TypeAlias(a) => {
            println!("Type alias: {} -> {}", a.path, a.resolved_path);
        }
        Item::Union(u) => {
            println!("Union: {}", u.name);
        }
    }
}
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

1. 🔌 **bronzite-client** - Your proc-macro uses the high-level `Crate` API
2. 🏠 **bronzite-daemon** - Background service that caches compilation
3. 🔬 **bronzite-query** - Rustc plugin that extracts type information
4. 📦 **bronzite-types** - Shared protocol types

## 📚 API Overview

### Main Entry Point

| Method | Description |
|--------|-------------|
| `Crate::reflect(name)` | 🔌 Connect to daemon and reflect on a crate |
| `krate.items(pattern)` | 📦 Get all items matching a pattern |
| `krate.structs(pattern)` | 🏗️ Get all structs |
| `krate.enums(pattern)` | 📋 Get all enums |
| `krate.traits(pattern)` | 🔗 Get all traits |
| `krate.get_struct(path)` | 🎯 Get a specific struct |
| `krate.get_enum(path)` | 🎯 Get a specific enum |
| `krate.get_trait(path)` | 🎯 Get a specific trait |

### Struct Methods

| Method | Description |
|--------|-------------|
| `struct.fields()` | 📋 Get all fields |
| `struct.methods()` | 🔧 Get inherent methods |
| `struct.trait_impls()` | 🔗 Get trait implementations |
| `struct.implements(trait)` | ✅ Check if implements a trait |
| `struct.layout()` | 📐 Get memory layout info |
| `struct.source()` | 📖 Get source code |
| `struct.docs()` | 📝 Get doc comments |

### Field Methods

| Method | Description |
|--------|-------------|
| `field.type_def()` | 🔗 Navigate to field's type definition |
| `field.name` | 📛 Field name (Option for tuple fields) |
| `field.ty` | 🏷️ Type as string |
| `field.size` | 📏 Size in bytes (if available) |
| `field.offset` | 📍 Offset in bytes (if available) |

### Method Methods

| Method | Description |
|--------|-------------|
| `method.return_type_def()` | 🔗 Navigate to return type |
| `method.param_types()` | 🔗 Navigate to parameter types |
| `method.body_source` | 📖 Method body source code |
| `method.parsed_signature` | 🔍 Parsed signature details |

### Trait Methods

| Method | Description |
|--------|-------------|
| `trait.methods()` | 🔧 Get all trait methods |
| `trait.associated_types()` | 🏷️ Get associated types |
| `trait.associated_consts()` | 🔢 Get associated constants |
| `trait.implementors()` | 📋 Get all implementing types |

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

Compile-time trait checks:
  User implements Serialize: true
  Product implements Serialize: true

=== Demo Complete ===
```

## 🔧 Requirements

- **Rust nightly-2025-08-20** - Required for the rustc plugin (the daemon handles this automatically)
- **Unix-like OS or Windows** - Uses Unix sockets on Unix, TCP on Windows

## 🔄 Migration Guide (v0.1 → v0.2)

### Breaking Changes

The v0.2 release introduces a completely redesigned API focused on ergonomics and navigation. The low-level client methods are still available, but the new high-level API is recommended.

#### Before (v0.1)

```rust
use bronzite_client::{BronziteClient, ensure_daemon_running};

ensure_daemon_running()?;
let mut client = BronziteClient::connect()?;

let impls = client.get_trait_impls("my_crate", "User")?;
let fields = client.get_fields("my_crate", "User")?;
let (implements, _) = client.check_impl("my_crate", "User", "Debug")?;
```

#### After (v0.2)

```rust
use bronzite_client::Crate;

let krate = Crate::reflect("my_crate")?;
let user = krate.get_struct("User")?;

let impls = user.trait_impls()?;
let fields = user.fields()?;
let implements = user.implements("Debug")?;
```

### Key Improvements

1. **Single connection** - `Crate::reflect()` handles daemon startup and connection
2. **Navigation** - Types hold references to the client, enabling fluent navigation
3. **Type-safe** - Unified `Item` enum instead of string-based queries
4. **Pattern matching** - Intuitive glob patterns for querying types
5. **Source code** - Most types now include their source code
6. **Ergonomic** - Chaining methods instead of multiple client calls

### Low-Level API Still Available

If you need the low-level API, it's still available:

```rust
use bronzite_client::BronziteClient;

let mut client = BronziteClient::connect()?;
let items = client.list_items("my_crate")?;
```

But we recommend the new high-level API for most use cases.

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
