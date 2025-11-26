#![feature(rustc_private)]

extern crate rustc_abi;
extern crate rustc_ast;
extern crate rustc_driver;
extern crate rustc_hir;
extern crate rustc_infer;
extern crate rustc_interface;
extern crate rustc_middle;
extern crate rustc_session;
extern crate rustc_span;
extern crate rustc_target;
extern crate rustc_trait_selection;

use rustc_infer::infer::TyCtxtInferExt;
use rustc_trait_selection::infer::InferCtxtExt;

use std::collections::HashMap;

use bronzite_types::{
    AssocConstInfo, AssocTypeInfo, CrateTypeInfo, Delimiter, EnumVariantInfo, FieldInfo,
    FunctionSignature, GenericParam, GenericParamKind, InherentImplDetails, ItemInfo, ItemKind,
    LayoutInfo, LiteralKind, MatchArm, MethodDetails, MethodSummary, ModuleInfo, ParamInfo, Query,
    QueryData, QueryResult, ReceiverInfo, ReexportInfo, SpanInfo, Token, TraitDetails,
    TraitImplDetails, TraitInfo, TraitMethodInfo, TypeAliasInfo, TypeDetails, TypeKind,
    TypeSummary, Visibility,
};
use clap::Parser;
use rustc_ast::ast;
use rustc_hir as hir;
use rustc_hir::def::DefKind;
use rustc_hir::def_id::{DefId, LOCAL_CRATE, LocalDefId};
use rustc_middle::ty::{self, TyCtxt, TypingEnv};
use rustc_span::symbol::sym;
use serde::{Deserialize, Serialize};

// Re-export key types for external users
pub use bronzite_types::CrateTypeInfo as CrateTypeInfoExport;

/// CLI arguments for bronzite-query
#[derive(Parser, Debug, Clone, Serialize, Deserialize)]
pub struct Args {
    /// Query to execute (for single-query mode)
    #[arg(short, long)]
    pub query: Option<String>,

    /// Extract all type information to a JSON file (for daemon mode)
    #[arg(long)]
    pub extract: bool,

    /// Output file for extraction (defaults to stdout)
    #[arg(long)]
    pub output: Option<String>,
}

/// The Bronzite query plugin
pub struct BronziteQueryPlugin;

impl rustc_plugin::RustcPlugin for BronziteQueryPlugin {
    type Args = Args;

    fn version(&self) -> std::borrow::Cow<'static, str> {
        "0.1.0".into()
    }

    fn driver_name(&self) -> std::borrow::Cow<'static, str> {
        "bronzite-query-driver".into()
    }

    fn args(
        &self,
        _target_dir: &rustc_plugin::Utf8Path,
    ) -> rustc_plugin::RustcPluginArgs<Self::Args> {
        let args = Args::parse_from(std::env::args().skip(1));
        rustc_plugin::RustcPluginArgs {
            args,
            filter: rustc_plugin::CrateFilter::OnlyWorkspace,
        }
    }

    fn run(
        self,
        compiler_args: Vec<String>,
        plugin_args: Self::Args,
    ) -> rustc_interface::interface::Result<()> {
        let mut callbacks = BronziteQueryCallbacks { args: plugin_args };
        rustc_driver::run_compiler(&compiler_args, &mut callbacks);
        Ok(())
    }
}

pub struct BronziteQueryCallbacks {
    args: Args,
}

impl rustc_driver::Callbacks for BronziteQueryCallbacks {
    fn after_analysis(
        &mut self,
        _compiler: &rustc_interface::interface::Compiler,
        tcx: TyCtxt<'_>,
    ) -> rustc_driver::Compilation {
        if self.args.extract {
            let info = extract_crate_info(tcx);
            output_extracted_info(&info, &self.args.output);
        } else if let Some(ref query_str) = self.args.query {
            let query = parse_query(query_str);
            let result = execute_query(tcx, &query);
            output_query_result(&result);
        }

        rustc_driver::Compilation::Stop
    }
}

// ============================================================================
// Source Code Extraction Helpers
// ============================================================================

/// Get the source code for a span
fn get_source_for_span(tcx: TyCtxt<'_>, span: rustc_span::Span) -> Option<String> {
    let source_map = tcx.sess.source_map();
    source_map.span_to_snippet(span).ok()
}

/// Get the source code for a definition
fn get_source_for_def(tcx: TyCtxt<'_>, def_id: DefId) -> Option<String> {
    if !def_id.is_local() {
        return None;
    }
    let span = tcx.source_span(def_id.expect_local());
    get_source_for_span(tcx, span)
}

// ============================================================================
// Doc Comments Extraction
// ============================================================================

/// Extract doc comments from attributes
fn extract_docs(tcx: TyCtxt<'_>, def_id: DefId) -> Option<String> {
    if !def_id.is_local() {
        return None;
    }

    // Use hir_attrs to get attributes
    let local_def_id = def_id.expect_local();
    let hir_id = tcx.local_def_id_to_hir_id(local_def_id);
    let attrs = tcx.hir_attrs(hir_id);
    let mut docs = Vec::new();

    for attr in attrs {
        if attr.is_doc_comment() {
            if let Some(doc) = attr.doc_str() {
                docs.push(doc.to_string());
            }
        } else if attr.has_name(sym::doc) {
            if let Some(value) = attr.value_str() {
                docs.push(value.to_string());
            }
        }
    }

    if docs.is_empty() {
        None
    } else {
        Some(docs.join("\n"))
    }
}

// ============================================================================
// Attributes Extraction
// ============================================================================

/// Extract non-doc attributes as strings
fn extract_attributes(tcx: TyCtxt<'_>, def_id: DefId) -> Vec<String> {
    if !def_id.is_local() {
        return Vec::new();
    }

    let local_def_id = def_id.expect_local();
    let hir_id = tcx.local_def_id_to_hir_id(local_def_id);
    let attrs = tcx.hir_attrs(hir_id);

    attrs
        .iter()
        .filter(|attr| !attr.is_doc_comment() && !attr.has_name(sym::doc))
        .filter_map(|attr| {
            // Reconstruct attribute from path symbols to avoid panics on parsed attributes
            // (attr.span() can panic for inline/parsed attributes in newer rustc versions)
            let path_symbols = attr.path();
            if path_symbols.is_empty() {
                None
            } else {
                let path_str: Vec<_> = path_symbols.iter().map(|s| s.to_string()).collect();
                Some(format!("#[{}]", path_str.join("::")))
            }
        })
        .collect()
}

// ============================================================================
// Token/AST Extraction
// ============================================================================

