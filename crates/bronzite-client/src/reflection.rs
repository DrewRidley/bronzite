//! High-level reflection API for Bronzite.
//!
//! This module provides an ergonomic, type-safe API for compile-time reflection.
//! Types hold references to the client and can navigate relationships fluently.
//!
//! # Example
//!
//! ```ignore
//! use bronzite_client::Crate;
//!
//! let krate = Crate::reflect("my_crate")?;
//!
//! // Query items with patterns
//! let items = krate.items("bevy::prelude::*")?;
//!
//! // Get a specific struct and explore it
//! let user = krate.get_struct("User")?;
//! for field in user.fields()? {
//!     println!("{}: {} (size: {})",
//!         field.name,
//!         field.ty,
//!         field.size.unwrap_or(0)
//!     );
//! }
//!
//! // Check trait implementations
//! if user.implements("Debug")? {
//!     println!("User implements Debug");
//! }
//! ```

use crate::{BronziteClient, Error, Result};
use bronzite_types::{
    AssocConstInfo, AssocTypeInfo, FieldInfo as RawFieldInfo, FunctionSignature, GenericParam,
    LayoutInfo, MethodDetails as RawMethodDetails, TraitDetails as RawTraitDetails,
    TraitImplDetails as RawTraitImpl, TypeDetails, TypeSummary, Visibility,
};
use std::sync::Arc;

// ============================================================================
// Core Reflection Entry Point
// ============================================================================

/// A reflected crate - the main entry point for type reflection.
pub struct Crate {
    name: String,
    client: Arc<BronziteClient>,
}

impl Crate {
    /// Reflect on a crate by name.
    ///
    /// This will connect to the daemon (starting it if needed) and return
    /// a handle for querying types in the specified crate.
    pub fn reflect(crate_name: impl Into<String>) -> Result<Self> {
        crate::ensure_daemon_running(None)?;
        let client = crate::connect()?;
        Ok(Self {
            name: crate_name.into(),
            client: Arc::new(client),
        })
    }

    /// Get the crate name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get all items matching a pattern.
    ///
    /// Supports:
    /// - Exact: `"foo::Bar"`
    /// - Wildcard: `"foo::Bar*"`
    /// - Single-level glob: `"foo::*"` (matches `foo::Bar` but not `foo::bar::Baz`)
    /// - Recursive glob: `"foo::**"` (matches all descendants)
    pub fn items(&self, pattern: &str) -> Result<Vec<Item>> {
        let types = self.client_mut()?.find_types(&self.name, pattern)?;

        types
            .into_iter()
            .map(|summary| Item::from_summary(summary, &self.name, Arc::clone(&self.client)))
            .collect()
    }

    /// Get all structs matching a pattern.
    pub fn structs(&self, pattern: &str) -> Result<Vec<StructDef>> {
        let items = self.items(pattern)?;
        Ok(items
            .into_iter()
            .filter_map(|item| match item {
                Item::Struct(s) => Some(s),
                _ => None,
            })
            .collect())
    }

    /// Get all enums matching a pattern.
    pub fn enums(&self, pattern: &str) -> Result<Vec<EnumDef>> {
        let items = self.items(pattern)?;
        Ok(items
            .into_iter()
            .filter_map(|item| match item {
                Item::Enum(e) => Some(e),
                _ => None,
            })
            .collect())
    }

    /// Get all traits matching a pattern.
    pub fn traits(&self, pattern: &str) -> Result<Vec<TraitDef>> {
        let all_traits = self.client_mut()?.get_traits(&self.name)?;

        let matching: Vec<_> = all_traits
            .into_iter()
            .filter(|t| bronzite_types::path_matches_pattern(&t.path, pattern))
            .collect();

        matching
            .into_iter()
            .map(|info| TraitDef::from_info(info, &self.name, Arc::clone(&self.client)))
            .collect()
    }

    /// Get a specific struct by path.
    pub fn get_struct(&self, path: &str) -> Result<StructDef> {
        let details = self.client_mut()?.get_type(&self.name, path)?;
        StructDef::from_details(details, &self.name, Arc::clone(&self.client))
    }

