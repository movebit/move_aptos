// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{context::Context, symbols::Symbols};
use anyhow::Result;
use im::HashMap;
use lsp_server::Request;
use lsp_types::{CompletionItem, CompletionItemKind, CompletionParams};
use move_command_line_common::files::FileHash;
use move_compiler::parser::ast::ModuleName;
use move_compiler::parser::ast::{Definition, ModuleIdent};
use move_compiler::parser::*;
use move_compiler::shared::Identifier;
use move_compiler::{
    parser::{
        ast::*,
        keywords::{BUILTINS, CONTEXTUAL_KEYWORDS, KEYWORDS, PRIMITIVE_TYPES},
        lexer::{Lexer, Tok},
    },
    shared::*,
    CommentMap,
};
use move_ir_types::ast::Statement_;
use move_ir_types::location::{Loc, Spanned};
use move_package::source_package::layout::SourcePackageLayout;
use move_package::source_package::manifest_parser::*;
use move_package::source_package::*;
use move_package::*;

use core::panic;
use move_symbol_pool::Symbol;
use petgraph::data::Build;
use serde::__private::de;
use std::borrow::{Borrow, BorrowMut};
use std::cell::RefCell;
use std::collections::btree_map::BTreeMap;
use std::hash::Hash;
use std::ops::{Add, Deref};
use std::path::Path;
use std::sync::Mutex;
use std::vec;
use std::{collections::HashSet, path::PathBuf, rc::Rc};
use tempfile::TempPath;
use walkdir::WalkDir;

/// Constructs an `lsp_types::CompletionItem` with the given `label` and `kind`.
fn completion_item(label: &str, kind: CompletionItemKind) -> CompletionItem {
    CompletionItem {
        label: label.to_owned(),
        kind: Some(kind),
        ..Default::default()
    }
}

/// Return a list of completion items corresponding to each one of Move's keywords.
///
/// Currently, this does not filter keywords out based on whether they are valid at the completion
/// request's cursor position, but in the future it ought to. For example, this function returns
/// all specification language keywords, but in the future it should be modified to only do so
/// within a spec block.
fn keywords() -> Vec<CompletionItem> {
    KEYWORDS
        .iter()
        .chain(CONTEXTUAL_KEYWORDS.iter())
        .chain(PRIMITIVE_TYPES.iter())
        .map(|label| {
            let kind = if label == &"copy" || label == &"move" {
                CompletionItemKind::Operator
            } else {
                CompletionItemKind::Keyword
            };
            completion_item(label, kind)
        })
        .collect()
}

/// Return a list of completion items of Move's primitive types
fn primitive_types() -> Vec<CompletionItem> {
    PRIMITIVE_TYPES
        .iter()
        .map(|label| completion_item(label, CompletionItemKind::Keyword))
        .collect()
}

/// Return a list of completion items corresponding to each one of Move's builtin functions.
fn builtins() -> Vec<CompletionItem> {
    BUILTINS
        .iter()
        .map(|label| completion_item(label, CompletionItemKind::Function))
        .collect()
}

/// Sends the given connection a response to a completion request.
///
/// The completions returned depend upon where the user's cursor is positioned.
pub fn on_completion_request2(context: &Context, request: &Request, symbols: &Symbols) {
    // eprintln!("handling completion request");
    // let parameters = serde_json::from_value::<CompletionParams>(request.params.clone())
    //     .expect("could not deserialize completion request");

    // let path = parameters
    //     .text_document_position
    //     .text_document
    //     .uri
    //     .to_file_path()
    //     .unwrap();
    // let buffer = context.files.get(&path);

    // if buffer.is_none() {
    //     eprintln!(
    //         "Could not read '{:?}' when handling completion request",
    //         path
    //     );
    // }

    // let buffer = buffer.unwrap();

    // let file_hash = FileHash::new(buffer);

    // match element {}
    // let mut items = vec![];
    // items.extend(builtins());
    // items.extend(primitive_types());
    // items.extend(keywords());

    // let result = serde_json::to_value(items).expect("could not serialize completion response");
    // eprintln!("about to send completion response");
    // let response = lsp_server::Response::new_ok(request.id.clone(), result);
    // if let Err(err) = context
    //     .connection
    //     .sender
    //     .send(lsp_server::Message::Response(response))
    // {
    //     eprintln!("could not send completion response: {:?}", err);
    // }
}

/// All Modules.
#[derive(Default, Debug)]
pub struct Modules {
    modules: HashMap<PathBuf /*  this is a Move.toml like xxxx/Move.toml  */, IDEModule>,
    ///
    named_to_addresses: HashMap<Symbol, NumericalAddress>,
}

fn read_all_move_toml_files(path: &PathBuf) -> Vec<PathBuf> {
    let mut ret = vec![];
    for item in WalkDir::new(path) {
        let item = item.unwrap();
        if item.file_type().is_file()
            && item
                .path()
                .to_str()
                .unwrap()
                .ends_with(SourcePackageLayout::Manifest.location_str())
        {
            ret.push(PathBuf::from(item.path()));
        }
    }
    ret
}

#[test]
fn xxx() {
    let x = Modules::new(&PathBuf::from("/home/yuyang/projects/test-move"));
    println!("xxxxxxxx:{:?}", x);
}

impl Modules {
    pub fn new(working_dir: &PathBuf) -> Self {
        let mut x = Self::default();
        // read all Move.toml
        let toml_files = read_all_move_toml_files(working_dir);
        for t in toml_files.iter() {
            x.hanle_one(t).unwrap();
        }
        x
    }

    fn hanle_one(&mut self, manifest_path: &PathBuf) -> Result<()> {
        let manifest = parse_move_manifest_from_file(manifest_path).unwrap();
        println!("xxxxxxxxx: {:?} ", manifest);
        let build_cfg = BuildConfig {
            dev_mode: true,
            test_mode: true,
            generate_docs: false,
            generate_abis: false,
            install_dir: None,
            force_recompilation: false,
            additional_named_addresses: BTreeMap::new(),
            architecture: None,
            fetch_deps_only: true,
            skip_fetch_latest_git_deps: false,
        };

        let g = build_cfg
            .resolution_graph_for_package(manifest_path.as_path(), &mut std::io::stderr())?;

        unimplemented!()
    }