/// Extract body tokens from a function body
fn extract_body_tokens(tcx: TyCtxt<'_>, def_id: DefId) -> Option<Vec<Token>> {
    if !def_id.is_local() {
        return None;
    }

    let local_def_id = def_id.expect_local();

    // Get the HIR body - hir_maybe_body_owned_by returns Option<&Body> directly
    let body = tcx.hir_maybe_body_owned_by(local_def_id)?;
    let expr = &body.value;

    Some(vec![extract_expr_tokens(tcx, expr)])
}

/// Convert a HIR expression to tokens
fn extract_expr_tokens(tcx: TyCtxt<'_>, expr: &hir::Expr<'_>) -> Token {
    let source_map = tcx.sess.source_map();

    match &expr.kind {
        hir::ExprKind::Lit(lit) => {
            let kind = match lit.node {
                ast::LitKind::Str(_, _) => LiteralKind::String,
                ast::LitKind::ByteStr(_, _) => LiteralKind::ByteString,
                ast::LitKind::CStr(_, _) => LiteralKind::String,
                ast::LitKind::Byte(_) => LiteralKind::Byte,
                ast::LitKind::Char(_) => LiteralKind::Char,
                ast::LitKind::Int(_, _) => LiteralKind::Int,
                ast::LitKind::Float(_, _) => LiteralKind::Float,
                ast::LitKind::Bool(_) => LiteralKind::Bool,
                ast::LitKind::Err(_) => LiteralKind::String,
            };
            let value = source_map.span_to_snippet(expr.span).unwrap_or_default();
            Token::Literal { kind, value }
        }

        hir::ExprKind::Path(qpath) => {
            let segments: Vec<String> = match qpath {
                hir::QPath::Resolved(_, path) => {
                    path.segments.iter().map(|s| s.ident.to_string()).collect()
                }
                hir::QPath::TypeRelative(_, segment) => vec![segment.ident.to_string()],
                hir::QPath::LangItem(item, _) => vec![format!("{:?}", item)],
            };
            if segments.len() == 1 {
                Token::Ident {
                    name: segments[0].clone(),
                }
            } else {
                Token::Path { segments }
            }
        }

        hir::ExprKind::Call(func, args) => {
            let func_token = extract_expr_tokens(tcx, func);
            let path = match &func_token {
                Token::Path { segments } => segments.clone(),
                Token::Ident { name } => vec![name.clone()],
                _ => vec!["<expr>".to_string()],
            };
            let arg_tokens: Vec<Token> = args.iter().map(|a| extract_expr_tokens(tcx, a)).collect();
            Token::FnCall {
                path,
                args: arg_tokens,
            }
        }

        hir::ExprKind::MethodCall(segment, receiver, args, _) => {
            let receiver_token = Box::new(extract_expr_tokens(tcx, receiver));
            let arg_tokens: Vec<Token> = args.iter().map(|a| extract_expr_tokens(tcx, a)).collect();
            Token::MethodCall {
                receiver: receiver_token,
                method: segment.ident.to_string(),
                args: arg_tokens,
            }
        }

        hir::ExprKind::Field(base, field) => Token::FieldAccess {
            base: Box::new(extract_expr_tokens(tcx, base)),
            field: field.to_string(),
        },

        hir::ExprKind::Binary(op, lhs, rhs) => Token::BinOp {
            lhs: Box::new(extract_expr_tokens(tcx, lhs)),
            op: format!("{:?}", op.node),
            rhs: Box::new(extract_expr_tokens(tcx, rhs)),
        },

        hir::ExprKind::Unary(op, expr) => Token::UnaryOp {
            op: format!("{:?}", op),
            expr: Box::new(extract_expr_tokens(tcx, expr)),
        },

        hir::ExprKind::If(cond, then_branch, else_branch) => Token::If {
            cond: Box::new(extract_expr_tokens(tcx, cond)),
            then_branch: vec![extract_expr_tokens(tcx, then_branch)],
            else_branch: else_branch.map(|e| vec![extract_expr_tokens(tcx, e)]),
        },

        hir::ExprKind::Match(expr, arms, _) => {
            let match_arms: Vec<MatchArm> = arms
                .iter()
                .map(|arm| {
                    let pattern = source_map
                        .span_to_snippet(arm.pat.span)
                        .unwrap_or_else(|_| "<pattern>".to_string());
                    let guard = arm.guard.map(|g| {
                        source_map
                            .span_to_snippet(g.span)
                            .unwrap_or_else(|_| "<guard>".to_string())
                    });
                    let body = vec![extract_expr_tokens(tcx, arm.body)];
                    MatchArm {
                        pattern,
                        guard,
                        body,
                    }
                })
                .collect();
            Token::Match {
                expr: Box::new(extract_expr_tokens(tcx, expr)),
                arms: match_arms,
            }
        }

        hir::ExprKind::Block(block, _) => {
            let stmts: Vec<Token> = block
                .stmts
                .iter()
                .map(|stmt| extract_stmt_tokens(tcx, stmt))
                .collect();
            let mut all_tokens = stmts;
            if let Some(expr) = block.expr {
                all_tokens.push(extract_expr_tokens(tcx, expr));
            }
            Token::Block { stmts: all_tokens }
        }

        hir::ExprKind::Ret(expr) => Token::Return {
            expr: expr.map(|e| Box::new(extract_expr_tokens(tcx, e))),
        },

        hir::ExprKind::Closure(closure) => {
            let body_id = closure.body;
            let body = tcx.hir_body(body_id);
            let params: Vec<String> = body
                .params
                .iter()
                .map(|p| source_map.span_to_snippet(p.pat.span).unwrap_or_default())
                .collect();
            Token::Closure {
                params,
                body: Box::new(extract_expr_tokens(tcx, &body.value)),
            }
        }

        hir::ExprKind::Tup(exprs) => Token::Group {
            delimiter: Delimiter::Paren,
            tokens: exprs.iter().map(|e| extract_expr_tokens(tcx, e)).collect(),
        },

        hir::ExprKind::Array(exprs) => Token::Group {
            delimiter: Delimiter::Bracket,
            tokens: exprs.iter().map(|e| extract_expr_tokens(tcx, e)).collect(),
        },

        hir::ExprKind::Struct(_, fields, base) => {
            let mut tokens: Vec<Token> = fields
                .iter()
                .map(|f| {
                    let field_name = f.ident.to_string();
                    let field_value = extract_expr_tokens(tcx, f.expr);
                    Token::BinOp {
                        lhs: Box::new(Token::Ident { name: field_name }),
                        op: ":".to_string(),
                        rhs: Box::new(field_value),
                    }
                })
                .collect();
            if let hir::StructTailExpr::Base(base_expr) = base {
                tokens.push(Token::UnaryOp {
                    op: "..".to_string(),
                    expr: Box::new(extract_expr_tokens(tcx, base_expr)),
                });
            }
            Token::Group {
                delimiter: Delimiter::Brace,
                tokens,
            }
        }

        hir::ExprKind::Loop(block, label, _, _) => {
            let label_str = label.map(|l| l.ident.to_string());
            let body = extract_expr_tokens(
                tcx,
                &hir::Expr {
                    hir_id: block.hir_id,
                    kind: hir::ExprKind::Block(block, None),
                    span: block.span,
                },
            );
            Token::Block {
                stmts: vec![
                    Token::Keyword {
                        name: if let Some(l) = label_str {
                            format!("'{}: loop", l)
                        } else {
                            "loop".to_string()
                        },
                    },
                    body,
                ],
            }
        }

        // For complex expressions, fall back to raw source
        _ => {
            let source = source_map
                .span_to_snippet(expr.span)
                .unwrap_or_else(|_| "<expr>".to_string());
            Token::Raw { source }
        }
    }
}