    /// Get a specific enum by path.
    pub fn get_enum(&self, path: &str) -> Result<EnumDef> {
        let details = self.client_mut()?.get_type(&self.name, path)?;
        EnumDef::from_details(details, &self.name, Arc::clone(&self.client))
    }

    /// Get a specific trait by path.
    pub fn get_trait(&self, path: &str) -> Result<TraitDef> {
        let details = self.client_mut()?.get_trait(&self.name, path)?;
        TraitDef::from_trait_details(details, &self.name, Arc::clone(&self.client))
    }

    /// Get a specific type alias by path.
    pub fn get_type_alias(&self, path: &str) -> Result<TypeAliasDef> {
        let (original, resolved, chain) = self.client_mut()?.resolve_alias(&self.name, path)?;
        Ok(TypeAliasDef {
            path: original,
            resolved_path: resolved,
            resolution_chain: chain,
            crate_name: self.name.clone(),
            client: Arc::clone(&self.client),
        })
    }

    /// Helper to get mutable client access (Arc doesn't need Mutex for single-threaded use).
    fn client_mut(&self) -> Result<&mut BronziteClient> {
        // SAFETY: This is safe in proc-macro context where we're single-threaded.
        // Arc is used for cheap cloning, not thread-safety here.
        unsafe {
            let ptr = Arc::as_ptr(&self.client) as *mut BronziteClient;
            Ok(&mut *ptr)
        }
    }
}

// ============================================================================
// Item Enum - Unified Type Representation
// ============================================================================

/// A unified representation of any Rust item (struct, enum, trait, etc).
///
/// This enum provides a type-safe way to work with different kinds of items
/// discovered through pattern matching or queries. Each variant contains
/// a specific type definition with navigation methods.
///
/// # Example
///
/// ```ignore
/// use bronzite_client::{Crate, Item};
///
/// let krate = Crate::reflect("my_crate")?;
/// for item in krate.items("*")? {
///     match item {
///         Item::Struct(s) => println!("Struct: {}", s.name),
///         Item::Enum(e) => println!("Enum: {}", e.name),
///         Item::Trait(t) => println!("Trait: {}", t.name),
///         Item::TypeAlias(a) => println!("Alias: {}", a.path),
///         Item::Union(u) => println!("Union: {}", u.name),
///     }
/// }
/// ```
#[derive(Debug, Clone)]
pub enum Item {
    /// A struct definition
    Struct(StructDef),
    /// An enum definition
    Enum(EnumDef),
    /// A trait definition
    Trait(TraitDef),
    /// A type alias
    TypeAlias(TypeAliasDef),
    /// A union definition
    Union(UnionDef),
}

impl Item {
    /// Get the name of this item.
    pub fn name(&self) -> &str {
        match self {
            Item::Struct(s) => &s.name,
            Item::Enum(e) => &e.name,
            Item::Trait(t) => &t.name,
            Item::TypeAlias(a) => &a.path,
            Item::Union(u) => &u.name,
        }
    }

    /// Get the full path of this item.
    pub fn path(&self) -> &str {
        match self {
            Item::Struct(s) => &s.path,
            Item::Enum(e) => &e.path,
            Item::Trait(t) => &t.path,
            Item::TypeAlias(a) => &a.path,
            Item::Union(u) => &u.path,
        }
    }

    fn from_summary(
        summary: TypeSummary,
        crate_name: &str,
        client: Arc<BronziteClient>,
    ) -> Result<Self> {
        match summary.kind {
            bronzite_types::TypeKind::Struct => Ok(Item::Struct(StructDef {
                name: summary.name,
                path: summary.path,
                generics: summary.generics,
                crate_name: crate_name.to_string(),
                client,
                cached_details: None,
            })),
            bronzite_types::TypeKind::Enum => Ok(Item::Enum(EnumDef {
                name: summary.name,
                path: summary.path,
                generics: summary.generics,
                crate_name: crate_name.to_string(),
                client,
                cached_details: None,
            })),
            bronzite_types::TypeKind::Union => Ok(Item::Union(UnionDef {
                name: summary.name,
                path: summary.path,
                generics: summary.generics,
                crate_name: crate_name.to_string(),
                client,
            })),
            bronzite_types::TypeKind::Trait => {
                // For traits, we need to fetch full details
                let client_mut = unsafe {
                    let ptr = Arc::as_ptr(&client) as *mut BronziteClient;
                    &mut *ptr
                };
                let details = client_mut.get_trait(crate_name, &summary.path)?;
                Ok(Item::Trait(TraitDef::from_trait_details(
                    details, crate_name, client,
                )?))
            }
            _ => Err(Error::UnexpectedResponse),
        }
    }
}