    /// Entrance for `ScopeVisitor` base on analyze.
    pub fn run_visitor(&self, visitor: &mut dyn ScopeVisitor) {
        let mut global_scope = Scopes::new();
        // Enter all global to global_scope.
        for (_, modules) in self.modules.iter() {
            for (_, d) in modules.defs.iter() {
                match d {
                    Definition::Module(ref m) => {
                        self.enter_module_top(&mut global_scope, m);
                    }
                    Definition::Address(ref a) => {
                        self.enter_address_top(&mut global_scope, a);
                    }
                    Definition::Script(ref s) => {
                        self.enter_script_top(&mut global_scope, s);
                    }
                }
            }
        }

        // Scan all for.
        for (_, modules) in self.modules.iter() {
            for (f, d) in modules.defs.iter() {
                // We have match the files.
                match d {
                    Definition::Module(x) => {
                        if !visitor.file_should_visit(x.loc.file_hash()) {
                            continue;
                        }
                    }
                    Definition::Address(_) => todo!(),
                    Definition::Script(_) => todo!(),
                }
            }
        }
    }

    /// Enter Top level
    fn enter_module_top(&self, s: &Scopes, module: &ModuleDefinition) {}
    fn enter_script_top(&self, s: &Scopes, module: &Script) {}
    fn enter_address_top(&self, s: &Scopes, module: &AddressDefinition) {}

    ///
    fn visit_function(&self, function: &Function, scopes: &Scopes, visitor: &mut dyn ScopeVisitor) {
        return scopes.enter_scope(|s| {
            self.visit_signature(&function.signature, s, visitor);
            if visitor.finished() {
                return;
            }
            match function.body.value {
                FunctionBody_::Native => {}
                FunctionBody_::Defined(ref seq) => self.visit_block(seq, scopes, visitor),
            }
        });
    }

    fn visit_block(&self, seq: &Sequence, scopes: &Scopes, visitor: &mut dyn ScopeVisitor) {
        scopes.enter_scope(|scopes| {
            for u in seq.0.iter() {
                self.visit_use_decl(u, scopes, visitor);
                if visitor.finished() {
                    return;
                }
            }
            for s in seq.1.iter() {
                self.visit_sequence_item(s, scopes, visitor);
                if visitor.finished() {
                    return;
                }
            }
            if let Some(ref exp) = seq.3.as_ref() {
                self.visit_expr(exp, scopes, visitor);
            }
        });
    }

    fn visit_sequence_item(
        &self,
        seq: &SequenceItem,
        scopes: &Scopes,
        visitor: &mut dyn ScopeVisitor,
    ) {
        match seq.value {
            SequenceItem_::Seq(ref e) => {
                self.visit_expr(e, scopes, visitor);
                if visitor.finished() {
                    return;
                }
            }
            SequenceItem_::Declare(ref list, ref ty) => {
                self.visit_bind_list(list, ty, None, scopes, visitor);
                if visitor.finished() {
                    return;
                }
            }
            SequenceItem_::Bind(ref list, ref ty, ref expr) => {
                self.visit_bind_list(list, ty, Some(expr), scopes, visitor);
                if visitor.finished() {
                    return;
                }
            }
        }
    }

    fn visit_bind_list(
        &self,
        bind_list: &BindList,
        ty: &Option<Type>,
        expr: Option<&Box<Exp>>,
        scopes: &Scopes,
        visitor: &mut dyn ScopeVisitor,
    ) {
        let ty = if let Some(ty) = ty {
            scopes.resolve_type(ty)
        } else if let Some(expr) = expr {
            self.get_expr_type(expr, scopes)
        } else {
            ResolvedType::new_unknown(bind_list.loc)
        };
        for (index, bind) in bind_list.value.iter().enumerate() {
            let ty = ty.nth_ty(index);
            let unknown = ResolvedType::new_unknown(bind_list.loc);
            let ty = ty.unwrap_or(&unknown);
            self.visit_bind(bind, ty, scopes, None, visitor);
            if visitor.finished() {
                return;
            }
        }
    }

    fn visit_bind(
        &self,
        bind: &Bind,
        ty: &ResolvedType,
        scopes: &Scopes,
        field: Option<&'_ Field>,
        visitor: &mut dyn ScopeVisitor,
    ) {
        match &bind.value {
            Bind_::Var(var) => {
                let item = Item::ExprVar(var.clone());
                visitor.handle_item(scopes, &item);
                if visitor.finished() {
                    return;
                }
                scopes.enter_item(var.0.value, item);
                return;
            }
            Bind_::Unpack(_, _, _) => todo!(),
        }
    }

    fn visit_type_apply(&self, ty: &Type, scopes: &Scopes, visitor: &mut dyn ScopeVisitor) {
        let item = Item::ApplyType(ty.clone());
        visitor.handle_item(scopes, &item);
    }

    fn name_to_addr(&self, name: Symbol) -> &NumericalAddress {
        self.named_to_addresses.get(&name).unwrap()
    }