/// Convert a HIR statement to tokens
fn extract_stmt_tokens(tcx: TyCtxt<'_>, stmt: &hir::Stmt<'_>) -> Token {
    let source_map = tcx.sess.source_map();

    match &stmt.kind {
        hir::StmtKind::Let(local) => {
            let pattern = source_map
                .span_to_snippet(local.pat.span)
                .unwrap_or_else(|_| "<pattern>".to_string());
            let ty = local.ty.map(|t| {
                source_map
                    .span_to_snippet(t.span)
                    .unwrap_or_else(|_| "<type>".to_string())
            });
            let init = local.init.map(|e| Box::new(extract_expr_tokens(tcx, e)));
            Token::Let { pattern, ty, init }
        }
        hir::StmtKind::Item(_) => Token::Raw {
            source: source_map
                .span_to_snippet(stmt.span)
                .unwrap_or_else(|_| "<item>".to_string()),
        },
        hir::StmtKind::Expr(expr) | hir::StmtKind::Semi(expr) => extract_expr_tokens(tcx, expr),
    }
}

// ============================================================================
// Type Resolution
// ============================================================================

/// Resolve a type alias chain to its final type
fn resolve_type_alias_chain(tcx: TyCtxt<'_>, def_id: DefId) -> (String, Vec<String>) {
    let mut chain = Vec::new();
    let mut current_def_id = def_id;
    let mut seen = std::collections::HashSet::new();

    loop {
        if seen.contains(&current_def_id) {
            break; // Avoid infinite loops
        }
        seen.insert(current_def_id);

        let ty = tcx.type_of(current_def_id).skip_binder();
        let ty_str = format!("{:?}", ty);
        chain.push(ty_str.clone());

        // Check if this type is itself an alias
        if let ty::TyKind::Alias(ty::Projection | ty::Opaque, alias_ty) = ty.kind() {
            current_def_id = alias_ty.def_id;
        } else if let Some(adt) = ty.ty_adt_def() {
            // If it's an ADT, we've reached the end
            chain.push(tcx.def_path_str(adt.did()));
            break;
        } else {
            break;
        }
    }

    let resolved = chain.last().cloned().unwrap_or_default();
    (resolved, chain)
}

/// Get fully resolved type string
fn get_resolved_type<'tcx>(tcx: TyCtxt<'tcx>, ty: ty::Ty<'tcx>) -> String {
    // Attempt to normalize the type
    let typing_env = TypingEnv::fully_monomorphized();

    match tcx.try_normalize_erasing_regions(typing_env, ty) {
        Ok(normalized) => format!("{:?}", normalized),
        Err(_) => format!("{:?}", ty),
    }
}

// ============================================================================
// Main Extraction Logic
// ============================================================================

/// Extract all type information from the crate
pub fn extract_crate_info(tcx: TyCtxt<'_>) -> CrateTypeInfo {
    let crate_name = tcx.crate_name(LOCAL_CRATE).to_string();

    let mut info = CrateTypeInfo {
        crate_name,
        crate_version: None,
        items: Vec::new(),
        types: HashMap::new(),
        traits: HashMap::new(),
        trait_impls: HashMap::new(),
        inherent_impls: HashMap::new(),
        type_aliases: HashMap::new(),
        layouts: HashMap::new(),
        modules: HashMap::new(),
    };

    let crate_items = tcx.hir_crate_items(());

    // First pass: collect all items
    for item_id in crate_items.free_items() {
        let def_id = item_id.owner_id.to_def_id();
        let local_def_id = item_id.owner_id.def_id;
        let def_kind = tcx.def_kind(def_id);
        let path = tcx.def_path_str(def_id);

        // Extract basic item info
        if let Some(item_info) = extract_item_info(tcx, def_id) {
            info.items.push(item_info);
        }

        // Extract detailed information based on kind
        match def_kind {
            DefKind::Struct | DefKind::Enum | DefKind::Union => {
                if let Some(type_details) = extract_type_details(tcx, local_def_id) {
                    if let Some(layout) = extract_layout_info(tcx, local_def_id) {
                        info.layouts.insert(path.clone(), layout);
                    }
                    info.types.insert(path.clone(), type_details);
                }
            }
            DefKind::Trait => {
                if let Some(trait_details) = extract_trait_details(tcx, def_id) {
                    info.traits.insert(path.clone(), trait_details);
                }
            }
            DefKind::TyAlias => {
                if let Some(alias_info) = extract_type_alias(tcx, def_id) {
                    info.type_aliases.insert(path.clone(), alias_info);
                }
            }
            DefKind::Mod => {
                if let Some(module_info) = extract_module_info(tcx, local_def_id) {
                    info.modules.insert(path.clone(), module_info);
                }
            }
            DefKind::Impl { .. } => {
                if let Some(trait_ref) = tcx.impl_trait_ref(def_id) {
                    // This is a trait impl
                    if let Some(impl_details) = extract_trait_impl_details(tcx, def_id) {
                        let self_ty = trait_ref.skip_binder().self_ty();
                        let self_ty_str = get_type_path_string(tcx, self_ty);
                        info.trait_impls
                            .entry(self_ty_str)
                            .or_default()
                            .push(impl_details);
                    }
                } else {
                    // This is an inherent impl
                    if let Some(impl_details) = extract_inherent_impl_details(tcx, def_id) {
                        let self_ty = tcx.type_of(def_id).skip_binder();
                        let self_ty_str = get_type_path_string(tcx, self_ty);
                        info.inherent_impls
                            .entry(self_ty_str)
                            .or_default()
                            .push(impl_details);
                    }
                }
            }
            _ => {}
        }
    }

    info
}

/// Get a clean path string for a type
fn get_type_path_string(tcx: TyCtxt<'_>, ty: ty::Ty<'_>) -> String {
    if let Some(adt) = ty.ty_adt_def() {
        tcx.def_path_str(adt.did())
    } else {
        format!("{:?}", ty)
    }
}