// ============================================================================
// Struct Definition
// ============================================================================

/// A reflected struct definition with navigation methods.
///
/// Provides access to struct metadata and navigation to related types like
/// fields, trait implementations, and methods.
///
/// # Example
///
/// ```ignore
/// use bronzite_client::Crate;
///
/// let krate = Crate::reflect("my_crate")?;
/// let user = krate.get_struct("User")?;
///
/// // Get fields
/// for field in user.fields()? {
///     println!("Field: {} of type {}",
///         field.name.unwrap_or_default(),
///         field.ty
///     );
/// }
///
/// // Check trait implementation
/// if user.implements("Debug")? {
///     println!("User implements Debug");
/// }
///
/// // Get methods
/// for method in user.methods()? {
///     println!("Method: {}", method.name);
/// }
/// ```
#[derive(Debug, Clone)]
pub struct StructDef {
    /// The struct's name (without path)
    pub name: String,
    /// The struct's full path
    pub path: String,
    /// Generic parameters
    pub generics: Vec<GenericParam>,
    crate_name: String,
    client: Arc<BronziteClient>,
    cached_details: Option<Box<TypeDetails>>,
}

impl StructDef {
    fn from_details(
        details: TypeDetails,
        crate_name: &str,
        client: Arc<BronziteClient>,
    ) -> Result<Self> {
        Ok(Self {
            name: details.name.clone(),
            path: details.path.clone(),
            generics: details.generics.clone(),
            crate_name: crate_name.to_string(),
            client,
            cached_details: Some(Box::new(details)),
        })
    }

    /// Get the struct's fields.
    pub fn fields(&self) -> Result<Vec<Field>> {
        let fields = self
            .client_mut()?
            .get_fields(&self.crate_name, &self.path)?;
        Ok(fields
            .into_iter()
            .map(|f| Field::from_raw(f, &self.crate_name, Arc::clone(&self.client)))
            .collect())
    }

    /// Get trait implementations for this struct.
    pub fn trait_impls(&self) -> Result<Vec<TraitImpl>> {
        let impls = self
            .client_mut()?
            .get_trait_impls(&self.crate_name, &self.path)?;
        Ok(impls
            .into_iter()
            .map(|i| TraitImpl::from_raw(i, &self.crate_name, Arc::clone(&self.client)))
            .collect())
    }

    /// Check if this struct implements a specific trait.
    pub fn implements(&self, trait_path: &str) -> Result<bool> {
        let (implements, _) =
            self.client_mut()?
                .check_impl(&self.crate_name, &self.path, trait_path)?;
        Ok(implements)
    }

    /// Get inherent methods (from `impl StructName { ... }` blocks).
    pub fn methods(&self) -> Result<Vec<Method>> {
        let impls = self
            .client_mut()?
            .get_inherent_impls(&self.crate_name, &self.path)?;
        Ok(impls
            .into_iter()
            .flat_map(|impl_block| {
                impl_block
                    .methods
                    .into_iter()
                    .map(|m| Method::from_raw(m, &self.crate_name, Arc::clone(&self.client)))
            })
            .collect())
    }

    /// Get memory layout information.
    pub fn layout(&self) -> Result<LayoutInfo> {
        self.client_mut()?.get_layout(&self.crate_name, &self.path)
    }

    /// Get the source code of this struct definition.
    pub fn source(&self) -> Option<&str> {
        self.details().and_then(|d| d.source.as_deref())
    }

    /// Get detailed type information.
    pub fn details(&self) -> Option<&TypeDetails> {
        self.cached_details.as_deref()
    }

    /// Get visibility of this struct.
    pub fn visibility(&self) -> Option<&Visibility> {
        self.details().map(|d| &d.visibility)
    }