    /// Get A Type for expr if possible.
    fn get_expr_type(&self, expr: &Exp, scopes: &Scopes) -> ResolvedType {
        match &expr.value {
            Exp_::Value(ref x) => match &x.value {
                Value_::Address(_) => ResolvedType::new_build_in(BuildInType::Address),
                Value_::Num(_) => ResolvedType::new_build_in(BuildInType::NumType),
                Value_::Bool(_) => ResolvedType::new_build_in(BuildInType::Bool),
                Value_::HexString(_) | Value_::ByteString(_) => {
                    ResolvedType::new_build_in(BuildInType::String)
                }
            },
            Exp_::Move(x) | Exp_::Copy(x) => scopes.find_var_type(x.0.value),
            Exp_::Name(name, _ /*  TODO this is a error. */) => {
                return UNKNOWN_TYPE.clone();
            }
            Exp_::Call(name, is_macro, ref type_args, exprs) => {
                if *is_macro {
                    let c = MacroCall::from_chain(name);
                    match c {
                        MacroCall::Assert => ResolvedType::new_unit(name.loc),
                    }
                } else {
                    let fun_type =
                        scopes.find_name_access_chain_type(name, |name| self.name_to_addr(name));
                    match &fun_type.0.value {
                        ResolvedType_::Fun(ref type_parameters, parameters, _) => {
                            let type_args: Option<Vec<ResolvedType>> =
                                if let Some(type_args) = type_args {
                                    Some(type_args.iter().map(|x| scopes.resolve_type(x)).collect())
                                } else {
                                    None
                                };
                            let mut fun_type = fun_type.clone();
                            let mut types = HashMap::new();
                            if let Some(ref ts) = type_args {
                                for (para, args) in type_parameters.iter().zip(ts.iter()) {
                                    types.insert(para.0.value, args.clone());
                                }
                            } else if type_parameters.len() > 0 {
                                //
                                let exprs_types: Vec<_> = exprs
                                    .value
                                    .iter()
                                    .map(|e| self.get_expr_type(e, scopes))
                                    .collect();
                                infer_type_on_expression(
                                    &mut types,
                                    type_parameters,
                                    parameters,
                                    &exprs_types,
                                );
                            }
                            fun_type.bind_type_parameter(&types);
                            match &fun_type.0.value {
                                ResolvedType_::Fun(_, _, ret) => ret.as_ref().clone(),
                                _ => unreachable!(),
                            }
                        }
                        // This maybe is a error.
                        _ => return UNKNOWN_TYPE.clone(),
                    }
                }
            }

            Exp_::Pack(name, type_args, _) => {
                let mut struct_ty =
                    scopes.find_name_access_chain_type(name, |s| self.name_to_addr(s));
                match &struct_ty.0.value {
                    ResolvedType_::Struct(_, type_parameters, fields) => {
                        let type_args: Option<Vec<ResolvedType>> =
                            if let Some(type_args) = type_args {
                                Some(type_args.iter().map(|x| scopes.resolve_type(x)).collect())
                            } else {
                                None
                            };
                        let mut types = HashMap::new();
                        if let Some(ref ts) = type_args {
                            for (para, args) in type_parameters.iter().zip(ts.iter()) {
                                types.insert(para.name.value, args.clone());
                            }
                        }
                        struct_ty.bind_type_parameter(&types);
                        struct_ty
                    }
                    _ => UNKNOWN_TYPE.clone(),
                }
            }
            Exp_::Vector(loc, ty, exprs) => {
                let mut ty = if let Some(ty) = ty {
                    if let Some(ty) = ty.get(0) {
                        Some(scopes.resolve_type(ty))
                    } else {
                        None
                    }
                } else {
                    None
                };
                if option_ty_is_valid(&ty) {
                    for e in exprs.value.iter() {
                        let ty2 = self.get_expr_type(e, scopes);
                        if !ty2.is_unknown() {
                            ty = Some(ty2);
                            break;
                        }
                    }
                }
                ty.unwrap_or(ResolvedType::new_unknown(loc.clone()))
            }
            Exp_::IfElse(_, then, else_) => {
                let mut ty = self.get_expr_type(expr, scopes);
                if ty.is_err() {
                    if let Some(else_) = else_ {
                        ty = self.get_expr_type(else_, scopes);
                    }
                }
                ty
            }
            Exp_::While(_, _) | Exp_::Loop(_) => ResolvedType::new_unit(expr.loc),
            Exp_::Block(_) => todo!(),
            Exp_::Lambda(_, _) => todo!(),
            Exp_::Quant(_, _, _, _, _) => todo!(),
            Exp_::ExpList(_) => todo!(),
            Exp_::Unit => ResolvedType::new_unit(expr.loc),
            Exp_::Assign(_, _) => ResolvedType::new_unit(expr.loc),
            Exp_::Return(_) => ResolvedType::new_unit(expr.loc),
            Exp_::Abort(_) => ResolvedType::new_unit(expr.loc),
            Exp_::Break => ResolvedType::new_unit(expr.loc),
            Exp_::Continue => ResolvedType::new_unit(expr.loc),
            Exp_::Dereference(e) => {
                let ty = self.get_expr_type(e, scopes);
                match &ty.0.value {
                    ResolvedType_::Ref(_, t) => t.as_ref().clone(),
                    _ => ty,
                }
            }
            Exp_::UnaryExp(_, e) => {
                let ty = self.get_expr_type(e, scopes);
                ty
            }
            Exp_::BinopExp(left, op, right) => {
                let left_ty = self.get_expr_type(left, scopes);
                let right_ty = self.get_expr_type(right, scopes);
                let pick = |prefer_left: bool| {
                    if prefer_left && !left_ty.is_err() {
                        left_ty.clone()
                    } else {
                        right_ty.clone()
                    }
                };
                match op.value {
                    BinOp_::Add => pick(true),
                    BinOp_::Sub => pick(true),
                    BinOp_::Mul => pick(true),
                    BinOp_::Mod => pick(true),
                    BinOp_::Div => pick(true),
                    BinOp_::BitOr => pick(true),
                    BinOp_::BitAnd => pick(true),
                    BinOp_::Xor => pick(true),
                    BinOp_::Shl => pick(true),
                    BinOp_::Shr => pick(true),
                    BinOp_::Range => todo!(),
                    BinOp_::Implies => todo!(),
                    BinOp_::Iff => todo!(),
                    BinOp_::And => todo!(),
                    BinOp_::Or => todo!(),
                    BinOp_::Eq
                    | BinOp_::Neq
                    | BinOp_::Lt
                    | BinOp_::Gt
                    | BinOp_::Le
                    | BinOp_::Ge => ResolvedType::new_build_in(BuildInType::Bool),
                }
            }
            Exp_::Borrow(is_mut, e) => {
                let ty = self.get_expr_type(e, scopes);
                ResolvedType::new_ref(expr.loc, *is_mut, ty)
            }
            Exp_::Dot(e, name) => {
                let ty = self.get_expr_type(e, scopes);
                if let Some(field) = ty.find_filed_by_name(name.value) {
                    field.1.clone()
                } else {
                    ty
                }
            }
            Exp_::Index(e, index) => {
                let ty = self.get_expr_type(e, scopes);
                if let Some(v) = ty.is_vector() {
                    v.clone()
                } else {
                    ty
                }
            }
            Exp_::Cast(_, ty) => {
                let ty = scopes.resolve_type(ty);
                ty
            }
            Exp_::Annotate(_, ty) => scopes.resolve_type(ty),
            Exp_::Spec(_) => todo!(),
            Exp_::UnresolvedError => {
                // Nothings. didn't know what to do.
                ResolvedType::new_unknown(expr.loc)
            }
        }
    }