fn extract_item_info(tcx: TyCtxt<'_>, def_id: DefId) -> Option<ItemInfo> {
    let def_kind = tcx.def_kind(def_id);
    let path = tcx.def_path_str(def_id);

    let name = match def_kind {
        DefKind::Use | DefKind::Impl { .. } | DefKind::GlobalAsm | DefKind::OpaqueTy => {
            path.split("::").last().unwrap_or("").to_string()
        }
        _ => match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            tcx.item_name(def_id).to_string()
        })) {
            Ok(name) => name,
            Err(_) => path.split("::").last().unwrap_or("").to_string(),
        },
    };

    let kind = match def_kind {
        DefKind::Struct => ItemKind::Struct,
        DefKind::Enum => ItemKind::Enum,
        DefKind::Union => ItemKind::Union,
        DefKind::Trait => ItemKind::Trait,
        DefKind::Fn => ItemKind::Function,
        DefKind::Const => ItemKind::Const,
        DefKind::Static { .. } => ItemKind::Static,
        DefKind::TyAlias => ItemKind::TypeAlias,
        DefKind::Impl { .. } => ItemKind::Impl,
        DefKind::Mod => ItemKind::Mod,
        DefKind::Use => ItemKind::Use,
        DefKind::ExternCrate => ItemKind::ExternCrate,
        DefKind::Macro(_) => ItemKind::Macro,
        DefKind::TraitAlias => ItemKind::TraitAlias,
        _ => ItemKind::Other(format!("{:?}", def_kind)),
    };

    let visibility = extract_visibility(tcx, def_id);
    let span = extract_span_info(tcx, def_id);

    Some(ItemInfo {
        name,
        path,
        kind,
        visibility,
        span,
    })
}

fn extract_visibility(tcx: TyCtxt<'_>, def_id: DefId) -> Visibility {
    let vis = tcx.visibility(def_id);
    if vis.is_public() {
        Visibility::Public
    } else {
        Visibility::Private
    }
}

fn extract_span_info(tcx: TyCtxt<'_>, def_id: DefId) -> Option<SpanInfo> {
    if !def_id.is_local() {
        return None;
    }

    let span = tcx.def_span(def_id);
    let source_map = tcx.sess.source_map();

    let lo = source_map.lookup_char_pos(span.lo());
    let hi = source_map.lookup_char_pos(span.hi());

    Some(SpanInfo {
        file: lo.file.name.prefer_remapped_unconditionally().to_string(),
        start_line: lo.line as u32,
        start_col: lo.col.0 as u32,
        end_line: hi.line as u32,
        end_col: hi.col.0 as u32,
    })
}

fn extract_type_details(tcx: TyCtxt<'_>, local_def_id: LocalDefId) -> Option<TypeDetails> {
    let def_id = local_def_id.to_def_id();
    let def_kind = tcx.def_kind(def_id);

    let name = tcx.item_name(def_id).to_string();
    let path = tcx.def_path_str(def_id);
    let visibility = extract_visibility(tcx, def_id);
    let generics = extract_generics(tcx, def_id);
    let where_clause = extract_where_clause(tcx, def_id);
    let span = extract_span_info(tcx, def_id);
    let docs = extract_docs(tcx, def_id);
    let attributes = extract_attributes(tcx, def_id);
    let source = get_source_for_def(tcx, def_id);

    let kind = match def_kind {
        DefKind::Struct => TypeKind::Struct,
        DefKind::Enum => TypeKind::Enum,
        DefKind::Union => TypeKind::Union,
        _ => return None,
    };

    // Extract fields for structs/unions
    let fields = if matches!(def_kind, DefKind::Struct | DefKind::Union) {
        Some(extract_struct_fields(tcx, local_def_id))
    } else {
        None
    };

    // Extract variants for enums
    let variants = if def_kind == DefKind::Enum {
        Some(extract_enum_variants(tcx, local_def_id))
    } else {
        None
    };

    // Get trait impls for this type
    let trait_impls = get_trait_impl_paths(tcx, def_id);

    // Get inherent methods
    let inherent_methods = extract_inherent_method_summaries(tcx, def_id);

    // Try to get layout
    let layout = extract_layout_info(tcx, local_def_id);

    Some(TypeDetails {
        name,
        path,
        kind,
        visibility,
        generics,
        where_clause,
        docs,
        attributes,
        fields,
        variants,
        trait_impls,
        inherent_methods,
        layout,
        source,
        span,
    })
}

fn extract_where_clause(tcx: TyCtxt<'_>, def_id: DefId) -> Option<String> {
    if !def_id.is_local() {
        return None;
    }

    let predicates = tcx.predicates_of(def_id);
    if predicates.predicates.is_empty() {
        return None;
    }

    let preds: Vec<String> = predicates
        .predicates
        .iter()
        .map(|(pred, _)| format!("{:?}", pred))
        .collect();

    if preds.is_empty() {
        None
    } else {
        Some(format!("where {}", preds.join(", ")))
    }
}

fn extract_generics(tcx: TyCtxt<'_>, def_id: DefId) -> Vec<GenericParam> {
    let generics = tcx.generics_of(def_id);

    generics
        .own_params
        .iter()
        .filter_map(|param| {
            let name = param.name.to_string();
            if name == "Self" {
                return None;
            }

            let kind = match param.kind {
                ty::GenericParamDefKind::Lifetime => GenericParamKind::Lifetime,
                ty::GenericParamDefKind::Type { .. } => GenericParamKind::Type,
                ty::GenericParamDefKind::Const { .. } => {
                    GenericParamKind::Const { ty: String::new() }
                }
            };

            // Get bounds from predicates
            let bounds = extract_param_bounds(tcx, def_id, param.index);

            Some(GenericParam {
                name,
                kind,
                bounds,
                default: None,
            })
        })
        .collect()
}

fn extract_param_bounds(tcx: TyCtxt<'_>, def_id: DefId, param_index: u32) -> Vec<String> {
    let predicates = tcx.predicates_of(def_id);
    let mut bounds = Vec::new();

    for (pred, _) in predicates.predicates {
        if let ty::ClauseKind::Trait(trait_pred) = pred.kind().skip_binder() {
            if let ty::TyKind::Param(param_ty) = trait_pred.self_ty().kind() {
                if param_ty.index == param_index {
                    bounds.push(tcx.def_path_str(trait_pred.def_id()));
                }
            }
        }
    }

    bounds
}