    /// Get doc comments.
    pub fn docs(&self) -> Option<&str> {
        self.details().and_then(|d| d.docs.as_deref())
    }

    fn client_mut(&self) -> Result<&mut BronziteClient> {
        unsafe {
            let ptr = Arc::as_ptr(&self.client) as *mut BronziteClient;
            Ok(&mut *ptr)
        }
    }
}

// ============================================================================
// Enum Definition
// ============================================================================

/// A reflected enum definition.
#[derive(Debug, Clone)]
pub struct EnumDef {
    pub name: String,
    pub path: String,
    pub generics: Vec<GenericParam>,
    crate_name: String,
    client: Arc<BronziteClient>,
    cached_details: Option<Box<TypeDetails>>,
}

impl EnumDef {
    fn from_details(
        details: TypeDetails,
        crate_name: &str,
        client: Arc<BronziteClient>,
    ) -> Result<Self> {
        Ok(Self {
            name: details.name.clone(),
            path: details.path.clone(),
            generics: details.generics.clone(),
            crate_name: crate_name.to_string(),
            client,
            cached_details: Some(Box::new(details)),
        })
    }

    /// Get the enum's variants.
    pub fn variants(&self) -> Option<&[bronzite_types::EnumVariantInfo]> {
        self.details().and_then(|d| d.variants.as_deref())
    }

    /// Get trait implementations for this enum.
    pub fn trait_impls(&self) -> Result<Vec<TraitImpl>> {
        let impls = self
            .client_mut()?
            .get_trait_impls(&self.crate_name, &self.path)?;
        Ok(impls
            .into_iter()
            .map(|i| TraitImpl::from_raw(i, &self.crate_name, Arc::clone(&self.client)))
            .collect())
    }

    /// Check if this enum implements a specific trait.
    pub fn implements(&self, trait_path: &str) -> Result<bool> {
        let (implements, _) =
            self.client_mut()?
                .check_impl(&self.crate_name, &self.path, trait_path)?;
        Ok(implements)
    }

    /// Get inherent methods.
    pub fn methods(&self) -> Result<Vec<Method>> {
        let impls = self
            .client_mut()?
            .get_inherent_impls(&self.crate_name, &self.path)?;
        Ok(impls
            .into_iter()
            .flat_map(|impl_block| {
                impl_block
                    .methods
                    .into_iter()
                    .map(|m| Method::from_raw(m, &self.crate_name, Arc::clone(&self.client)))
            })
            .collect())
    }

    /// Get the source code of this enum definition.
    pub fn source(&self) -> Option<&str> {
        self.details().and_then(|d| d.source.as_deref())
    }

    pub fn details(&self) -> Option<&TypeDetails> {
        self.cached_details.as_deref()
    }

    pub fn visibility(&self) -> Option<&Visibility> {
        self.details().map(|d| &d.visibility)
    }

    pub fn docs(&self) -> Option<&str> {
        self.details().and_then(|d| d.docs.as_deref())
    }

    fn client_mut(&self) -> Result<&mut BronziteClient> {
        unsafe {
            let ptr = Arc::as_ptr(&self.client) as *mut BronziteClient;
            Ok(&mut *ptr)
        }
    }
}

// ============================================================================
// Union Definition
// ============================================================================

/// A reflected union definition.
#[derive(Debug, Clone)]
pub struct UnionDef {
    pub name: String,
    pub path: String,
    pub generics: Vec<GenericParam>,
    crate_name: String,
    client: Arc<BronziteClient>,
}

impl UnionDef {
    /// Get the union's fields.
    pub fn fields(&self) -> Result<Vec<Field>> {
        let fields = self
            .client_mut()?
            .get_fields(&self.crate_name, &self.path)?;
        Ok(fields
            .into_iter()
            .map(|f| Field::from_raw(f, &self.crate_name, Arc::clone(&self.client)))
            .collect())
    }

    fn client_mut(&self) -> Result<&mut BronziteClient> {
        unsafe {
            let ptr = Arc::as_ptr(&self.client) as *mut BronziteClient;
            Ok(&mut *ptr)
        }
    }
}

// ============================================================================
// Trait Definition
// ============================================================================