    fn visit_expr(&self, exp: &Exp, scopes: &Scopes, visitor: &mut dyn ScopeVisitor) {
        match &exp.value {
            Exp_::Value(ref v) => {
                if let Some(name) = get_name_from_value(v) {
                    let item = Item::ExprAddressName(name.clone());
                    visitor.handle_item(scopes, &item);
                }
            }
            Exp_::Move(var) | Exp_::Copy(var) => {
                let item = Item::ExprVar(var.clone());
                visitor.handle_item(scopes, &item);
            }

            Exp_::Name(chain, _ty /*  How to use _ty */) => {
                let item = Item::NameAccessChain(chain.clone());
                visitor.handle_item(scopes, &item);
            }
            Exp_::Call(ref chain, is_macro, ref types, ref exprs) => {
                if *is_macro {
                    let c = MacroCall::from_chain(chain);
                    let item = Item::MacroCall(c);
                    visitor.handle_item(scopes, &item);
                } else {
                    let item = Item::NameAccessChain(chain.clone());
                    visitor.handle_item(scopes, &item);
                }
                if visitor.finished() {
                    return;
                }
                if let Some(ref types) = types {
                    for t in types.iter() {
                        let item = Item::ApplyType(t.clone());
                        visitor.handle_item(scopes, &item);
                    }
                }
                for expr in exprs.value.iter() {
                    self.visit_expr(exp, scopes, visitor);
                    if visitor.finished() {
                        return;
                    }
                }
            }

            Exp_::Pack(ref leading, ref types, fields) => {
                let ty = self.get_expr_type(exp, scopes);
                let item = Item::NameAccessChain(leading.clone());
                visitor.handle_item(scopes, &item);
                if visitor.finished() {
                    return;
                }
                if let Some(types) = types {
                    for t in types.iter() {
                        self.visit_type_apply(t, scopes, visitor);
                        if visitor.finished() {
                            return;
                        }
                    }
                }
                for f in fields.iter() {
                    let field_type = ty.find_filed_by_name(f.0.value());
                    if let Some(field_type) = field_type {
                        let item = Item::FieldInitialization(f.0.clone(), field_type.1.clone());
                        visitor.handle_item(scopes, &item);
                    }
                    self.visit_expr(&f.1, scopes, visitor);
                    if visitor.finished() {
                        return;
                    }
                }
            }
            Exp_::Vector(_loc, ref ty, ref exprs) => {
                if let Some(ty) = ty {
                    for t in ty.iter() {
                        self.visit_type_apply(t, scopes, visitor);
                        if visitor.finished() {
                            return;
                        }
                    }
                }
                for e in exprs.value.iter() {
                    self.visit_expr(e, scopes, visitor);
                    if visitor.finished() {
                        return;
                    }
                }
            }

            Exp_::IfElse(condition, then_, else_) => {
                self.visit_expr(condition, scopes, visitor);
                if visitor.finished() {
                    return;
                }
                self.visit_expr(then_, scopes, visitor);
                if visitor.finished() {
                    return;
                }
                if let Some(else_) = else_ {
                    self.visit_expr(else_.as_ref(), scopes, visitor);
                }
            }
            Exp_::While(condition, body) => {
                self.visit_expr(condition, scopes, visitor);
                if visitor.finished() {
                    return;
                }
                self.visit_expr(body.as_ref(), scopes, visitor);
            }

            Exp_::Loop(e) => {
                self.visit_expr(e.as_ref(), scopes, visitor);
            }
            Exp_::Block(b) => self.visit_block(b, scopes, visitor),
            Exp_::Lambda(_, _) => todo!(),
            Exp_::Quant(_, _, _, _, _) => todo!(),
            Exp_::ExpList(list) => {
                for e in list.iter() {
                    self.visit_expr(e, scopes, visitor);
                    if visitor.finished() {
                        return;
                    }
                }
            }
            Exp_::Unit => {
                // Nothing.
            }
            Exp_::Assign(left, right) => {
                self.visit_expr(left, scopes, visitor);
                if visitor.finished() {
                    return;
                }
                self.visit_expr(right, scopes, visitor);
            }
            Exp_::Return(e) => {
                if let Some(e) = e {
                    self.visit_expr(e, scopes, visitor);
                }
            }
            Exp_::Abort(e) => self.visit_expr(e.as_ref(), scopes, visitor),
            Exp_::Break => {
                let item = Item::KeyWords("break");
                visitor.handle_item(scopes, &item);
            }
            Exp_::Continue => {
                let item = Item::KeyWords("continue");
                visitor.handle_item(scopes, &item);
            }
            Exp_::Dereference(x) => {
                self.visit_expr(x.as_ref(), scopes, visitor);
            }
            Exp_::UnaryExp(_, e) => {
                self.visit_expr(e.as_ref(), scopes, visitor);
            }
            Exp_::BinopExp(left, _, right) => {
                self.visit_expr(left, scopes, visitor);
                if visitor.finished() {
                    return;
                }
                self.visit_expr(right, scopes, visitor);
            }
            Exp_::Borrow(_, e) => {
                self.visit_expr(e.as_ref(), scopes, visitor);
            }
            Exp_::Dot(e, field) => {
                self.visit_expr(e.as_ref(), scopes, visitor);
                if visitor.finished() {
                    return;
                }
                let ty = self.get_expr_type(e, scopes);
                if let Some(field) = ty.find_filed_by_name(field.value) {
                    let item = Item::AccessFiled(field.0.clone(), field.1.clone());
                    visitor.handle_item(scopes, &item);
                }
            }
            Exp_::Index(_, _) => todo!(),
            Exp_::Cast(e, ty) => {
                self.visit_expr(e.as_ref(), scopes, visitor);
                if visitor.finished() {
                    return;
                }
                self.visit_type_apply(ty, scopes, visitor);
            }
            Exp_::Annotate(e, ty) => {
                self.visit_expr(e.as_ref(), scopes, visitor);
                if visitor.finished() {
                    return;
                }
                self.visit_type_apply(ty, scopes, visitor);
            }
            Exp_::Spec(_) => todo!(),
            Exp_::UnresolvedError => {
                //
            }
        }
    }

