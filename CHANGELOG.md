# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2025-11-28

### 🎉 Major API Redesign

This release introduces a completely redesigned high-level API focused on ergonomics, type safety, and fluent navigation between types. The old low-level API is still available for backwards compatibility scenarios, but the new API is recommended for all new code.

### Added

#### New High-Level Reflection API
- **`Crate::reflect(name)`** - New main entry point that handles daemon connection and startup
- **`Item` enum** - Unified representation of all Rust items (Struct, Enum, Trait, TypeAlias, Union)
- **Smart type wrappers** with `Arc<BronziteClient>` references for fluent navigation:
  - `StructDef` - Enhanced struct representation
  - `EnumDef` - Enhanced enum representation  
  - `TraitDef` - Enhanced trait representation
  - `TypeAliasDef` - Enhanced type alias representation
  - `UnionDef` - Union representation
  - `Field` - Field representation with navigation to type definitions
  - `Method` - Method representation with signature parsing
  - `TraitImpl` - Trait implementation representation
  - `TraitMethod` - Trait method representation

#### Pattern Matching
- **Glob patterns** for querying types:
  - Exact match: `"foo::Bar"`
  - Prefix wildcard: `"foo::Bar*"`
  - Single-level glob: `"foo::*"` (matches `foo::Bar` but not `foo::bar::Baz`)
  - Recursive glob: `"foo::**"` (matches all descendants)

#### Navigation Methods
- **`struct.fields()`** - Get fields and navigate to their types
- **`struct.methods()`** - Get inherent methods with full signatures
- **`struct.trait_impls()`** - Get trait implementations
- **`struct.implements(trait)`** - Check trait implementation
- **`field.type_def()`** - Navigate from field to its type definition
- **`method.return_type_def()`** - Navigate to method return type
- **`method.param_types()`** - Navigate to parameter types
- **`trait.implementors()`** - Get all types implementing a trait
- **Similar methods on `EnumDef`, `TraitDef`, etc.**

#### Type-Specific Queries
- **`krate.structs(pattern)`** - Get only structs matching pattern
- **`krate.enums(pattern)`** - Get only enums matching pattern
- **`krate.traits(pattern)`** - Get only traits matching pattern
- **`krate.items(pattern)`** - Get all items matching pattern

#### Source Code Access
- Most types now include `source()` method to get their source code
- Methods include `body_source` field with implementation source
- Doc comments accessible via `docs()` method

### Changed

#### Breaking Changes

1. **API Structure**
   - Old: Multiple client method calls required
   - New: Single `Crate::reflect()` creates a handle for all queries
   
   ```rust
   // Before (v0.1)
   let mut client = BronziteClient::connect()?;
   let impls = client.get_trait_impls("my_crate", "User")?;
   
   // After (v0.2)
   let krate = Crate::reflect("my_crate")?;
   let user = krate.get_struct("User")?;
   let impls = user.trait_impls()?;
   ```

2. **Navigation Model**
   - Old: String-based queries with manual client management
   - New: Type-safe navigation with automatic client handling
   
   ```rust
   // Before (v0.1)
   let fields = client.get_fields("my_crate", "User")?;
   for field in fields {
       let field_type = client.get_type("my_crate", &field.ty)?;
   }
   
   // After (v0.2)
   let user = krate.get_struct("User")?;
   for field in user.fields()? {
       if let Some(field_type) = field.type_def()? {
           // field_type is an Item enum
       }
   }
   ```

3. **Type Representation**
   - Old: Separate `TypeDetails`, `TypeSummary`, etc.
   - New: Unified `Item` enum with specific types (`StructDef`, `EnumDef`, etc.)

4. **Pattern Matching**
   - Old: `find_types()` with limited pattern support
   - New: `items()`, `structs()`, `enums()`, `traits()` with full glob support

### Improved

- **Ergonomics** - Method chaining instead of multiple client calls
- **Type Safety** - Strongly-typed wrappers instead of raw protocol types
- **Discoverability** - IDE autocomplete shows available navigation paths
- **Performance** - Single connection shared via `Arc` instead of multiple client instances
- **Error Handling** - Better error messages with context

### Deprecated

- None (old API still available but not recommended)

### Removed

- None (fully backwards compatible at the client level)

### Fixed

- N/A

### Migration Guide

See the [Migration Guide](README.md#-migration-guide-v01--v02) in the README for detailed migration instructions.

### Internal Changes

- Added `reflection` module in `bronzite-client`
- `BronziteClient` now derives `Debug`
- Unsafe pointer casting used for `Arc<BronziteClient>` mutation (safe in single-threaded proc-macro context)

---

## [0.1.0] - 2024-11-26

### Added

- Initial release of Bronzite
- Core daemon architecture with rustc plugin
- Low-level client API for proc-macros
- Basic type introspection queries:
  - `list_items` - List all items in a crate
  - `get_type` - Get type details
  - `get_trait_impls` - Get trait implementations
  - `get_inherent_impls` - Get inherent impl blocks
  - `get_fields` - Get struct fields
  - `get_traits` - List traits
  - `get_trait` - Get trait details
  - `check_impl` - Check trait implementation
  - `find_types` - Find types by pattern
  - `resolve_alias` - Resolve type aliases
  - `get_implementors` - Get types implementing a trait
  - `get_layout` - Get memory layout
- Proc-macro helpers in `bronzite-macros`
- Example proc-macros demonstrating usage
- Unix socket IPC (TCP on Windows)
- Automatic daemon startup with `ensure_daemon_running()`
- Documentation and examples

[0.2.0]: https://github.com/drewridley/bronzite/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/drewridley/bronzite/releases/tag/v0.1.0