/// A reflected trait definition.
#[derive(Debug, Clone)]
pub struct TraitDef {
    pub name: String,
    pub path: String,
    pub generics: Vec<GenericParam>,
    pub is_auto: bool,
    pub is_unsafe: bool,
    pub supertraits: Vec<String>,
    pub source: Option<String>,
    pub docs: Option<String>,
    crate_name: String,
    client: Arc<BronziteClient>,
    cached_details: Option<Box<RawTraitDetails>>,
}

impl TraitDef {
    fn from_info(
        info: bronzite_types::TraitInfo,
        crate_name: &str,
        client: Arc<BronziteClient>,
    ) -> Result<Self> {
        // Fetch full details
        let client_mut = unsafe {
            let ptr = Arc::as_ptr(&client) as *mut BronziteClient;
            &mut *ptr
        };
        let details = client_mut.get_trait(crate_name, &info.path)?;
        Self::from_trait_details(details, crate_name, client)
    }

    fn from_trait_details(
        details: RawTraitDetails,
        crate_name: &str,
        client: Arc<BronziteClient>,
    ) -> Result<Self> {
        Ok(Self {
            name: details.name.clone(),
            path: details.path.clone(),
            generics: details.generics.clone(),
            is_auto: details.is_auto,
            is_unsafe: details.is_unsafe,
            supertraits: details.supertraits.clone(),
            source: details.source.clone(),
            docs: details.docs.clone(),
            crate_name: crate_name.to_string(),
            client,
            cached_details: Some(Box::new(details)),
        })
    }