    fn visit_use_decl(&self, use_decl: &UseDecl, scopes: &Scopes, visitor: &mut dyn ScopeVisitor) {
        let item = Item::Use(use_decl.clone());
        visitor.handle_item(scopes, &item);
        if visitor.finished() {
            return;
        }
        match &use_decl.use_ {
            Use::Module(module, alias) => {
                let mut name = module.value.module.0.value;
                if let Some(alias) = alias {
                    name = alias.0.value;
                }
                let r = scopes.visit_top_scope(|top| -> Option<Rc<RefCell<Scope>>> {
                    let x = top
                        .address
                        .get(match &module.value.address.value {
                            LeadingNameAccess_::AnonymousAddress(num) => num,
                            LeadingNameAccess_::Name(name) => self.name_to_addr(name.value),
                        })?
                        .modules
                        .get(&module.value.module.0.value)?
                        .clone();
                    Some(x)
                });
                if r.is_none() {
                    return;
                }
                let r = r.unwrap();
                let item = Item::ImportedUseModule(module.clone(), r);
                scopes.enter_item(name, item);
            }
            Use::Members(module, members) => {
                let r = scopes.visit_top_scope(|top| -> Option<Rc<RefCell<Scope>>> {
                    let x = top
                        .address
                        .get(match &module.value.address.value {
                            LeadingNameAccess_::AnonymousAddress(num) => num,
                            LeadingNameAccess_::Name(name) => self.name_to_addr(name.value),
                        })?
                        .modules
                        .get(&module.value.module.0.value)?
                        .clone();
                    Some(x)
                });
                if r.is_none() {
                    return;
                }
                let r = r.unwrap();
                for (member, alias) in members.iter() {
                    let mut name = member.value;
                    if let Some(alias) = alias {
                        name = alias.value;
                    }
                    if let Some(i) = r.as_ref().borrow().items.get(&member.value) {
                        let item = Item::ImportUse(Box::new(i.clone()));
                        scopes.enter_item(name, item);
                    }
                }
            }
        }
    }

    fn visit_signature(
        &self,
        signature: &FunctionSignature,
        scopes: &Scopes,
        visitor: &mut dyn ScopeVisitor,
    ) {
        for (name, v) in signature.type_parameters.iter() {
            let item = Item::TParam(name.clone(), v.clone());
            visitor.handle_item(scopes, &item);
            if visitor.finished() {
                return;
            }
            // Enter this.
            scopes.enter_item(name.value, item);
        }

        for (v, t) in signature.parameters.iter() {
            let item = Item::ApplyType(t.clone());
            // found
            visitor.handle_item(scopes, &item);
            if visitor.finished() {
                return;
            }
            let t = scopes.resolve_type(t);
            let item = Item::Parameter(v.clone(), t);
            // found
            visitor.handle_item(scopes, &item);
            if visitor.finished() {
                return;
            }
            scopes.enter_item(v.value(), item)
        }
    }
}

#[derive(Default, Debug, Clone)]
pub struct Scope {
    items: HashMap<Symbol, Item>,
    is_function: bool,
    is_spec: bool,
    /// Top level scope have this structure.
    addresses: Option<Addresses>,
}

#[derive(Debug, Clone)]
struct Addresses {
    /// address to modules
    address: HashMap<NumericalAddress, Address>,
}

impl Addresses {
    fn new() -> Self {
        Self {
            address: Default::default(),
        }
    }
}

#[derive(Debug, Default, Clone)]
struct Address {
    /// module name to Scope.
    modules: HashMap<Symbol, Rc<RefCell<Scope>>>,
}

/// Check is option is Some and ResolvedType is not unknown and not a error.
fn option_ty_is_valid(x: &Option<ResolvedType>) -> bool {
    if let Some(ref x) = x {
        !x.is_err()
    } else {
        false
    }
}

impl Scope {
    fn new_top() -> Self {
        Self {
            items: Default::default(),
            is_function: false,
            is_spec: false,
            addresses: Some(Addresses::new()),
        }
    }
    fn enter_build_in(&mut self) {
        self.enter_item(Symbol::from("bool"), Item::BuildInType(BuildInType::Bool));
        self.enter_item(Symbol::from("u8"), Item::BuildInType(BuildInType::U8));
        self.enter_item(Symbol::from("u64"), Item::BuildInType(BuildInType::U64));
        self.enter_item(Symbol::from("u128"), Item::BuildInType(BuildInType::U128));
        self.enter_item(
            Symbol::from("address"),
            Item::BuildInType(BuildInType::Address),
        );
    }
    fn enter_item(&mut self, s: Symbol, item: Item) {
        self.items.insert(s, item);
    }
}

#[derive(Clone, Copy, Debug)]
pub enum MacroCall {
    Assert,
}

impl MacroCall {
    fn from_chain(chain: &NameAccessChain) -> Self {
        match &chain.value {
            NameAccessChain_::One(name) => Self::from_symbol(name.value),
            NameAccessChain_::Two(_, _) => unreachable!(),
            NameAccessChain_::Three(_, _) => unreachable!(),
        }
    }
    fn from_symbol(s: Symbol) -> Self {
        match s.as_str() {
            "assert" => Self::Assert,
            _ => unreachable!(),
        }
    }
}

/// Visit scopes for inner to outer.
pub trait ScopeVisitor {
    /// Handle this item.
    /// If `should_finish` return true. All `enter_scope` and enter_scope called function will return.
    fn handle_item(&mut self, scopes: &Scopes, item: &Item);
    /// Need not visit this structure???
    fn file_should_visit(&self, p: FileHash) -> bool;
    /// Visitor should finished.
    fn finished(&self) -> bool;
}

#[derive(Clone, Debug)]
enum ResolvedType_ {
    UnKnown,
    Struct(Name, Vec<StructTypeParameter>, Vec<(Field, ResolvedType)>),
    /// struct { ... }
    BuildInType(BuildInType),
    /// T : drop
    TParam(Name, Vec<Ability>),
    ApplyTParam(
        Box<ResolvedType>,
        /* two field copied from TParam  */ Name,
        Vec<Ability>,
    ),
    /// & mut ...
    Ref(bool, Box<ResolvedType>),
    /// ()
    Unit,
    /// (t1, t2, ... , tn)
    /// Used for return values and expression blocks
    Multiple(Vec<ResolvedType>),
    Fun(
        Vec<(Name, Vec<Ability>)>, // type parameters.
        Vec<ResolvedType>,         // parameters.
        Box<ResolvedType>,         // return type.
    ),
    Vec(Box<ResolvedType>),
    /// Can't resolve the Type,Keep the ast type.
    ResolvedFailed(Type_),
}