fn extract_struct_fields(tcx: TyCtxt<'_>, local_def_id: LocalDefId) -> Vec<FieldInfo> {
    let def_id = local_def_id.to_def_id();
    let adt_def = tcx.adt_def(def_id);
    let variant = adt_def.non_enum_variant();

    variant
        .fields
        .iter()
        .enumerate()
        .map(|(index, field)| {
            let ty = tcx.type_of(field.did).skip_binder();
            let visibility = extract_visibility(tcx, field.did);
            let docs = extract_docs(tcx, field.did);
            let attributes = extract_attributes(tcx, field.did);

            FieldInfo {
                name: Some(field.name.to_string()),
                index,
                ty: format!("{:?}", ty),
                resolved_ty: Some(get_resolved_type(tcx, ty)),
                visibility,
                docs,
                attributes,
                offset: None, // Filled in by layout
                size: None,
                span: extract_span_info(tcx, field.did),
            }
        })
        .collect()
}

fn extract_enum_variants(tcx: TyCtxt<'_>, local_def_id: LocalDefId) -> Vec<EnumVariantInfo> {
    let def_id = local_def_id.to_def_id();
    let adt_def = tcx.adt_def(def_id);

    adt_def
        .variants()
        .iter()
        .enumerate()
        .map(|(index, variant)| {
            let docs = extract_docs(tcx, variant.def_id);
            let attributes = extract_attributes(tcx, variant.def_id);

            let fields: Vec<FieldInfo> = variant
                .fields
                .iter()
                .enumerate()
                .map(|(field_index, field)| {
                    let ty = tcx.type_of(field.did).skip_binder();
                    let visibility = extract_visibility(tcx, field.did);
                    let name = field.name.to_string();
                    let name = if name.parse::<usize>().is_ok() {
                        None
                    } else {
                        Some(name)
                    };

                    FieldInfo {
                        name,
                        index: field_index,
                        ty: format!("{:?}", ty),
                        resolved_ty: Some(get_resolved_type(tcx, ty)),
                        visibility,
                        docs: extract_docs(tcx, field.did),
                        attributes: extract_attributes(tcx, field.did),
                        offset: None,
                        size: None,
                        span: extract_span_info(tcx, field.did),
                    }
                })
                .collect();

            // Get discriminant value if explicit
            let discriminant = match variant.discr {
                ty::VariantDiscr::Explicit(def_id) => {
                    // Try to get the evaluated value
                    Some(format!("{:?}", def_id))
                }
                ty::VariantDiscr::Relative(offset) => Some(format!("{}", offset)),
            };

            EnumVariantInfo {
                name: variant.name.to_string(),
                index,
                fields,
                discriminant,
                docs,
                attributes,
                span: extract_span_info(tcx, variant.def_id),
            }
        })
        .collect()
}

fn extract_layout_info(tcx: TyCtxt<'_>, local_def_id: LocalDefId) -> Option<LayoutInfo> {
    let def_id = local_def_id.to_def_id();
    let ty = tcx.type_of(def_id).skip_binder();

    let typing_env = TypingEnv::fully_monomorphized();
    let layout = tcx.layout_of(typing_env.as_query_input(ty)).ok()?;

    let size = layout.size.bytes() as usize;
    let align = layout.align.abi.bytes() as usize;

    let is_copy = tcx.type_is_copy_modulo_regions(typing_env, ty);
    let is_sized = ty.is_sized(tcx, typing_env);

    // Check for Send and Sync
    let is_send = check_trait_impl(tcx, ty, "core::marker::Send");
    let is_sync = check_trait_impl(tcx, ty, "core::marker::Sync");

    Some(LayoutInfo {
        size,
        align,
        field_offsets: None,
        variants: None,
        is_sized,
        is_copy,
        is_send,
        is_sync,
    })
}

fn check_trait_impl<'tcx>(tcx: TyCtxt<'tcx>, ty: ty::Ty<'tcx>, trait_path: &str) -> bool {
    // Look for the trait in known lang items
    let trait_def_id = match trait_path {
        "core::marker::Send" => tcx.get_diagnostic_item(rustc_span::sym::Send),
        "core::marker::Sync" => tcx.get_diagnostic_item(rustc_span::sym::Sync),
        _ => None,
    };

    if let Some(def_id) = trait_def_id {
        let infcx = tcx.infer_ctxt().build(ty::TypingMode::non_body_analysis());
        let param_env = ty::ParamEnv::empty();
        return infcx
            .type_implements_trait(def_id, [ty], param_env)
            .must_apply_modulo_regions();
    }

    false
}

fn get_trait_impl_paths(tcx: TyCtxt<'_>, type_def_id: DefId) -> Vec<String> {
    let mut trait_paths = Vec::new();
    let crate_items = tcx.hir_crate_items(());

    for item_id in crate_items.free_items() {
        let impl_def_id = item_id.owner_id.to_def_id();
        if !matches!(tcx.def_kind(impl_def_id), DefKind::Impl { .. }) {
            continue;
        }

        if let Some(trait_ref) = tcx.impl_trait_ref(impl_def_id) {
            let impl_self_ty = trait_ref.skip_binder().self_ty();
            let matches = match impl_self_ty.ty_adt_def() {
                Some(adt) => adt.did() == type_def_id,
                None => false,
            };

            if matches {
                let trait_path = tcx.def_path_str(trait_ref.skip_binder().def_id);
                trait_paths.push(trait_path);
            }
        }
    }

    trait_paths
}

fn extract_inherent_method_summaries(tcx: TyCtxt<'_>, def_id: DefId) -> Vec<MethodSummary> {
    let mut methods = Vec::new();

    for impl_def_id in tcx.inherent_impls(def_id).iter() {
        for &item_def_id in tcx.associated_item_def_ids(impl_def_id) {
            let item = tcx.associated_item(item_def_id);
            if matches!(item.kind, ty::AssocKind::Fn { .. }) {
                let sig = tcx.fn_sig(item_def_id).skip_binder();
                methods.push(MethodSummary {
                    name: item.name().to_string(),
                    path: tcx.def_path_str(item_def_id),
                    signature: format!("{:?}", sig),
                    is_unsafe: sig.safety().is_unsafe(),
                    is_const: tcx.is_const_fn(item_def_id),
                    is_async: tcx.asyncness(item_def_id).is_async(),
                });
            }
        }
    }

    methods
}