    /// Get all methods defined in this trait.
    pub fn methods(&self) -> Vec<TraitMethod> {
        self.cached_details
            .as_ref()
            .map(|d| {
                d.methods
                    .iter()
                    .map(|m| TraitMethod {
                        name: m.name.clone(),
                        signature: m.signature.clone(),
                        parsed_signature: m.parsed_signature.clone(),
                        has_default: m.has_default,
                        default_body: m.default_body.clone(),
                        is_unsafe: m.is_unsafe,
                        docs: m.docs.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get associated types.
    pub fn associated_types(&self) -> Vec<&AssocTypeInfo> {
        self.cached_details
            .as_ref()
            .map(|d| d.assoc_types.iter().collect())
            .unwrap_or_default()
    }

    /// Get associated constants.
    pub fn associated_consts(&self) -> Vec<&AssocConstInfo> {
        self.cached_details
            .as_ref()
            .map(|d| d.assoc_consts.iter().collect())
            .unwrap_or_default()
    }

    /// Get all types that implement this trait.
    pub fn implementors(&self) -> Result<Vec<Item>> {
        let types = self
            .client_mut()?
            .get_implementors(&self.crate_name, &self.path)?;
        types
            .into_iter()
            .map(|summary| Item::from_summary(summary, &self.crate_name, Arc::clone(&self.client)))
            .collect()
    }

    fn client_mut(&self) -> Result<&mut BronziteClient> {
        unsafe {
            let ptr = Arc::as_ptr(&self.client) as *mut BronziteClient;
            Ok(&mut *ptr)
        }
    }
}

/// A method defined in a trait.
#[derive(Debug, Clone)]
pub struct TraitMethod {
    pub name: String,
    pub signature: String,
    pub parsed_signature: FunctionSignature,
    pub has_default: bool,
    pub default_body: Option<String>,
    pub is_unsafe: bool,
    pub docs: Option<String>,
}

// ============================================================================
// Type Alias Definition
// ============================================================================

/// A reflected type alias.
#[derive(Debug, Clone)]
pub struct TypeAliasDef {
    pub path: String,
    pub resolved_path: String,
    pub resolution_chain: Vec<String>,
    crate_name: String,
    client: Arc<BronziteClient>,
}

impl TypeAliasDef {
    /// Resolve this alias to its concrete type.
    pub fn resolve(&self) -> Result<Item> {
        // Get the final resolved type
        let details = self
            .client_mut()?
            .get_type(&self.crate_name, &self.resolved_path)?;

        let summary = TypeSummary {
            name: details.name.clone(),
            path: details.path.clone(),
            kind: details.kind.clone(),
            generics: details.generics.clone(),
        };

        Item::from_summary(summary, &self.crate_name, Arc::clone(&self.client))
    }

    fn client_mut(&self) -> Result<&mut BronziteClient> {
        unsafe {
            let ptr = Arc::as_ptr(&self.client) as *mut BronziteClient;
            Ok(&mut *ptr)
        }
    }
}

// ============================================================================
// Field
// ============================================================================

/// A field of a struct, enum variant, or union.
///
/// Provides field metadata and the ability to navigate to the field's type definition.
///
/// # Example
///
/// ```ignore
/// let user = krate.get_struct("User")?;
/// for field in user.fields()? {
///     println!("Field: {}", field.name.as_deref().unwrap_or("<unnamed>"));
///     println!("  Type: {}", field.ty);
///     println!("  Size: {:?}", field.size);
///
///     // Navigate to the field's type definition
///     if let Some(field_type) = field.type_def()? {
///         println!("  Defined in: {}", field_type.path());
///     }
/// }
/// ```
#[derive(Debug, Clone)]
pub struct Field {
    /// Field name (None for tuple struct fields)
    pub name: Option<String>,
    /// Field index in the struct
    pub index: usize,
    /// Type as a string
    pub ty: String,
    /// Resolved type (following aliases)
    pub resolved_ty: Option<String>,
    /// Field visibility
    pub visibility: Visibility,
    /// Doc comments
    pub docs: Option<String>,
    /// Offset in bytes (if layout is known)
    pub offset: Option<usize>,
    /// Size in bytes (if layout is known)
    pub size: Option<usize>,
    crate_name: String,
    client: Arc<BronziteClient>,
}

impl Field {
    fn from_raw(raw: RawFieldInfo, crate_name: &str, client: Arc<BronziteClient>) -> Self {
        Self {
            name: raw.name,
            index: raw.index,
            ty: raw.ty,
            resolved_ty: raw.resolved_ty,
            visibility: raw.visibility,
            docs: raw.docs,
            offset: raw.offset,
            size: raw.size,
            crate_name: crate_name.to_string(),
            client,
        }
    }

    /// Get the type definition for this field's type.
    pub fn type_def(&self) -> Result<Option<Item>> {
        let type_path = self.resolved_ty.as_ref().unwrap_or(&self.ty);

        // Try to get type details
        match self.client_mut()?.get_type(&self.crate_name, type_path) {
            Ok(details) => {
                let summary = TypeSummary {
                    name: details.name.clone(),
                    path: details.path.clone(),
                    kind: details.kind.clone(),
                    generics: details.generics.clone(),
                };
                Ok(Some(Item::from_summary(
                    summary,
                    &self.crate_name,
                    Arc::clone(&self.client),
                )?))
            }
            Err(_) => Ok(None), // Type might be external or primitive
        }
    }

    fn client_mut(&self) -> Result<&mut BronziteClient> {
        unsafe {
            let ptr = Arc::as_ptr(&self.client) as *mut BronziteClient;
            Ok(&mut *ptr)
        }
    }
}

// ============================================================================
// Trait Implementation
// ============================================================================

/// A trait implementation block.
#[derive(Debug, Clone)]
pub struct TraitImpl {
    pub trait_path: String,
    pub generics: Vec<GenericParam>,
    pub is_unsafe: bool,
    pub source: Option<String>,
    raw: RawTraitImpl,
    crate_name: String,
    client: Arc<BronziteClient>,
}

impl TraitImpl {
    fn from_raw(raw: RawTraitImpl, crate_name: &str, client: Arc<BronziteClient>) -> Self {
        Self {
            trait_path: raw.trait_path.clone(),
            generics: raw.generics.clone(),
            is_unsafe: raw.is_unsafe,
            source: raw.source.clone(),
            raw,
            crate_name: crate_name.to_string(),
            client,
        }
    }

    /// Get the trait definition being implemented.
    pub fn trait_def(&self) -> Result<TraitDef> {
        let details = self
            .client_mut()?
            .get_trait(&self.crate_name, &self.trait_path)?;
        TraitDef::from_trait_details(details, &self.crate_name, Arc::clone(&self.client))
    }

    /// Get methods defined in this impl block.
    pub fn methods(&self) -> Vec<Method> {
        self.raw
            .methods
            .iter()
            .map(|m| Method::from_raw(m.clone(), &self.crate_name, Arc::clone(&self.client)))
            .collect()
    }

    /// Get associated types in this impl.
    pub fn associated_types(&self) -> &[AssocTypeInfo] {
        &self.raw.assoc_types
    }

    /// Get associated constants in this impl.
    pub fn associated_consts(&self) -> &[AssocConstInfo] {
        &self.raw.assoc_consts
    }

    fn client_mut(&self) -> Result<&mut BronziteClient> {
        unsafe {
            let ptr = Arc::as_ptr(&self.client) as *mut BronziteClient;
            Ok(&mut *ptr)
        }
    }
}

// ============================================================================
// Method
// ============================================================================

/// A method (from an impl block).
///
/// Provides method metadata including signature, source code, and the ability
/// to navigate to parameter and return types.
///
/// # Example
///
/// ```ignore
/// let user = krate.get_struct("User")?;
/// for method in user.methods()? {
///     println!("Method: {}", method.name);
///     println!("  Signature: {}", method.signature);
///
///     // Get return type
///     if let Some(return_type) = method.return_type_def()? {
///         println!("  Returns: {}", return_type.name());
///     }
///
///     // Get method body source
///     if let Some(body) = &method.body_source {
///         println!("  Body: {}", body);
///     }
/// }
/// ```
#[derive(Debug, Clone)]
pub struct Method {
    /// Method name
    pub name: String,
    /// Full signature as a string
    pub signature: String,
    /// Parsed signature components
    pub parsed_signature: FunctionSignature,
    /// Method body source code (if available)
    pub body_source: Option<String>,
    /// Whether this is an unsafe method
    pub is_unsafe: bool,
    /// Whether this is a const method
    pub is_const: bool,
    /// Whether this is an async method
    pub is_async: bool,
    /// Doc comments
    pub docs: Option<String>,
    crate_name: String,
    client: Arc<BronziteClient>,
}

impl Method {
    fn from_raw(raw: RawMethodDetails, crate_name: &str, client: Arc<BronziteClient>) -> Self {
        Self {
            name: raw.name,
            signature: raw.signature,
            parsed_signature: raw.parsed_signature,
            body_source: raw.body_source,
            is_unsafe: raw.is_unsafe,
            is_const: raw.is_const,
            is_async: raw.is_async,
            docs: raw.docs,
            crate_name: crate_name.to_string(),
            client,
        }
    }

    /// Get the return type as an Item if it's a known type.
    pub fn return_type_def(&self) -> Result<Option<Item>> {
        if let Some(return_ty) = &self.parsed_signature.return_ty {
            match self.client_mut()?.get_type(&self.crate_name, return_ty) {
                Ok(details) => {
                    let summary = TypeSummary {
                        name: details.name.clone(),
                        path: details.path.clone(),
                        kind: details.kind.clone(),
                        generics: details.generics.clone(),
                    };
                    Ok(Some(Item::from_summary(
                        summary,
                        &self.crate_name,
                        Arc::clone(&self.client),
                    )?))
                }
                Err(_) => Ok(None),
            }
        } else {
            Ok(None)
        }
    }

    /// Get parameter type definitions.
    pub fn param_types(&self) -> Result<Vec<Option<Item>>> {
        self.parsed_signature
            .params
            .iter()
            .map(
                |param| match self.client_mut()?.get_type(&self.crate_name, &param.ty) {
                    Ok(details) => {
                        let summary = TypeSummary {
                            name: details.name.clone(),
                            path: details.path.clone(),
                            kind: details.kind.clone(),
                            generics: details.generics.clone(),
                        };
                        Ok(Some(Item::from_summary(
                            summary,
                            &self.crate_name,
                            Arc::clone(&self.client),
                        )?))
                    }
                    Err(_) => Ok(None),
                },
            )
            .collect()
    }

    fn client_mut(&self) -> Result<&mut BronziteClient> {
        unsafe {
            let ptr = Arc::as_ptr(&self.client) as *mut BronziteClient;
            Ok(&mut *ptr)
        }
    }
}