#[derive(Clone, Debug)]
pub struct ResolvedType(Spanned<ResolvedType_>);
impl ResolvedType {
    fn nth_ty(&self, index: usize) -> Option<&'_ ResolvedType> {
        match &self.0.value {
            ResolvedType_::Multiple(x) => x.get(index),
            ResolvedType_::Struct(_, _, fields) => fields.get(index).map(|x| &x.1),
            _ => None,
        }
    }
    fn is_vector(&self) -> Option<&'_ ResolvedType> {
        match &self.0.value {
            ResolvedType_::Vec(x) => Some(x.as_ref()),
            _ => None,
        }
    }

    fn find_filed_by_name(&self, name: Symbol) -> Option<&'_ (Field, ResolvedType)> {
        match &self.0.value {
            ResolvedType_::Struct(_, _, fields) => {
                for f in fields.iter() {
                    if f.0.value() == name {
                        return Some(f);
                    }
                }
                None
            }
            _ => None,
        }
    }
    #[inline]
    const fn new_unknown(loc: Loc) -> ResolvedType {
        Self(Spanned {
            loc,
            value: ResolvedType_::UnKnown,
        })
    }
    fn new_multi(loc: Loc, one: ResolvedType, num: usize) -> Self {
        Self(Spanned {
            loc,
            value: ResolvedType_::Multiple((0..num).map(|_| one.clone()).collect()),
        })
    }
    #[inline]
    fn new_unit(loc: Loc) -> Self {
        Self(Spanned {
            loc,
            value: ResolvedType_::Unit,
        })
    }
    #[inline]
    fn new_build_in(b: BuildInType) -> Self {
        Self(Spanned {
            loc: UNKNOWN_LOC.clone(),
            value: ResolvedType_::Unit,
        })
    }
    #[inline]
    fn is_unknown(&self) -> bool {
        match &self.0.value {
            ResolvedType_::UnKnown => true,
            _ => false,
        }
    }
    #[inline]
    fn is_resolved_failed(&self) -> bool {
        match &self.0.value {
            ResolvedType_::ResolvedFailed(_) => true,
            _ => false,
        }
    }

    #[inline]
    fn new_ref(loc: Loc, is_mut: bool, e: ResolvedType) -> Self {
        let value = ResolvedType_::Ref(is_mut, Box::new(e));
        Self(Spanned { loc, value })
    }

    #[inline]
    fn is_err(&self) -> bool {
        self.is_resolved_failed() || self.is_unknown()
    }
    fn is_tparam(&self) -> bool {
        match &self.0.value {
            ResolvedType_::TParam(_, _) => true,
            _ => false,
        }
    }
    #[inline]
    fn is_fun(&self) -> bool {
        match &self.0.value {
            ResolvedType_::Fun(_, _, _) => true,
            _ => false,
        }
    }

    /// bind type parameter to concrete tpe
    fn bind_type_parameter(&mut self, types: &HashMap<Symbol, ResolvedType>) {
        match &mut self.0.value {
            ResolvedType_::UnKnown => {}
            ResolvedType_::Struct(_, _, ref mut fields) => {
                for i in 0..fields.len() {
                    let mut t = fields.get_mut(i).unwrap();
                    t.1.bind_type_parameter(types);
                }
            }
            ResolvedType_::BuildInType(_) => {}
            ResolvedType_::TParam(name, _) => {
                if let Some(x) = types.get(&name.value) {
                    std::mem::replace(&mut self.0.value, (*x).clone().0.value);
                }
            }
            ResolvedType_::Ref(_, ref mut b) => {
                b.as_mut().bind_type_parameter(types);
            }
            ResolvedType_::Unit => {}
            ResolvedType_::Multiple(ref mut xs) => {
                for i in 0..xs.len() {
                    let mut t = xs.get_mut(i).unwrap();
                    t.bind_type_parameter(types);
                }
            }
            ResolvedType_::Fun(_, ref mut xs, ref mut ret) => {
                for i in 0..xs.len() {
                    let mut t = xs.get_mut(i).unwrap();
                    t.bind_type_parameter(types);
                }
                ret.as_mut().bind_type_parameter(types);
            }
            ResolvedType_::Vec(ref mut b) => {
                b.as_mut().bind_type_parameter(types);
            }
            ResolvedType_::ResolvedFailed(_) => {}
            ResolvedType_::ApplyTParam(_, _, _) => {
                unreachable!("called multiple times.")
            }
        }
    }
}

#[derive(Clone, Debug, Copy)]
pub enum BuildInType {
    Bool,
    U8,
    U64,
    U128,
    Address,
    /// A number type from literal.
    /// Could be u8 and ... depend on How it is used.
    NumType,

    /// https://move-book.com/advanced-topics/managing-collections-with-vectors.html?highlight=STring#hex-and-bytestring-literal-for-inline-vector-definitions
    String,
}