fn extract_trait_details(tcx: TyCtxt<'_>, trait_def_id: DefId) -> Option<TraitDetails> {
    let name = tcx.item_name(trait_def_id).to_string();
    let path = tcx.def_path_str(trait_def_id);
    let visibility = extract_visibility(tcx, trait_def_id);
    let generics = extract_generics(tcx, trait_def_id);
    let where_clause = extract_where_clause(tcx, trait_def_id);
    let span = extract_span_info(tcx, trait_def_id);
    let docs = extract_docs(tcx, trait_def_id);
    let attributes = extract_attributes(tcx, trait_def_id);
    let source = get_source_for_def(tcx, trait_def_id);

    let trait_def = tcx.trait_def(trait_def_id);
    let is_auto = trait_def.has_auto_impl;
    let is_unsafe = trait_def.safety.is_unsafe();

    // Get supertraits
    let supertraits: Vec<String> = tcx
        .explicit_super_predicates_of(trait_def_id)
        .skip_binder()
        .iter()
        .filter_map(|(pred, _)| {
            if let ty::ClauseKind::Trait(trait_pred) = pred.kind().skip_binder() {
                if trait_pred.def_id() != trait_def_id {
                    Some(tcx.def_path_str(trait_pred.def_id()))
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect();

    let methods = extract_trait_methods(tcx, trait_def_id);
    let assoc_types = extract_trait_assoc_types(tcx, trait_def_id);
    let assoc_consts = extract_trait_assoc_consts(tcx, trait_def_id);
    let implementors = get_trait_implementors(tcx, trait_def_id);

    Some(TraitDetails {
        name,
        path,
        visibility,
        generics,
        where_clause,
        is_auto,
        is_unsafe,
        supertraits,
        methods,
        assoc_types,
        assoc_consts,
        docs,
        attributes,
        source,
        implementors,
        span,
    })
}

fn extract_trait_methods(tcx: TyCtxt<'_>, trait_def_id: DefId) -> Vec<TraitMethodInfo> {
    tcx.associated_item_def_ids(trait_def_id)
        .iter()
        .filter_map(|&item_def_id| {
            let item = tcx.associated_item(item_def_id);
            if !matches!(item.kind, ty::AssocKind::Fn { .. }) {
                return None;
            }

            let sig = tcx.fn_sig(item_def_id).skip_binder();
            let has_default = item.defaultness(tcx).has_value();
            let docs = extract_docs(tcx, item_def_id);
            let attributes = extract_attributes(tcx, item_def_id);
            let default_body = if has_default {
                get_source_for_def(tcx, item_def_id)
            } else {
                None
            };

            Some(TraitMethodInfo {
                name: item.name().to_string(),
                signature: format!("{:?}", sig),
                parsed_signature: parse_fn_signature(tcx, item_def_id),
                has_default,
                default_body,
                is_unsafe: sig.safety().is_unsafe(),
                docs,
                attributes,
                span: extract_span_info(tcx, item_def_id),
            })
        })
        .collect()
}

fn extract_trait_assoc_types(tcx: TyCtxt<'_>, trait_def_id: DefId) -> Vec<AssocTypeInfo> {
    tcx.associated_item_def_ids(trait_def_id)
        .iter()
        .filter_map(|&item_def_id| {
            let item = tcx.associated_item(item_def_id);
            if !matches!(item.kind, ty::AssocKind::Type { .. }) {
                return None;
            }

            let docs = extract_docs(tcx, item_def_id);
            let bounds = extract_assoc_type_bounds(tcx, item_def_id);

            Some(AssocTypeInfo {
                name: item.name().to_string(),
                ty: None,
                bounds,
                default: None, // TODO: check for default
                docs,
                span: extract_span_info(tcx, item_def_id),
            })
        })
        .collect()
}

fn extract_assoc_type_bounds(tcx: TyCtxt<'_>, def_id: DefId) -> Vec<String> {
    let predicates = tcx.item_bounds(def_id);
    predicates
        .skip_binder()
        .iter()
        .map(|pred| format!("{:?}", pred))
        .collect()
}

fn extract_trait_assoc_consts(tcx: TyCtxt<'_>, trait_def_id: DefId) -> Vec<AssocConstInfo> {
    tcx.associated_item_def_ids(trait_def_id)
        .iter()
        .filter_map(|&item_def_id| {
            let item = tcx.associated_item(item_def_id);
            if !matches!(item.kind, ty::AssocKind::Const { .. }) {
                return None;
            }

            let ty = tcx.type_of(item_def_id).skip_binder();
            let docs = extract_docs(tcx, item_def_id);

            Some(AssocConstInfo {
                name: item.name().to_string(),
                ty: format!("{:?}", ty),
                value: None,
                docs,
                span: extract_span_info(tcx, item_def_id),
            })
        })
        .collect()
}

fn get_trait_implementors(tcx: TyCtxt<'_>, trait_def_id: DefId) -> Vec<String> {
    let mut implementors = Vec::new();
    let crate_items = tcx.hir_crate_items(());

    for item_id in crate_items.free_items() {
        let impl_def_id = item_id.owner_id.to_def_id();
        if !matches!(tcx.def_kind(impl_def_id), DefKind::Impl { .. }) {
            continue;
        }

        if let Some(trait_ref) = tcx.impl_trait_ref(impl_def_id) {
            if trait_ref.skip_binder().def_id == trait_def_id {
                let self_ty = trait_ref.skip_binder().self_ty();
                implementors.push(get_type_path_string(tcx, self_ty));
            }
        }
    }

    implementors
}

fn extract_trait_impl_details(tcx: TyCtxt<'_>, impl_def_id: DefId) -> Option<TraitImplDetails> {
    let trait_ref = tcx.impl_trait_ref(impl_def_id)?;
    let trait_ref = trait_ref.skip_binder();

    let self_ty = get_type_path_string(tcx, trait_ref.self_ty());
    let trait_path = tcx.def_path_str(trait_ref.def_id);
    let generics = extract_generics(tcx, impl_def_id);
    let where_clause = extract_where_clause(tcx, impl_def_id);
    let span = extract_span_info(tcx, impl_def_id);
    let source = get_source_for_def(tcx, impl_def_id);

    let polarity = tcx.impl_polarity(impl_def_id);
    let is_negative = matches!(polarity, ty::ImplPolarity::Negative);

    let methods = extract_impl_methods(tcx, impl_def_id);
    let assoc_types = extract_impl_assoc_types(tcx, impl_def_id);
    let assoc_consts = extract_impl_assoc_consts(tcx, impl_def_id);

    Some(TraitImplDetails {
        self_ty,
        trait_path,
        generics,
        where_clause,
        is_negative,
        is_unsafe: false,
        methods,
        assoc_types,
        assoc_consts,
        source,
        span,
    })
}

fn extract_inherent_impl_details(
    tcx: TyCtxt<'_>,
    impl_def_id: DefId,
) -> Option<InherentImplDetails> {
    let self_ty = tcx.type_of(impl_def_id).skip_binder();
    let generics = extract_generics(tcx, impl_def_id);
    let where_clause = extract_where_clause(tcx, impl_def_id);
    let span = extract_span_info(tcx, impl_def_id);
    let source = get_source_for_def(tcx, impl_def_id);

    let methods = extract_impl_methods(tcx, impl_def_id);
    let assoc_types = extract_impl_assoc_types(tcx, impl_def_id);
    let assoc_consts = extract_impl_assoc_consts(tcx, impl_def_id);

    Some(InherentImplDetails {
        self_ty: get_type_path_string(tcx, self_ty),
        generics,
        where_clause,
        is_unsafe: false,
        methods,
        assoc_consts,
        assoc_types,
        source,
        span,
    })
}

fn extract_impl_methods(tcx: TyCtxt<'_>, impl_def_id: DefId) -> Vec<MethodDetails> {
    tcx.associated_item_def_ids(impl_def_id)
        .iter()
        .filter_map(|&item_def_id| {
            let item = tcx.associated_item(item_def_id);
            if !matches!(item.kind, ty::AssocKind::Fn { .. }) {
                return None;
            }

            let sig = tcx.fn_sig(item_def_id).skip_binder();
            let docs = extract_docs(tcx, item_def_id);
            let attributes = extract_attributes(tcx, item_def_id);
            let body_source = get_source_for_def(tcx, item_def_id);
            let body_tokens = extract_body_tokens(tcx, item_def_id);

            Some(MethodDetails {
                name: item.name().to_string(),
                path: tcx.def_path_str(item_def_id),
                signature: format!("{:?}", sig),
                parsed_signature: parse_fn_signature(tcx, item_def_id),
                has_body: true,
                body_source,
                body_tokens,
                is_unsafe: sig.safety().is_unsafe(),
                is_const: tcx.is_const_fn(item_def_id),
                is_async: tcx.asyncness(item_def_id).is_async(),
                docs,
                attributes,
                span: extract_span_info(tcx, item_def_id),
            })
        })
        .collect()
}

fn extract_impl_assoc_types(tcx: TyCtxt<'_>, impl_def_id: DefId) -> Vec<AssocTypeInfo> {
    tcx.associated_item_def_ids(impl_def_id)
        .iter()
        .filter_map(|&item_def_id| {
            let item = tcx.associated_item(item_def_id);
            if !matches!(item.kind, ty::AssocKind::Type { .. }) {
                return None;
            }

            let ty = tcx.type_of(item_def_id).skip_binder();
            let docs = extract_docs(tcx, item_def_id);

            Some(AssocTypeInfo {
                name: item.name().to_string(),
                ty: Some(format!("{:?}", ty)),
                bounds: Vec::new(),
                default: None,
                docs,
                span: extract_span_info(tcx, item_def_id),
            })
        })
        .collect()
}

fn extract_impl_assoc_consts(tcx: TyCtxt<'_>, impl_def_id: DefId) -> Vec<AssocConstInfo> {
    tcx.associated_item_def_ids(impl_def_id)
        .iter()
        .filter_map(|&item_def_id| {
            let item = tcx.associated_item(item_def_id);
            if !matches!(item.kind, ty::AssocKind::Const { .. }) {
                return None;
            }

            let ty = tcx.type_of(item_def_id).skip_binder();
            let docs = extract_docs(tcx, item_def_id);

            Some(AssocConstInfo {
                name: item.name().to_string(),
                ty: format!("{:?}", ty),
                value: None,
                docs,
                span: extract_span_info(tcx, item_def_id),
            })
        })
        .collect()
}

fn parse_fn_signature(tcx: TyCtxt<'_>, fn_def_id: DefId) -> FunctionSignature {
    let sig = tcx.fn_sig(fn_def_id).skip_binder();
    let inputs = sig.inputs().skip_binder();

    let mut receiver = None;
    let mut params = Vec::new();

    for (i, ty) in inputs.iter().enumerate() {
        if i == 0 {
            let ty_str = format!("{:?}", ty);
            if ty_str.contains("Self") || ty_str.contains("self") {
                receiver = Some(ReceiverInfo {
                    kind: ty_str.clone(),
                    is_mut: ty_str.contains("mut"),
                    is_ref: ty_str.contains("&"),
                    lifetime: None,
                });
                continue;
            }
        }

        params.push(ParamInfo {
            name: format!("arg{}", i),
            ty: format!("{:?}", ty),
            is_mut: false,
        });
    }

    let return_ty = {
        let output = sig.output().skip_binder();
        if output.is_unit() {
            None
        } else {
            Some(format!("{:?}", output))
        }
    };

    FunctionSignature {
        receiver,
        params,
        return_ty,
        generics: extract_generics(tcx, fn_def_id),
        where_clause: extract_where_clause(tcx, fn_def_id),
    }
}

fn extract_type_alias(tcx: TyCtxt<'_>, def_id: DefId) -> Option<TypeAliasInfo> {
    let name = tcx.item_name(def_id).to_string();
    let path = tcx.def_path_str(def_id);
    let visibility = extract_visibility(tcx, def_id);
    let generics = extract_generics(tcx, def_id);
    let span = extract_span_info(tcx, def_id);
    let docs = extract_docs(tcx, def_id);

    let ty = tcx.type_of(def_id).skip_binder();
    let ty_str = format!("{:?}", ty);

    let (resolved_ty, _chain) = resolve_type_alias_chain(tcx, def_id);

    Some(TypeAliasInfo {
        name,
        path,
        generics,
        ty: ty_str,
        resolved_ty,
        visibility,
        docs,
        span,
    })
}

fn extract_module_info(tcx: TyCtxt<'_>, local_def_id: LocalDefId) -> Option<ModuleInfo> {
    let def_id = local_def_id.to_def_id();

    let name = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        tcx.item_name(def_id).to_string()
    })) {
        Ok(n) if !n.is_empty() => n,
        _ => return None,
    };

    let path = tcx.def_path_str(def_id);
    let visibility = extract_visibility(tcx, def_id);

    // Get child items
    let mut items = Vec::new();
    let mut reexports = Vec::new();

    for child in tcx.module_children(def_id) {
        let child_name = child.ident.to_string();

        if child.reexport_chain.is_empty() {
            items.push(child_name);
        } else {
            // This is a re-export
            if let Some(child_def_id) = child.res.opt_def_id() {
                let original_path = tcx.def_path_str(child_def_id);
                reexports.push(ReexportInfo {
                    name: child_name,
                    original_path,
                    visibility: Visibility::Public, // Re-exports are typically public
                });
            }
        }
    }

    Some(ModuleInfo {
        name,
        path,
        visibility,
        items,
        reexports,
    })
}

// ============================================================================
// Output Functions
// ============================================================================

fn output_extracted_info(info: &CrateTypeInfo, output: &Option<String>) {
    let json = serde_json::to_string_pretty(info).expect("Failed to serialize");

    match output {
        Some(path) => {
            std::fs::write(path, json).expect("Failed to write output file");
        }
        None => {
            println!("{}", json);
        }
    }
}

fn output_query_result(result: &QueryResult) {
    let json = serde_json::to_string(result).expect("Failed to serialize");
    println!("{}", json);
}

// ============================================================================
// Query Execution
// ============================================================================

fn parse_query(query_str: &str) -> Query {
    let parts: Vec<&str> = query_str.split(':').collect();

    match parts[0] {
        "list_items" => Query::ListItems,
        "get_type" if parts.len() >= 2 => Query::GetType {
            path: parts[1].to_string(),
        },
        "get_trait_impls" if parts.len() >= 2 => Query::GetTraitImpls {
            type_path: parts[1].to_string(),
        },
        "get_inherent_impls" if parts.len() >= 2 => Query::GetInherentImpls {
            type_path: parts[1].to_string(),
        },
        "get_fields" if parts.len() >= 2 => Query::GetFields {
            type_path: parts[1].to_string(),
        },
        "get_layout" if parts.len() >= 2 => Query::GetLayout {
            type_path: parts[1].to_string(),
        },
        "get_traits" => Query::GetTraits,
        "get_trait" if parts.len() >= 2 => Query::GetTrait {
            path: parts[1].to_string(),
        },
        "find_types" if parts.len() >= 2 => Query::FindTypes {
            pattern: parts[1].to_string(),
        },
        "resolve_alias" if parts.len() >= 2 => Query::ResolveAlias {
            path: parts[1].to_string(),
        },
        "check_impl" if parts.len() >= 3 => Query::CheckImpl {
            type_path: parts[1].to_string(),
            trait_path: parts[2].to_string(),
        },
        "get_implementors" if parts.len() >= 2 => Query::GetImplementors {
            trait_path: parts[1].to_string(),
        },
        _ => {
            eprintln!("Unknown query: {}", query_str);
            eprintln!("Available queries:");
            eprintln!("  list_items");
            eprintln!("  get_type:<path>");
            eprintln!("  get_trait_impls:<type_path>");
            eprintln!("  get_inherent_impls:<type_path>");
            eprintln!("  get_fields:<type_path>");
            eprintln!("  get_layout:<type_path>");
            eprintln!("  get_traits");
            eprintln!("  get_trait:<path>");
            eprintln!("  find_types:<pattern>");
            eprintln!("  resolve_alias:<path>");
            eprintln!("  check_impl:<type_path>:<trait_path>");
            eprintln!("  get_implementors:<trait_path>");
            std::process::exit(1);
        }
    }
}

fn execute_query(tcx: TyCtxt<'_>, query: &Query) -> QueryResult {
    let info = extract_crate_info(tcx);

    match query {
        Query::ListItems => QueryResult::Success {
            data: QueryData::Items { items: info.items },
        },

        Query::GetType { path } => match info.types.get(path) {
            Some(type_details) => QueryResult::Success {
                data: QueryData::TypeInfo(type_details.clone()),
            },
            None => QueryResult::Error {
                message: format!("Type not found: {}", path),
            },
        },

        Query::GetTraitImpls { type_path } => {
            let impls = info.trait_impls.get(type_path).cloned().unwrap_or_default();
            QueryResult::Success {
                data: QueryData::TraitImpls { impls },
            }
        }

        Query::GetInherentImpls { type_path } => {
            let impls = info
                .inherent_impls
                .get(type_path)
                .cloned()
                .unwrap_or_default();
            QueryResult::Success {
                data: QueryData::InherentImpls { impls },
            }
        }

        Query::GetFields { type_path } => {
            let fields = info
                .types
                .get(type_path)
                .and_then(|t| t.fields.clone())
                .unwrap_or_default();
            QueryResult::Success {
                data: QueryData::Fields { fields },
            }
        }

        Query::GetLayout { type_path } => match info.layouts.get(type_path) {
            Some(layout) => QueryResult::Success {
                data: QueryData::Layout(layout.clone()),
            },
            None => QueryResult::Error {
                message: format!("Layout not available for: {}", type_path),
            },
        },

        Query::GetTraits => {
            let traits: Vec<TraitInfo> = info
                .traits
                .values()
                .map(|t| TraitInfo {
                    name: t.name.clone(),
                    path: t.path.clone(),
                    generics: t.generics.clone(),
                    required_methods: t.methods.iter().filter(|m| !m.has_default).count(),
                    provided_methods: t.methods.iter().filter(|m| m.has_default).count(),
                    supertraits: t.supertraits.clone(),
                })
                .collect();
            QueryResult::Success {
                data: QueryData::Traits { traits },
            }
        }

        Query::GetTrait { path } => match info.traits.get(path) {
            Some(trait_details) => QueryResult::Success {
                data: QueryData::TraitDetails(trait_details.clone()),
            },
            None => QueryResult::Error {
                message: format!("Trait not found: {}", path),
            },
        },

        Query::FindTypes { pattern } => {
            let types: Vec<TypeSummary> = info
                .types
                .values()
                .filter(|t| bronzite_types::path_matches_pattern(&t.path, pattern))
                .map(|t| TypeSummary {
                    name: t.name.clone(),
                    path: t.path.clone(),
                    kind: t.kind.clone(),
                    generics: t.generics.clone(),
                })
                .collect();
            QueryResult::Success {
                data: QueryData::Types { types },
            }
        }

        Query::ResolveAlias { path } => match info.type_aliases.get(path) {
            Some(alias) => QueryResult::Success {
                data: QueryData::ResolvedType {
                    original: alias.ty.clone(),
                    resolved: alias.resolved_ty.clone(),
                    chain: vec![alias.path.clone(), alias.resolved_ty.clone()],
                },
            },
            None => QueryResult::Error {
                message: format!("Type alias not found: {}", path),
            },
        },

        Query::CheckImpl {
            type_path,
            trait_path,
        } => {
            let impls = info.trait_impls.get(type_path).cloned().unwrap_or_default();
            let impl_info = impls.into_iter().find(|i| i.trait_path == *trait_path);
            QueryResult::Success {
                data: QueryData::ImplCheck {
                    implements: impl_info.is_some(),
                    impl_info,
                },
            }
        }

        Query::GetImplementors { trait_path } => match info.traits.get(trait_path) {
            Some(trait_details) => {
                let types: Vec<TypeSummary> = trait_details
                    .implementors
                    .iter()
                    .filter_map(|ty_str| {
                        info.types
                            .values()
                            .find(|t| t.path == *ty_str)
                            .map(|t| TypeSummary {
                                name: t.name.clone(),
                                path: t.path.clone(),
                                kind: t.kind.clone(),
                                generics: t.generics.clone(),
                            })
                    })
                    .collect();
                QueryResult::Success {
                    data: QueryData::Implementors { types },
                }
            }
            None => QueryResult::Error {
                message: format!("Trait not found: {}", trait_path),
            },
        },

        Query::Ping => QueryResult::Success {
            data: QueryData::Pong,
        },

        Query::Shutdown => QueryResult::Success {
            data: QueryData::ShuttingDown,
        },
    }
}