impl BuildInType {
    fn from_symbol(s: Symbol) -> Self {
        match s.as_str() {
            "u8" => Self::U8,
            "u64" => Self::U64,
            "u128" => Self::U128,
            "bool" => Self::Bool,
            "address" => Self::Address,
            _ => unreachable!(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Item {
    /////////////////////////////
    /// VALUE types
    Parameter(Var, ResolvedType),
    ImportedUseModule(ModuleIdent, Rc<RefCell<Scope>>),
    ImportUse(Box<Item>),

    /////////////////////////
    /// TYPE types
    Struct(Name, Vec<StructTypeParameter>, Vec<(Field, ResolvedType)>),
    /// build in types.
    BuildInType(BuildInType),
    /// Here are all definition.
    TParam(Name, Vec<Ability>),

    ////////////////////////////////
    /// various access types.
    // A type apply.
    ApplyType(Type),
    Use(UseDecl),
    ExprVar(Var),
    NameAccessChain(NameAccessChain),
    // Maybe the same as ExprName.
    ExprAddressName(Name),
    FieldInitialization(Field, ResolvedType /*  field type */),
    AccessFiled(Field, ResolvedType /*  field type */),
    ///////////////
    /// key words
    KeyWords(&'static str),
    /////////////////
    /// Marco call
    MacroCall(MacroCall),
}

impl Item {
    ///
    fn to_type(&self) -> Option<ResolvedType> {
        let (loc, x) = match self {
            Item::TParam(name, ab) => (name.loc, ResolvedType_::TParam(name.clone(), ab.clone())),
            Item::Struct(name, types, fields) => (
                name.loc,
                ResolvedType_::Struct(name.clone(), types.clone(), fields.clone()),
            ),
            Item::BuildInType(b) => (UNKNOWN_LOC, ResolvedType_::BuildInType(*b)),
            _ => return None,
        };

        Some(ResolvedType(Spanned { loc, value: x }))
    }
}

#[derive(Debug)]
pub struct IDEModule {
    defs: HashMap<
        PathBuf, /*  file path  xxxx/abc.move  */
        move_compiler::parser::ast::Definition,
    >,
    filepath_to_filehash: HashMap<String /* file path */, FileHash>,
}

#[derive(Clone)]
pub struct Scopes {
    scopes: Rc<RefCell<Vec<Scope>>>,
}

impl Scopes {
    fn new() -> Self {
        let x = Scopes {
            scopes: Default::default(),
        };
        let s = Scope::new_top();
        x.scopes.as_ref().borrow_mut().push(s);
        x
    }

    fn enter_build_in(&self) {
        self.scopes
            .as_ref()
            .borrow_mut()
            .first_mut()
            .unwrap()
            .enter_build_in();
    }

    fn enter_scope<R>(&self, call_back: impl FnOnce(&Scopes) -> R) -> R {
        let s = Scope::default();
        self.scopes.as_ref().borrow_mut().push(s);
        let _guard = ScopesGuarder::new(self.clone());
        let r = call_back(self);
        r
    }

    // Enter
    fn enter_item(&self, name: Symbol, item: Item) {
        self.scopes
            .as_ref()
            .borrow_mut()
            .last_mut()
            .unwrap()
            .enter_item(name, item);
    }

    fn enter_top_item(
        &self,
        address: NumericalAddress,
        module: Symbol,
        item_name: Symbol,
        item: Item,
    ) {
        let mut b = self.scopes.as_ref().borrow_mut();
        let mut s = b.first_mut().unwrap();
        if s.addresses.is_none() {
            s.addresses = Some(Addresses::new());
        };
        let t = s.addresses.as_mut().unwrap();
        if !t.address.contains_key(&address) {
            t.address.insert(address, Default::default());
        }
        if !t
            .address
            .get(&address)
            .unwrap()
            .modules
            .contains_key(&module)
        {
            t.address
                .get_mut(&address)
                .unwrap()
                .modules
                .insert(module, Default::default());
        }
        // finally, OK to borrow.
        t.address
            .get_mut(&address)
            .unwrap()
            .modules
            .get_mut(&module)
            .unwrap()
            .as_ref()
            .borrow_mut()
            .borrow_mut()
            .items
            .insert(item_name, item);
    }

    /// Visit all scope from inner to outer.
    fn inner_first_visit(&self, mut visitor: impl FnMut(&Scope) -> bool /*  stop??? */) {
        for s in self.scopes.as_ref().borrow().iter().rev() {
            if visitor(s) {
                return;
            }
        }
    }

    ///
    fn setup_scope(&self, f: impl FnOnce(&mut Scope)) {
        let mut x = self.scopes.as_ref().borrow_mut();
        let x = x.last_mut().unwrap();
        f(x);
    }

    fn under_function(&self) -> Option<()> {
        let mut r = None;
        self.inner_first_visit(|s| {
            if s.is_function {
                r = Some(());
                return true;
            }
            false
        });
        r
    }

    fn under_spec(&self) -> Option<()> {
        let mut r = None;
        self.inner_first_visit(|s| {
            if s.is_spec {
                r = Some(());
                return true;
            }
            false
        });
        r
    }

    fn resolve_name_access_chain_type(&self, chain: &NameAccessChain) -> ResolvedType {
        let failed = ResolvedType::new_unknown(chain.loc);

        let scopes = self.scopes.as_ref().borrow();
        for s in scopes.iter() {
            // We must be in global scope.
            let item = match &chain.value {
                NameAccessChain_::One(x) => {
                    if let Some(item) = s.items.get(&x.value) {
                        return item.to_type().unwrap_or(failed);
                    }
                }
                NameAccessChain_::Two(_, _) => todo!(),
                NameAccessChain_::Three(_, _) => todo!(),
            };
        }
        // make sure return a false type.
        return ResolvedType(Spanned {
            loc: chain.loc,
            value: ResolvedType_::ResolvedFailed(Type_::Apply(Box::new(chain.clone()), vec![])),
        });
    }

    fn find_var_type(&self, name: Symbol) -> ResolvedType {
        let mut ret = None;
        self.inner_first_visit(|s| {
            if let Some(v) = s.items.get(&name) {
                match v {
                    Item::Parameter(_, t) => {
                        ret = Some(t.clone());
                        return true;
                    }
                    _ => {}
                }
            };
            false
        });
        return ResolvedType(Spanned {
            loc: UNKNOWN_LOC,
            value: ResolvedType_::UnKnown,
        });
    }

    fn find_name_access_chain_type<'a>(
        &self,
        chain: &NameAccessChain,
        name_to_addr: impl Fn(Symbol) -> &'a NumericalAddress,
    ) -> ResolvedType {
        let failed = ResolvedType::new_unknown(chain.loc);
        match &chain.value {
            NameAccessChain_::One(name) => {
                let mut r = None;
                self.inner_first_visit(|s| {
                    if let Some(v) = s.items.get(&name.value) {
                        r = v.to_type();
                        if r.is_some() {
                            return true;
                        }
                    }
                    false
                });
                r.unwrap_or(failed)
            }
            NameAccessChain_::Two(name, member) => {
                // first find this name.
                let mut r = None;
                self.inner_first_visit(|s| {
                    if let Some(v) = s.items.get(&member.value) {
                        r = Some(v.clone());
                        return true;
                    }
                    false
                });

                if r.is_none() {
                    return failed;
                }
                let r = r.unwrap();
                match r {
                    Item::ImportedUseModule(_, members) => {
                        if let Some(item) = members.as_ref().borrow().items.get(&member.value) {
                            item.to_type().unwrap_or(failed)
                        } else {
                            failed
                        }
                    }
                    _ => failed,
                }
            }
            NameAccessChain_::Three(chain_two, member) => self.visit_top_scope(|top| {
                let modules = top.address.get(match &chain_two.value.0.value {
                    LeadingNameAccess_::AnonymousAddress(x) => x,
                    LeadingNameAccess_::Name(name) => name_to_addr(name.value),
                });
                if modules.is_none() {
                    return failed;
                }
                let modules = modules.unwrap();

                let module = modules.modules.get(&chain_two.value.1.value);
                if module.is_none() {
                    return failed;
                }
                let module = module.unwrap();
                if let Some(item) = module.as_ref().borrow().items.get(&member.value) {
                    item.to_type().unwrap_or(failed)
                } else {
                    failed
                }
            }),
        }
    }

    fn visit_top_scope<R>(&self, x: impl FnOnce(&Addresses) -> R) -> R {
        x(self
            .scopes
            .as_ref()
            .borrow()
            .first()
            .unwrap()
            .addresses
            .as_ref()
            .unwrap())
    }

    fn resolve_type(&self, ty: &Type) -> ResolvedType {
        let r = match &ty.value {
            Type_::Apply(ref chain, types) => {
                let chain_ty = self.resolve_name_access_chain_type(chain);
                let _types: Vec<_> = types.iter().map(|ty| self.resolve_type(ty)).collect();
                return chain_ty;
            }
            Type_::Ref(m, ref b) => ResolvedType_::Ref(*m, Box::new(self.resolve_type(b.as_ref()))),
            Type_::Fun(ref parameters, ref ret) => {
                let parameters: Vec<_> = parameters.iter().map(|v| self.resolve_type(v)).collect();
                let ret = self.resolve_type(ret.as_ref());
                ResolvedType_::Fun(vec![], parameters, Box::new(ret))
            }
            Type_::Unit => ResolvedType_::Unit,
            Type_::Multiple(ref types) => {
                let types: Vec<_> = types.iter().map(|v| self.resolve_type(v)).collect();
                ResolvedType_::Multiple(types)
            }
        };
        ResolvedType(Spanned {
            loc: ty.loc,
            value: r,
        })
    }
}

/// RAII type pop on `enter_scope`.
struct ScopesGuarder(Scopes);

impl ScopesGuarder {
    fn new(s: Scopes) -> Self {
        Self(s)
    }
}

impl Drop for ScopesGuarder {
    fn drop(&mut self) {
        self.0.scopes.as_ref().borrow_mut().pop().unwrap();
    }
}

#[test]
fn xxxx() {
    let s = Scopes::new();
    s.enter_scope(|s| s.enter_scope(|s| s.enter_scope(|_| {})));
}

const UNKNOWN_TYPE: ResolvedType = ResolvedType::new_unknown(Loc::new(FileHash::empty(), 0, 0));

const UNKNOWN_LOC: Loc = Loc::new(FileHash::empty(), 0, 0);

/// Double way mapping from FileHash and FilePath.
#[derive(Debug, Default)]
struct PathBufHashMap {
    path_2_hash: HashMap<PathBuf, FileHash>,
    hash_2_path: HashMap<FileHash, PathBuf>,
}

impl PathBufHashMap {
    fn update(&mut self, path: PathBuf, hash: FileHash) {
        if let Some(hash) = self.path_2_hash.get(&path) {
            self.hash_2_path.remove(&hash);
        }
        self.path_2_hash.insert(path.clone(), hash.clone());
        self.hash_2_path.insert(hash, path);
    }

    fn get_hash(&self, path: &PathBuf) -> Option<&'_ FileHash> {
        self.path_2_hash.get(path)
    }
    fn get_path(&self, hash: &FileHash) -> Option<&'_ PathBuf> {
        self.hash_2_path.get(hash)
    }
}

fn get_name_from_value(v: &Value) -> Option<&Name> {
    match &v.value {
        Value_::Address(ref x) => match &x.value {
            LeadingNameAccess_::AnonymousAddress(_) => None,
            LeadingNameAccess_::Name(ref name) => Some(name),
        },
        _ => None,
    }
}

fn infer_type_on_expression(
    ret: &mut HashMap<Symbol, ResolvedType>,
    type_parameters: &Vec<(Name, Vec<Ability>)>,
    parameters: &Vec<ResolvedType>,
    expression_types: &Vec<ResolvedType>,
) {
    for (index, p) in parameters.iter().enumerate() {
        if let Some(expr_type) = expression_types.get(index) {
            bind(ret, p, expr_type);
        } else {
            break;
        }
    }
    fn bind(
        ret: &mut HashMap<Symbol, ResolvedType>,
        // may be a type have type parameter.
        parameter_type: &ResolvedType,
        // a type that is certain.
        expr_type: &ResolvedType,
    ) {
        match &parameter_type.0.value {
            ResolvedType_::UnKnown => {}
            ResolvedType_::Struct(_, _, fields) => match &expr_type.0.value {
                ResolvedType_::Struct(_, _, fields2) => {
                    for (l, r) in fields.iter().zip(fields2.iter()) {
                        bind(ret, &l.1, &r.1);
                    }
                }
                _ => {}
            },
            ResolvedType_::BuildInType(_) => {}
            ResolvedType_::TParam(name, _) => {
                ret.insert(name.value, expr_type.clone());
            }
            ResolvedType_::ApplyTParam(_, _, _) => {}
            ResolvedType_::Ref(_, l) => match &expr_type.0.value {
                ResolvedType_::Ref(_, r) => bind(ret, l.as_ref(), r.as_ref()),
                _ => {}
            },
            ResolvedType_::Unit => {}
            ResolvedType_::Multiple(x) => match &expr_type.0.value {
                ResolvedType_::Multiple(y) => {
                    for (index, l) in x.iter().enumerate() {
                        if let Some(r) = y.get(index) {
                            bind(ret, l, r);
                        } else {
                            break;
                        }
                    }
                }
                _ => {}
            },
            /// function is not expression
            ResolvedType_::Fun(_, parameters, _) => {}
            ResolvedType_::Vec(x) => match &expr_type.0.value {
                ResolvedType_::Vec(y) => {
                    bind(ret, x.as_ref(), y.as_ref());
                }
                _ => {}
            },
            ResolvedType_::ResolvedFailed(_) => {}
        }
    }
}
