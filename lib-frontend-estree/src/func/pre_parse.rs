use super::constraint;
use super::undoable_hash_map::UndoableHashMap;
use super::varusage;
use super::varusage::Usage;
use super::ParseProgramError;
use super::ProgramPreExports;
use crate::attributes::NodeForEachWithAttributes;
use crate::attributes::NodeForEachWithAttributesMut;
use crate::estree::SourceLocation as esSL;
use crate::estree::*;
use crate::extensions::IntoSourceLocation;
use crate::frontendvar::*;
use projstd::log::CompileMessage;
use std::collections::BTreeMap;
use std::collections::HashSet;
use std::{collections::HashMap, usize};

/**
 * Pre-parse an ESTree program
 * Decide where every ir local should be declared,
 * and whether they need to be address-taken (i.e. put in the heap).
 * Also detect duplicate variable detection in the same scope; if so, raises an error.
 *
 * Note: import_ctx contains x elements, where x is the number of imports detected in the dep_graph step, in order;
 * and each element is a hash map from name to the imported prevar (which must be a global, i.e. prevar.depth == 0).
 *
 * start_idx is the number of existing globals already declared; new Target globals in this es_program should be assigned a VarLocId that starts from start_idx onwards.
 *
 * name_ctx contains all pre-declared Source names (effectively an auto-import of everything), but are considered to be pre-validated in a separate validation context,
 * and so new variables of the same name will shadow them without any error.
 * name_ctx may be modified, but must be returned to its original state before the function returns (this allows the frontend to have good time complexity guarantees).
 *
 * Returns the hash map of exported names.
 * At the top-level, we don't care about the usage of variables (because they are all globals anyway)
 */
pub fn pre_parse_program(
    es_program: &mut Program,
    _loc: &Option<esSL>,
    name_ctx: &mut HashMap<String, PreVar>, // pre-declared Source names
    import_ctx: &[&ProgramPreExports], // all prevars here must be globals, i.e. have depth == 0
    /* depth: usize */ // not needed, implied to be 0
    start_idx: &mut usize, // the number of (global) variables
    filename: Option<&str>,
) -> Result<ProgramPreExports, CompileMessage<ParseProgramError>> {
    // Extracts both Targets and Directs.  Will return Err if any declaration (either Target or Direct) is considered to be duplicate.
    // will also annotate any LHS identifiers with the target index
    // an imported name does not get a new prevar; it retains the old one instead (so there is no overhead in IR to calling a function or using a variable across a module boundary)
    let (curr_decls, exports): (Vec<(String, PreVar)>, ProgramPreExports) =
        validate_and_extract_imports_and_decls(&es_program.body, import_ctx, start_idx, filename)?;

    let undo_ctx = name_ctx.add_scope(curr_decls);

    let mut direct_funcs = Vec::new();

    es_program
        .body
        .each_with_attributes_mut(filename, |es_node, attr| {
            let usages =
                pre_parse_statement(es_node, attr, name_ctx, &mut direct_funcs, 0, filename)?;
            assert!(
                usages.is_empty(),
                "Global variable got returned as a Usage, this is a bug"
            );
            Ok(())
        })?;

    es_program.direct_funcs = direct_funcs;

    name_ctx.remove_scope(undo_ctx);

    Ok(exports)
}

/**
 * Pre-parse an ESTree node block content
 * Decide where every ir local should be declared,
 * and whether they need to be address-taken (i.e. put in the heap).
 * Also detect duplicate variable detection in the same scope; if so, raises an error.
 *
 * name_ctx may be modified, but must be returned to its original state before the function returns (this allows the frontend to have good time complexity guarantees).
 */
fn pre_parse_block_statement(
    es_block: &mut BlockStatement,
    _loc: &Option<esSL>,
    name_ctx: &mut HashMap<String, PreVar>, // contains all names referenceable from outside the current sequence
    /*deps: &[&HashMap<String, PreVar>],*/
    /*order: usize,*/
    depth: usize,
    filename: Option<&str>,
) -> Result<BTreeMap<VarLocId, Usage>, CompileMessage<ParseProgramError>> {
    let new_depth = depth + 1;

    // Extracts both Targets and Directs.  Will return Err if any declaration (either Target or Direct) is considered to be duplicate.
    // will also annotate any LHS identifiers with the target index
    let curr_decls: Vec<(String, PreVar)> =
        validate_and_extract_decls(&es_block.body, new_depth, &mut 0, filename)?;

    let undo_ctx = name_ctx.add_scope(curr_decls);

    let mut ret_usages: BTreeMap<VarLocId, Usage> = BTreeMap::new();

    let mut direct_funcs = Vec::new();

    es_block
        .body
        .each_with_attributes_mut(filename, |es_node, attr| {
            let usages = pre_parse_statement(
                es_node,
                attr,
                name_ctx,
                &mut direct_funcs,
                new_depth,
                filename,
            )?;
            let tmp = std::mem::take(&mut ret_usages); // necessary because of weird borrow rules in Rust
            ret_usages = varusage::merge_series(tmp, usages);
            Ok(())
        })?;

    es_block.direct_funcs = direct_funcs;

    es_block.address_taken_vars = split_off_address_taken_vars(&mut ret_usages, new_depth);

    name_ctx.remove_scope(undo_ctx);

    Ok(ret_usages)
}

/**
 * Like pre_parse_block_statement, but with additional variables (function parameters) prepended, and will translate the result
 */
fn pre_parse_function<F: Function + Scope>(
    es_func: &mut F,
    _loc: &Option<esSL>,
    name_ctx: &mut HashMap<String, PreVar>, // contains all names referenceable from outside the current sequence
    depth: usize,
    filename: Option<&str>,
) -> Result<BTreeMap<VarLocId, Usage>, CompileMessage<ParseProgramError>> {
    let new_depth = depth + 1;

    let (params, body) = es_func.params_body_mut();

    // Extracts both targets and decls.  Will return Err if any declaration (either Target or Direct) is considered to be duplicate.
    // will also annotate any LHS identifiers with the target index
    let curr_params: Vec<(String, VarLocId)> =
        validate_and_extract_params(params, new_depth, filename)?;

    // state that all the params are used and modified
    let mut ret_usages: BTreeMap<VarLocId, Usage> = curr_params
        .iter()
        .map(|(_, varlocid)| (*varlocid, Usage::UsedAndModified))
        .collect();

    let mut curr_decls: Vec<(String, PreVar)> = curr_params
        .into_iter()
        .map(|(name, varlocid)| (name, PreVar::Target(varlocid)))
        .collect();

    let undo_ctx = if let Node {
        loc: _,
        kind: NodeKind::BlockStatement(es_block),
    } = body
    {
        // it is a normal function body (not the kind of ArrowFunctionExpression that just has an expression)
        // so we do things similar to pre_parse_block_statement, but not that the params are logically 'in the same block' as the params for depth and duplicate declaration purposes

        // add in the variables declared in this block
        // todo! This is a bug, validating function-level decls should be done in the same validation context as the parameters, as mandated by JavaScript.
        curr_decls.append(&mut validate_and_extract_decls(
            &es_block.body,
            new_depth,
            &mut params.len(),
            filename,
        )?);

        let undo_ctx = name_ctx.add_scope(curr_decls);

        let mut direct_funcs = Vec::new();

        es_block
            .body
            .each_with_attributes_mut(filename, |es_node, attr| {
                let usages = pre_parse_statement(
                    es_node,
                    attr,
                    name_ctx,
                    &mut direct_funcs,
                    new_depth,
                    filename,
                )?;
                let tmp = std::mem::take(&mut ret_usages); // necessary because of weird borrow rules in Rust
                ret_usages = varusage::merge_series(tmp, usages);
                Ok(())
            })?;

        *es_func.direct_funcs_mut() = direct_funcs;

        undo_ctx
    } else {
        // it is just an expression, and it should be interpreted as 'return <expr>;'

        let undo_ctx = name_ctx.add_scope(curr_decls);

        // no variables to add, since it is just a return expr
        let usages = match pre_parse_expr(body, name_ctx, new_depth, filename)? {
            MultipleOrSingleBTreeMap::BTreeMap(usages) => usages,
            _ => unreachable!(),
        };
        ret_usages = varusage::merge_series(ret_usages, usages);

        undo_ctx
    };

    // note: we don't use the address_taken_vars field of this es_block, even if it is a block
    // we use the one from the function instead

    *es_func.address_taken_vars_mut() = split_off_address_taken_vars(&mut ret_usages, new_depth);

    name_ctx.remove_scope(undo_ctx);

    *es_func.captured_vars_mut() = clone_varusages(&ret_usages);

    Ok(varusage::wrap_closure(ret_usages))
}

fn split_off_address_taken_vars(
    ret_usages: &mut BTreeMap<VarLocId, Usage>,
    depth: usize,
) -> Vec<usize> {
    ret_usages
        .split_off(&VarLocId {
            depth: depth,
            index: 0,
        })
        .into_iter()
        .filter_map(|(varlocid, usage)| {
            if usage == Usage::AddressTaken {
                Some(varlocid.index)
            } else {
                None
            }
        })
        .collect()
}

fn clone_varusages(ret_usages: &BTreeMap<VarLocId, Usage>) -> Vec<VarLocId> {
    ret_usages.iter().map(|(varlocid, _)| *varlocid).collect()
}

fn pre_parse_statement(
    es_node: &mut Node,
    attr: HashMap<String, Option<String>>,
    name_ctx: &mut HashMap<String, PreVar>, // contains all names referenceable from outside the current sequence
    direct_funcs: &mut Vec<(String, Box<[ir::VarType]>)>,
    /*deps: &[&HashMap<String, PreVar>],*/
    /*order: usize,*/
    depth: usize,
    filename: Option<&str>,
) -> Result<BTreeMap<VarLocId, Usage>, CompileMessage<ParseProgramError>> {
    let mut is_direct = false;
    let mut constraint_str: Option<String> = None;
    for (key, opt_val) in attr.into_iter() {
        match key.as_str() {
            "direct" => {
                if opt_val != None {
                    return Err(CompileMessage::new_error(
                        es_node.loc.into_sl(filename).to_owned(),
                        ParseProgramError::AttributeContentError(
                            "The 'direct' attribute on this statement cannot have a value",
                        ),
                    ));
                }
                is_direct = true;
            }
            "constraint" => {
                if opt_val == None {
                    return Err(CompileMessage::new_error(
                        es_node.loc.into_sl(filename).to_owned(),
                        ParseProgramError::AttributeContentError(
                            "The 'constraint' attribute on this statement must have a value",
                        ),
                    ));
                }
                constraint_str = opt_val;
            }
            _ => {
                return Err(CompileMessage::new_error(
                    es_node.loc.into_sl(filename).to_owned(),
                    ParseProgramError::AttributeUnrecognizedError(key),
                ));
            }
        }
    }

    // 'constraint' attribute can only appear with 'direct'
    if !is_direct && constraint_str != None {
        return Err(CompileMessage::new_error(
            es_node.loc.into_sl(filename).to_owned(),
            ParseProgramError::AttributeContentError("The 'constraint' attribute on this statement cannot appear without the 'direct' attribute")));
    }

    // 'is_direct' can only appear on FunctionDeclaration
    if is_direct {
        return if let NodeKind::FunctionDeclaration(func_decl) = &mut es_node.kind {
            pre_parse_direct_func_decl(
                func_decl,
                constraint_str,
                &es_node.loc,
                name_ctx,
                direct_funcs,
                depth,
                filename,
            )
        // a direct function declaration does not incur any usages
        } else {
            Err(CompileMessage::new_error(
                es_node.loc.into_sl(filename).to_owned(),
                ParseProgramError::AttributeContentError(
                    "The 'direct' attribute can only appear on FunctionDeclaration",
                ),
            ))
        };
    }

    match &mut es_node.kind {
        NodeKind::ExpressionStatement(stmt) => {
            pre_parse_expr_statement(stmt, &es_node.loc, name_ctx, depth, filename)
        }
        NodeKind::BlockStatement(block) => {
            pre_parse_block_statement(block, &es_node.loc, name_ctx, depth, filename)
        }
        NodeKind::ReturnStatement(stmt) => {
            pre_parse_return_statement(stmt, &es_node.loc, name_ctx, depth, filename)
        }
        NodeKind::IfStatement(stmt) => {
            pre_parse_if_statement(stmt, &es_node.loc, name_ctx, depth, filename)
        }
        NodeKind::FunctionDeclaration(func_decl) => {
            pre_parse_func_decl(func_decl, &es_node.loc, name_ctx, depth, filename)
        }
        NodeKind::VariableDeclaration(var_decl) => {
            pre_parse_var_decl(var_decl, &es_node.loc, name_ctx, depth, filename)
        }
        NodeKind::ArrayExpression(_) => {
            unreachable!()
        }
        NodeKind::EmptyStatement(_) => Ok(BTreeMap::new()), // EmptyStatement does not use any variables
        NodeKind::DebuggerStatement(_)
        | NodeKind::WithStatement(_)
        | NodeKind::LabeledStatement(_)
        | NodeKind::BreakStatement(_)
        | NodeKind::ContinueStatement(_) => Err(CompileMessage::new_error(
            es_node.loc.into_sl(filename).to_owned(),
            ParseProgramError::ESTreeError("This statement type is not allowed"),
        )),
        node_kind => {
            // if we are at global scope, we accept Import and Export declarations (but don't do anything with them)
            // because the caller would have already mapped them
            if depth == 0 {
                match node_kind {
                    NodeKind::ImportDeclaration(import_decl) => {
                        pre_parse_import_decl(import_decl, &es_node.loc, name_ctx, filename)?;
                        Ok(BTreeMap::new())
                    }
                    NodeKind::ExportNamedDeclaration(export_decl) => {
                        pre_parse_export_decl(export_decl, &es_node.loc, name_ctx, filename)?;
                        Ok(BTreeMap::new())
                    }
                    _ => Err(CompileMessage::new_error(
                        es_node.loc.into_sl(filename).to_owned(),
                        ParseProgramError::ESTreeError(
                            "Statement, Import or Export node expected at top-level",
                        ),
                    )),
                }
            } else {
                Err(CompileMessage::new_error(
                    es_node.loc.into_sl(filename).to_owned(),
                    ParseProgramError::ESTreeError("Statement node expected in a BlockStatement"),
                ))
            }
        }
    }
}

fn pre_parse_expr_statement(
    es_expr_stmt: &mut ExpressionStatement,
    _loc: &Option<esSL>,
    name_ctx: &mut HashMap<String, PreVar>, // contains all names referenceable from outside the current sequence
    depth: usize,
    filename: Option<&str>,
) -> Result<BTreeMap<VarLocId, Usage>, CompileMessage<ParseProgramError>> {
    // we have to detect the AssignmentExpression here, since in Source AssignmentExpression is not allowed to be nested.
    let es_expr_node: &mut Node = &mut *es_expr_stmt.expression;
    if let NodeKind::AssignmentExpression(AssignmentExpression {
        operator,
        left,
        right,
    }) = &mut es_expr_node.kind
    {
        match operator.as_str() {
            "=" => match &mut **left {
                Node {
                    loc: _,
                    kind: NodeKind::Identifier(Identifier { name, prevar }),
                } => {
                    let rhs_expr = match pre_parse_expr(&mut **right, name_ctx, depth, filename)? {
                        MultipleOrSingleBTreeMap::BTreeMap(rhs_expr) => rhs_expr,
                        _ => unreachable!(),
                    };
                    let resvar = *name_ctx.get(name.as_str()).unwrap();
                    assert!(*prevar == Some(resvar)); // they should already have a prevar attached
                                                      // note: this is probably a bug, they would not have prevar attached yet...
                    let varlocid = match resvar {
                        PreVar::Target(varlocid) => varlocid,
                        PreVar::Direct => panic!("ICE: Should be VarLocId"),
                    };
                    if varlocid.depth == 0 {
                        // it is a global variable, but don't do anything because it doesn't count as a usage
                        Ok(rhs_expr)
                    } else {
                        // it's not a global variable
                        // say that we used this variable
                        // the RHS comes first because the RHS is evaluated first before doing the actual assignment
                        Ok(varusage::merge_series(
                            rhs_expr,
                            varusage::from_modified(varlocid),
                        ))
                    }
                }
                Node { loc, kind: _ } => Err(CompileMessage::new_error(
                    loc.into_sl(filename).to_owned(),
                    ParseProgramError::ESTreeError(
                        "Expected ESTree Identifier at LHS of AssignmentExpression",
                    ),
                )),
            },
            _ => Err(CompileMessage::new_error(
                es_expr_node.loc.into_sl(filename).to_owned(),
                ParseProgramError::SourceRestrictionError(
                    "Compound assignment operator not allowed",
                ),
            )),
        }
    } else {
        let ret = match pre_parse_expr(es_expr_node, name_ctx, depth, filename)? {
            MultipleOrSingleBTreeMap::BTreeMap(ret) => ret,
            _ => unreachable!(),
        };
        Ok(ret)
    }
}

fn pre_parse_return_statement(
    es_return: &mut ReturnStatement,
    loc: &Option<esSL>,
    name_ctx: &mut HashMap<String, PreVar>, // contains all names referenceable from outside the current sequence
    depth: usize,
    filename: Option<&str>,
) -> Result<BTreeMap<VarLocId, Usage>, CompileMessage<ParseProgramError>> {
    if let Some(box_node) = &mut es_return.argument {
        let ret = match pre_parse_expr(&mut *box_node, name_ctx, depth, filename)? {
            MultipleOrSingleBTreeMap::BTreeMap(ret) => ret,
            _ => unreachable!(),
        };
        Ok(ret)
    } else {
        Err(CompileMessage::new_error(
            loc.into_sl(filename).to_owned(),
            ParseProgramError::SourceRestrictionError("Return statement must have a value"),
        ))
    }
}

fn pre_parse_if_statement(
    es_if: &mut IfStatement,
    loc: &Option<esSL>,
    name_ctx: &mut HashMap<String, PreVar>, // contains all names referenceable from outside the current sequence
    depth: usize,
    filename: Option<&str>,
) -> Result<BTreeMap<VarLocId, Usage>, CompileMessage<ParseProgramError>> {
    if let NodeKind::BlockStatement(es_true_block) = &mut es_if.consequent.kind {
        if let Some(es_false_node) = &mut es_if.alternate {
            if let NodeKind::BlockStatement(es_false_block) = &mut es_false_node.kind {
                let first = match pre_parse_expr(&mut *es_if.test, name_ctx, depth, filename)? {
                    MultipleOrSingleBTreeMap::BTreeMap(first) => first,
                    _ => unreachable!(),
                };

                Ok(varusage::merge_series(
                    first,
                    varusage::merge_parallel(
                        pre_parse_block_statement(
                            es_true_block,
                            &es_if.consequent.loc,
                            name_ctx,
                            depth,
                            filename,
                        )?,
                        pre_parse_block_statement(
                            es_false_block,
                            &es_false_node.loc,
                            name_ctx,
                            depth,
                            filename,
                        )?,
                    ),
                ))
            } else {
                Err(CompileMessage::new_error(
                    loc.into_sl(filename).to_owned(),
                    ParseProgramError::SourceRestrictionError(
                        "Alternative of if statement must be a block",
                    ),
                ))
            }
        } else {
            Err(CompileMessage::new_error(
                loc.into_sl(filename).to_owned(),
                ParseProgramError::SourceRestrictionError(
                    "Alternative of if statement must be present",
                ),
            ))
        }
    } else {
        Err(CompileMessage::new_error(
            loc.into_sl(filename).to_owned(),
            ParseProgramError::SourceRestrictionError("Consequent of if statement must be a block"),
        ))
    }
}

/**
 * This is a normal function declaration, not the direct kind.  So it is equivalent to a const declaration.
 */
fn pre_parse_func_decl(
    es_func_decl: &mut FunctionDeclaration,
    loc: &Option<esSL>,
    name_ctx: &mut HashMap<String, PreVar>, // contains all names referenceable from outside the current sequence
    depth: usize,
    filename: Option<&str>,
) -> Result<BTreeMap<VarLocId, Usage>, CompileMessage<ParseProgramError>> {
    let rhs_expr = pre_parse_function(es_func_decl, loc, name_ctx, depth, filename)?; // parse_function will parse the function body, and transform the result using the function usage transformer.
    let prevar = match es_func_decl {
        FunctionDeclaration { id, .. } => {
            match &mut **id {
                Node {
                    loc: _,
                    kind: NodeKind::Identifier(Identifier { name, prevar }),
                } => {
                    let ret = *name_ctx.get(name.as_str()).unwrap(); // must exist since this is a LHS name, it would have been added earlier
                    *prevar = Some(ret);
                    ret
                }
                Node { loc, kind: _ } => {
                    return Err(CompileMessage::new_error(
                        loc.into_sl(filename).to_owned(),
                        ParseProgramError::ESTreeError("Expected ESTree Identifier here"),
                    ))
                }
            }
        }
    };
    let varlocid = match prevar {
        PreVar::Target(varlocid) => varlocid,
        PreVar::Direct => panic!("ICE: Should be VarLocId"),
    };
    if varlocid.depth == 0 {
        // it is a global variable, but don't do anything because it doesn't count as a usage
        Ok(rhs_expr)
    } else {
        // it's not a global variable
        // say that we used this variable
        // the RHS comes first because the RHS is evaluated first before doing the actual assignment
        Ok(varusage::merge_series(
            rhs_expr,
            varusage::from_modified(varlocid),
        ))
    }
}

fn pre_parse_direct_func_decl(
    es_func_decl: &mut FunctionDeclaration,
    constraint_str: Option<String>,
    loc: &Option<esSL>,
    name_ctx: &mut HashMap<String, PreVar>, // contains all names referenceable from outside the current sequence
    direct_funcs: &mut Vec<(String, Box<[ir::VarType]>)>,
    depth: usize,
    filename: Option<&str>,
) -> Result<BTreeMap<VarLocId, Usage>, CompileMessage<ParseProgramError>> {
    let rhs_expr = pre_parse_function(es_func_decl, loc, name_ctx, depth, filename)?; // parse_function will parse the function body, and transform the result using the function usage transformer.

    match es_func_decl {
        FunctionDeclaration { id, .. } => {
            match &mut **id {
                Node {
                    loc: _,
                    kind: NodeKind::Identifier(Identifier { name, prevar }),
                } => {
                    // register it as a direct function
                    *prevar = Some(PreVar::Direct);

                    // register this direct func in the given direct_funcs param
                    let constraints = constraint_str
                        .as_deref()
                        .map_or(Ok(HashMap::new()), |cstr| {
                            constraint::parse_constraint(cstr)
                        })
                        .map_err(|(_loc_str, reason)| {
                            CompileMessage::new_error(
                                loc.into_sl(filename).to_owned(), // this is the wrong location, but it's hard to get the correct one
                                ParseProgramError::AttributeContentError(reason),
                            )
                        })?;
                    let direct_type: Box<[ir::VarType]> = es_func_decl
                        .params
                        .iter()
                        .map(|id_node| match id_node {
                            Node {
                                loc: _,
                                kind: NodeKind::Identifier(id),
                            } => Ok(constraints
                                .get(id.name.as_str())
                                .copied()
                                .unwrap_or(ir::VarType::Any)),
                            Node { loc, kind: _ } => Err(CompileMessage::new_error(
                                loc.into_sl(filename).to_owned(),
                                ParseProgramError::ESTreeError("Expected ESTree Identifier here"),
                            )),
                        })
                        .collect::<Result<Box<[ir::VarType]>, CompileMessage<ParseProgramError>>>(
                        )?;
                    direct_funcs.push((name.clone(), direct_type));
                }
                Node { loc, kind: _ } => {
                    return Err(CompileMessage::new_error(
                        loc.into_sl(filename).to_owned(),
                        ParseProgramError::ESTreeError("Expected ESTree Identifier here"),
                    ))
                }
            }
        }
    }

    // check that there are no captured variables (globals are not in the rhs_expr)
    if !rhs_expr.is_empty() {
        return Err(CompileMessage::new_error(
            loc.into_sl(filename).to_owned(),
            ParseProgramError::DirectFunctionCaptureError,
        ));
    }

    Ok(rhs_expr)
}

fn pre_parse_var_decl(
    es_var_decl: &mut VariableDeclaration,
    _loc: &Option<esSL>,
    name_ctx: &mut HashMap<String, PreVar>, // contains all names referenceable from outside the current sequence
    depth: usize,
    filename: Option<&str>,
) -> Result<BTreeMap<VarLocId, Usage>, CompileMessage<ParseProgramError>> {
    es_var_decl
        .declarations
        .iter_mut()
        .map(|decl| {
            match &mut decl.kind {
                NodeKind::VariableDeclarator(VariableDeclarator { id, init }) => {
                    match &mut **id {
                        Node {
                            loc: _,
                            kind: NodeKind::Identifier(Identifier { name, prevar }),
                        } => {
                            if let Some(expr) = init {
                                match pre_parse_expr(expr, name_ctx, depth, filename)? {
                                    MultipleOrSingleBTreeMap::BTreeMap(rhs_expr) => {
                                        let resvar = *name_ctx.get(name.as_str()).unwrap();
                                        *prevar = Some(resvar);
                                        let varlocid = match resvar {
                                            PreVar::Target(varlocid) => varlocid,
                                            PreVar::Direct => panic!("ICE: Should be VarLocId"),
                                        };
                                        if varlocid.depth == 0 {
                                            // it is a global variable, but don't do anything because it doesn't count as a usage
                                            Ok(rhs_expr)
                                        } else {
                                            // it's not a global variable
                                            // say that we used this variable
                                            // the RHS comes first because the RHS is evaluated first before doing the actual assignment
                                            Ok(varusage::merge_series(
                                                rhs_expr,
                                                varusage::from_modified(varlocid),
                                            ))
                                        }
                                    }
                                    MultipleOrSingleBTreeMap::MultipleBtreeMap(expressions) => {
                                        // TODONIG : not appending to prevar for arrays
                                        let ret = expressions
                                            .into_iter()
                                            .map(|expr| {
                                                let resvar = *name_ctx.get(name.as_str()).unwrap();
                                                *prevar = Some(resvar);
                                                let varlocid = match resvar {
                                                    PreVar::Target(varlocid) => varlocid,
                                                    PreVar::Direct => {
                                                        panic!("ICE: Should be VarLocId")
                                                    }
                                                };
                                                if varlocid.depth == 0 {
                                                    expr
                                                } else {
                                                    varusage::merge_series(
                                                        expr,
                                                        varusage::from_modified(varlocid),
                                                    )
                                                }
                                            })
                                            .fold(None, |prev, current| match prev {
                                                Some(prev) => {
                                                    Some(varusage::merge_series(prev, current))
                                                }
                                                None => Some(current),
                                            });

                                        if let Some(ret) = ret {
                                            Ok(ret)
                                        } else {
                                            Err(CompileMessage::new_error(
                                                decl.loc.into_sl(filename).to_owned(),
                                                ParseProgramError::SourceRestrictionError(
                                                    "Unable to parse multiple BTRee Maps",
                                                ),
                                            ))
                                        }
                                    }
                                }
                            } else {
                                Err(CompileMessage::new_error(
                                    decl.loc.into_sl(filename).to_owned(),
                                    ParseProgramError::SourceRestrictionError(
                                        "Variable initializer must be present for Source",
                                    ),
                                ))
                            }
                        }
                        Node { loc, kind: _ } => Err(CompileMessage::new_error(
                            loc.into_sl(filename).to_owned(),
                            ParseProgramError::ESTreeError("Expected ESTree Identifier here"),
                        )),
                    }
                }
                _ => Err(CompileMessage::new_error(
                    decl.loc.into_sl(filename).to_owned(),
                    ParseProgramError::ESTreeError(
                        "Children of VariableDeclaration must only be VariableDeclarator",
                    ),
                )),
            }
        })
        .fold(Ok(BTreeMap::new()), |acc, curr| {
            acc.and_then(|a| Ok(varusage::merge_series(a, curr?)))
        })
}

fn pre_parse_import_decl(
    es_import_decl: &mut ImportDeclaration,
    _loc: &Option<esSL>,
    name_ctx: &mut HashMap<String, PreVar>, // contains all names referenceable from outside the current sequence
    filename: Option<&str>,
) -> Result<(), CompileMessage<ParseProgramError>> {
    for import_spec_node in &mut es_import_decl.specifiers {
        if let Node {
            loc: _,
            kind: NodeKind::ImportSpecifier(import_spec),
        } = import_spec_node
        {
            if let Node {
                loc: _,
                kind: NodeKind::Identifier(Identifier { name, prevar }),
            } = &mut *import_spec.local
            {
                let resvar = *name_ctx.get(name.as_str()).unwrap();
                *prevar = Some(resvar);
            } else {
                return Err(CompileMessage::new_error(
                    import_spec.local.loc.into_sl(filename).to_owned(),
                    ParseProgramError::ESTreeError("ImportSpecifier local must be Identifier"),
                ));
            }
        } else {
            return Err(CompileMessage::new_error(
                import_spec_node.loc.into_sl(filename).to_owned(),
                ParseProgramError::ESTreeError(
                    "Expected ImportSpecifier inside ImportDeclaration only",
                ),
            ));
        }
    }
    Ok(())
}

fn pre_parse_export_decl(
    es_export_decl: &mut ExportNamedDeclaration,
    _loc: &Option<esSL>,
    name_ctx: &mut HashMap<String, PreVar>, // contains all names referenceable from outside the current sequence
    filename: Option<&str>,
) -> Result<(), CompileMessage<ParseProgramError>> {
    for export_spec_node in &mut es_export_decl.specifiers {
        if let Node {
            loc: _,
            kind: NodeKind::ExportSpecifier(export_spec),
        } = export_spec_node
        {
            if let Node {
                loc: _,
                kind: NodeKind::Identifier(Identifier { name, prevar }),
            } = &mut *export_spec.local
            {
                let resvar = *name_ctx.get(name.as_str()).unwrap();
                *prevar = Some(resvar);
            } else {
                return Err(CompileMessage::new_error(
                    export_spec.local.loc.into_sl(filename).to_owned(),
                    ParseProgramError::ESTreeError("ExportSpecifier local must be Identifier"),
                ));
            }
        } else {
            return Err(CompileMessage::new_error(
                export_spec_node.loc.into_sl(filename).to_owned(),
                ParseProgramError::ESTreeError(
                    "Expected ExportSpecifier inside ExportNamedDeclaration only",
                ),
            ));
        }
    }
    Ok(())
}

enum MultipleOrSingleBTreeMap {
    BTreeMap(BTreeMap<VarLocId, Usage>),
    MultipleBtreeMap(Vec<BTreeMap<VarLocId, Usage>>),
}

fn pre_parse_expr(
    es_expr: &mut Node,
    name_ctx: &mut HashMap<String, PreVar>,
    depth: usize,
    filename: Option<&str>,
) -> Result<MultipleOrSingleBTreeMap, CompileMessage<ParseProgramError>> {
    match &mut es_expr.kind {
        NodeKind::Identifier(identifier) => {
            let ret =
                pre_parse_identifier_use(identifier, &es_expr.loc, name_ctx, depth, filename)?;
            Ok(MultipleOrSingleBTreeMap::BTreeMap(ret))
        }
        NodeKind::Literal(literal) => match literal.value {
            LiteralValue::String(_) | LiteralValue::Boolean(_) | LiteralValue::Number(_) => {
                Ok(MultipleOrSingleBTreeMap::BTreeMap(BTreeMap::new()))
            }
            LiteralValue::Null => Err(CompileMessage::new_error(
                es_expr.loc.into_sl(filename).to_owned(),
                ParseProgramError::SourceRestrictionError("Null literal not allowed"),
            )),
            LiteralValue::RegExp => Err(CompileMessage::new_error(
                es_expr.loc.into_sl(filename).to_owned(),
                ParseProgramError::SourceRestrictionError("Regular expression not allowed"),
            )),
        },
        NodeKind::FunctionExpression(_) => Err(CompileMessage::new_error(
            es_expr.loc.into_sl(filename).to_owned(),
            ParseProgramError::SourceRestrictionError(
                "Function expression not allowed, use arrow function syntax instead",
            ),
        )),
        NodeKind::ArrowFunctionExpression(function) => {
            let ret = pre_parse_function(function, &es_expr.loc, name_ctx, depth, filename)?;
            Ok(MultipleOrSingleBTreeMap::BTreeMap(ret))
        }
        NodeKind::UnaryExpression(unary_expr) => {
            pre_parse_expr(&mut *unary_expr.argument, name_ctx, depth, filename)
        }
        NodeKind::UpdateExpression(_) => Err(CompileMessage::new_error(
            es_expr.loc.into_sl(filename).to_owned(),
            ParseProgramError::SourceRestrictionError(
                "Increment and decrement operators not allowed",
            ),
        )),
        NodeKind::BinaryExpression(binary_expr) => {
            // both sides of the operator are always evaluated, and JS requires left-to-right evaluation
            let lhs = match pre_parse_expr(&mut *binary_expr.left, name_ctx, depth, filename)? {
                MultipleOrSingleBTreeMap::BTreeMap(lhs) => lhs,
                _ => unreachable!(),
            };

            let rhs = match pre_parse_expr(&mut *binary_expr.right, name_ctx, depth, filename)? {
                MultipleOrSingleBTreeMap::BTreeMap(rhs) => rhs,
                _ => unreachable!(),
            };

            Ok(MultipleOrSingleBTreeMap::BTreeMap(varusage::merge_series(
                lhs, rhs,
            )))
        }
        NodeKind::AssignmentExpression(_) => Err(CompileMessage::new_error(
            es_expr.loc.into_sl(filename).to_owned(),
            ParseProgramError::SourceRestrictionError(
                "Assignment cannot be nested in an expression",
            ),
        )),
        NodeKind::LogicalExpression(logical_expr) => {
            // logical operators will short circuit, but it doesn't affect the result
            // since a + (b | empty) === a + b
            // let lhs = pre_parse_expr(&mut *logical_expr.left, name_ctx, depth, filename)?;

            let lhs = match pre_parse_expr(&mut *logical_expr.left, name_ctx, depth, filename)? {
                MultipleOrSingleBTreeMap::BTreeMap(lhs) => lhs,
                _ => unreachable!(),
            };

            let rhs = match pre_parse_expr(&mut *logical_expr.right, name_ctx, depth, filename)? {
                MultipleOrSingleBTreeMap::BTreeMap(rhs) => rhs,
                _ => unreachable!(),
            };

            Ok(MultipleOrSingleBTreeMap::BTreeMap(varusage::merge_series(
                lhs, rhs,
            )))
        }
        NodeKind::ConditionalExpression(cond_expr) => {
            // conditional expression, i.e. a ? b : c
            // like an if-statement, the returned result is a + (b | c)
            let test = match pre_parse_expr(&mut *cond_expr.test, name_ctx, depth, filename)? {
                MultipleOrSingleBTreeMap::BTreeMap(test) => test,
                _ => unreachable!(),
            };

            let true_ret =
                match pre_parse_expr(&mut *cond_expr.consequent, name_ctx, depth, filename)? {
                    MultipleOrSingleBTreeMap::BTreeMap(true_ret) => true_ret,
                    _ => unreachable!(),
                };

            let false_ret =
                match pre_parse_expr(&mut *cond_expr.alternate, name_ctx, depth, filename)? {
                    MultipleOrSingleBTreeMap::BTreeMap(false_ret) => false_ret,
                    _ => unreachable!(),
                };

            Ok(MultipleOrSingleBTreeMap::BTreeMap(varusage::merge_series(
                test,
                varusage::merge_parallel(true_ret, false_ret),
            )))
        }
        NodeKind::CallExpression(call_expr) => {
            // function call, i.e. f(a, b, ...)
            // just use the callee and all the params
            // JS requires 'f' to be evaluated first, followed by 'a', then 'b', etc.
            let f_ret = pre_parse_expr(&mut *call_expr.callee, name_ctx, depth, filename)?;
            call_expr
                .arguments
                .iter_mut()
                .fold(Ok(f_ret), |r_prev, arg| {
                    r_prev.and_then(|prev| {
                        let prev = match prev {
                            MultipleOrSingleBTreeMap::BTreeMap(prev) => prev,
                            _ => unreachable!(),
                        };

                        let next = match pre_parse_expr(arg, name_ctx, depth, filename)? {
                            MultipleOrSingleBTreeMap::BTreeMap(next) => next,
                            _ => unreachable!(),
                        };

                        Ok(MultipleOrSingleBTreeMap::BTreeMap(varusage::merge_series(
                            prev, next,
                        )))
                    })
                })
        }
        NodeKind::ArrayExpression(arr_expr) => {
            let mut ret = Vec::with_capacity(arr_expr.elements.len());
            for el in arr_expr.elements.iter_mut() {
                let ret_el = match pre_parse_expr(el, name_ctx, depth, filename)? {
                    MultipleOrSingleBTreeMap::BTreeMap(map) => map,
                    MultipleOrSingleBTreeMap::MultipleBtreeMap(_) => unreachable!(),
                };
                ret.push(ret_el);
            }
            Ok(MultipleOrSingleBTreeMap::MultipleBtreeMap(ret))
        }
        _ => Err(CompileMessage::new_error(
            es_expr.loc.into_sl(filename).to_owned(),
            ParseProgramError::ESTreeError("Expression node expected"),
        )),
    }
}

fn pre_parse_identifier_use(
    es_id: &mut Identifier,
    loc: &Option<esSL>,
    name_ctx: &mut HashMap<String, PreVar>,
    _depth: usize,
    filename: Option<&str>,
) -> Result<BTreeMap<VarLocId, Usage>, CompileMessage<ParseProgramError>> {
    match name_ctx.get(es_id.name.as_str()) {
        Some(prevar) => {
            es_id.prevar = Some(*prevar); // save the variable location
            match *prevar {
                PreVar::Target(varlocid) => {
                    if varlocid.depth == 0 {
                        // it is a global variable, but don't do anything because it doesn't count as a usage
                        Ok(BTreeMap::new())
                    } else {
                        // it's not a global variable
                        // say that we used this variable
                        Ok(varusage::from_used(varlocid))
                    }
                }
                PreVar::Direct => {
                    // don't do anything, because direct names do not count as a usage
                    Ok(BTreeMap::new())
                }
            }
        }
        None => Err(CompileMessage::new_error(
            loc.into_sl(filename).to_owned(),
            ParseProgramError::UndeclaredNameError(es_id.name.clone()),
        )),
    }
}

/**
 * Extracts both Targets and Directs.
 * Will return Err if any declaration (either Target or Direct) is considered to be duplicate.
 * Will also annotate any LHS identifiers with the target index.
 */
fn validate_and_extract_decls(
    es_block_body: &[Node],
    depth: usize,
    start_idx: &mut usize,
    filename: Option<&str>,
) -> Result<Vec<(String, PreVar)>, CompileMessage<ParseProgramError>> {
    let mut var_ctx: ProgramPreExports = VarCtx::new();
    let mut ret: Vec<(String, PreVar)> = Vec::new();
    es_block_body.each_with_attributes(filename, |es_node, attr| match es_node {
        Node {
            loc,
            kind: NodeKind::FunctionDeclaration(func_decl),
        } => process_func_decl_validation(
            &mut var_ctx,
            &mut ret,
            func_decl,
            loc,
            attr,
            depth,
            start_idx,
            filename,
        ),
        Node {
            loc,
            kind: NodeKind::VariableDeclaration(var_decl),
        } => process_var_decl_validation(
            &mut var_ctx,
            &mut ret,
            var_decl,
            loc,
            attr,
            depth,
            start_idx,
            filename,
        ),
        _ => Ok(()),
    })?;
    Ok(ret)
}

/**
 * Extracts all the names in this scope, and create a new prevar for each of them
 * with Target prevars starting from the given start_idx.
 * Returns the extracted names as a vector, but also extracts and returns exported names.
 * We only support the export list syntax currently (i.e. "export { a, b, c };").
 *
 * See validate_and_extract_decls().
 *
 * For imports and exports, we do not create a new prevar for each of them - we use the existing prevar that they refer to instead.
 */
fn validate_and_extract_imports_and_decls(
    es_program_body: &[Node],
    import_ctx: &[&ProgramPreExports],
    start_idx: &mut usize,
    filename: Option<&str>,
) -> Result<(Vec<(String, PreVar)>, ProgramPreExports), CompileMessage<ParseProgramError>> {
    let mut var_ctx: ProgramPreExports = VarCtx::new();
    let mut ret: Vec<(String, PreVar)> = Vec::new();
    let mut exports: ProgramPreExports = ProgramPreExports::new();
    let mut import_decl_idx = 0;
    es_program_body.each_with_attributes(filename, |es_node, attr| match es_node {
        Node {
            loc,
            kind: NodeKind::FunctionDeclaration(func_decl),
        } => process_func_decl_validation(
            &mut var_ctx,
            &mut ret,
            func_decl,
            loc,
            attr,
            0,
            start_idx,
            filename,
        ),
        Node {
            loc,
            kind: NodeKind::VariableDeclaration(var_decl),
        } => process_var_decl_validation(
            &mut var_ctx,
            &mut ret,
            var_decl,
            loc,
            attr,
            0,
            start_idx,
            filename,
        ),
        Node {
            loc,
            kind: NodeKind::ImportDeclaration(import_decl),
        } => process_import_decl_validation(
            &mut var_ctx,
            &mut ret,
            import_ctx[{
                let tmp = import_decl_idx;
                import_decl_idx += 1;
                tmp
            }],
            import_decl,
            loc,
            attr,
            filename,
        ),
        Node {
            loc,
            kind: NodeKind::ExportNamedDeclaration(export_decl),
        } => {
            process_export_decl_validation(&var_ctx, &mut exports, export_decl, loc, attr, filename)
        }
        _ => Ok(()),
    })?;
    Ok((ret, exports))
}

fn process_func_decl_validation(
    var_ctx: &mut ProgramPreExports,
    out: &mut Vec<(String, PreVar)>,
    func_decl: &FunctionDeclaration,
    _loc: &Option<esSL>,
    attr: HashMap<String, Option<String>>,
    depth: usize,
    start_idx: &mut usize,
    filename: Option<&str>,
) -> Result<(), CompileMessage<ParseProgramError>> {
    if attr.contains_key("direct") {
        if let Some(name) = try_coalesce_id_direct(
            var_ctx,
            &func_decl.params,
            &*func_decl.id,
            attr.get("constraint"),
            depth,
            start_idx,
            filename,
        )? {
            out.push((name.to_owned(), PreVar::Direct));
        }
    } else {
        let (name, varlocid) =
            try_coalesce_id_target(var_ctx, &*func_decl.id, depth, start_idx, filename)?;
        out.push((name.to_owned(), PreVar::Target(varlocid)));
    }
    Ok(())
}

fn process_var_decl_validation(
    var_ctx: &mut ProgramPreExports,
    out: &mut Vec<(String, PreVar)>,
    var_decl: &VariableDeclaration,
    loc: &Option<esSL>,
    attr: HashMap<String, Option<String>>,
    depth: usize,
    start_idx: &mut usize,
    filename: Option<&str>,
) -> Result<(), CompileMessage<ParseProgramError>> {
    if attr.contains_key("direct") {
        Err(CompileMessage::new_error(
            loc.into_sl(filename).to_owned(),
            ParseProgramError::AttributeContentError(
                "The 'direct' attribute can only appear on FunctionDeclaration",
            ),
        ))
    } else {
        for var_decr_node in &var_decl.declarations {
            if let Node {
                loc: _,
                kind: NodeKind::VariableDeclarator(var_decr),
            } = var_decr_node
            {
                let (name, varlocid) =
                    try_coalesce_id_target(var_ctx, &*var_decr.id, depth, start_idx, filename)?;
                out.push((name.to_owned(), PreVar::Target(varlocid)));
            } else {
                return Err(CompileMessage::new_error(
                    loc.into_sl(filename).to_owned(),
                    ParseProgramError::ESTreeError(
                        "Expected VariableDeclarator inside VariableDeclaration only",
                    ),
                ));
            }
        }
        Ok(())
    }
}

fn try_coalesce_id_target<'a>(
    var_ctx: &mut ProgramPreExports,
    es_id_node: &'a Node,
    depth: usize,
    start_idx: &mut usize,
    filename: Option<&str>,
) -> Result<(&'a str, VarLocId), CompileMessage<ParseProgramError>> {
    if let Node {
        loc,
        kind: NodeKind::Identifier(es_id),
    } = es_id_node
    {
        let ret = VarLocId {
            depth: depth,
            index: *start_idx,
        };
        if var_ctx.try_coalesce(es_id.name.clone(), VarValue::Target(ret)) {
            // insertion(coalesce) succeeded
            *start_idx += 1;
            Ok((es_id.name.as_str(), ret))
        } else {
            // insertion(coalesce) failed (i.e. it is a duplicate)
            Err(CompileMessage::new_error(
                loc.into_sl(filename).to_owned(),
                ParseProgramError::DuplicateDeclarationError(es_id.name.clone()),
            ))
        }
    } else {
        panic!("Node must be an identifier!");
    }
}
// returns true if it is a new variable
// or false if it is a new overload of an existing variable
fn try_coalesce_id_direct<'a>(
    var_ctx: &mut ProgramPreExports,
    param_nodes: &[Node],
    es_id_node: &'a Node,
    constraint_str_opt: Option<&Option<String>>,
    _depth: usize,
    start_idx: &mut usize,
    filename: Option<&str>,
) -> Result<Option<&'a str>, CompileMessage<ParseProgramError>> {
    if let Node {
        loc,
        kind: NodeKind::Identifier(es_id),
    } = es_id_node
    {
        let mut param_set: HashSet<&str> = HashSet::new();
        let params: Box<[&str]> = param_nodes
            .iter()
            .map(|es_node| {
                if let Node {
                    loc: loc2,
                    kind: NodeKind::Identifier(es_id),
                } = es_node
                {
                    if param_set.insert(&es_id.name) {
                        Ok(es_id.name.as_str())
                    } else {
                        Err(CompileMessage::new_error(
                            loc2.into_sl(filename).to_owned(),
                            ParseProgramError::DuplicateDeclarationError(es_id.name.clone()),
                        ))
                    }
                } else {
                    Err(CompileMessage::new_error(
                        es_node.loc.into_sl(filename).to_owned(),
                        ParseProgramError::ESTreeError("Expected identifier"),
                    ))
                }
            })
            .collect::<Result<Box<[&str]>, CompileMessage<ParseProgramError>>>()?;
        let constraints: HashMap<&str, ir::VarType> = match constraint_str_opt {
            Some(cso) => match cso {
                Some(constraint_str) => {
                    constraint::parse_constraint(constraint_str).map_err(|(_loc_str, reason)| {
                        CompileMessage::new_error(
                            loc.into_sl(filename).to_owned(), // this is the wrong location, but it's hard to get the correct one
                            ParseProgramError::AttributeContentError(reason),
                        )
                    })?
                }
                None => {
                    return Err(CompileMessage::new_error(
                        loc.into_sl(filename).to_owned(),
                        ParseProgramError::AttributeContentError(
                            "'constraint' attribute must have a value",
                        ),
                    ));
                }
            },
            None => HashMap::new(),
        };
        for (key, _) in &constraints {
            if !param_set.contains(key) {
                return Err(CompileMessage::new_error(
                    loc.into_sl(filename).to_owned(),
                    ParseProgramError::AttributeContentError(
                        "Parameter name specified in 'constraint' attribute does not exist",
                    ),
                ));
            }
        }
        let direct_type: Box<[ir::VarType]> = params
            .into_iter()
            .map(|name| constraints.get(name).copied().unwrap_or(ir::VarType::Any))
            .collect();
        let curr_len = var_ctx.len();
        if var_ctx.try_coalesce(
            es_id.name.clone(),
            VarValue::Direct(OverloadSet::from_single(direct_type)),
        ) {
            // insertion(coalesce) succeeded
            if var_ctx.len() > curr_len {
                *start_idx += 1;
                Ok(Some(es_id.name.as_str()))
            } else {
                Ok(None)
            }
        } else {
            // insertion(coalesce) failed (i.e. it is a duplicate)
            Err(CompileMessage::new_error(
                loc.into_sl(filename).to_owned(),
                ParseProgramError::DuplicateDeclarationError(es_id.name.clone()),
            ))
        }
    } else {
        panic!("Node must be an identifier!");
    }
}

fn process_import_decl_validation(
    var_ctx: &mut ProgramPreExports,
    out: &mut Vec<(String, PreVar)>,
    import_state: &ProgramPreExports,
    import_decl: &ImportDeclaration,
    loc: &Option<esSL>,
    attr: HashMap<String, Option<String>>,
    filename: Option<&str>,
) -> Result<(), CompileMessage<ParseProgramError>> {
    if !attr.is_empty() {
        Err(CompileMessage::new_error(
            loc.into_sl(filename).to_owned(),
            ParseProgramError::AttributeContentError(
                "Attributes are not allowed on import declaration",
            ),
        ))
    } else {
        for import_spec_node in &import_decl.specifiers {
            if let Node {
                loc: _,
                kind: NodeKind::ImportSpecifier(import_spec),
            } = import_spec_node
            {
                if let Node {
                    loc: loc3,
                    kind: NodeKind::Identifier(source_id),
                } = &*import_spec.imported
                {
                    if let Node {
                        loc: loc4,
                        kind: NodeKind::Identifier(local_id),
                    } = &*import_spec.local
                    {
                        let varvalue =
                            import_state.get(source_id.name.as_str()).ok_or_else(|| {
                                CompileMessage::new_error(
                                    loc3.into_sl(filename).to_owned(),
                                    ParseProgramError::UndeclaredExportError(
                                        source_id.name.clone(),
                                    ),
                                )
                            })?;
                        match varvalue {
                            VarValue::Target(varlocid) => {
                                if !var_ctx.try_coalesce(
                                    local_id.name.clone(),
                                    VarValue::Target(*varlocid),
                                ) {
                                    return Err(CompileMessage::new_error(
                                        loc4.into_sl(filename).to_owned(),
                                        ParseProgramError::DuplicateDeclarationError(
                                            local_id.name.clone(),
                                        ),
                                    ));
                                } else {
                                    out.push((local_id.name.to_owned(), PreVar::Target(*varlocid)));
                                }
                            }
                            VarValue::Direct(signature) => {
                                if !var_ctx.try_coalesce(
                                    local_id.name.clone(),
                                    VarValue::Direct(signature.clone()),
                                ) {
                                    return Err(CompileMessage::new_error(
                                        loc4.into_sl(filename).to_owned(),
                                        ParseProgramError::DuplicateDeclarationError(
                                            local_id.name.clone(),
                                        ),
                                    ));
                                } else {
                                    out.push((local_id.name.to_owned(), PreVar::Direct));
                                }
                            }
                        }
                    } else {
                        return Err(CompileMessage::new_error(
                            import_spec.local.loc.into_sl(filename).to_owned(),
                            ParseProgramError::ESTreeError(
                                "ImportSpecifier local must be Identifier",
                            ),
                        ));
                    }
                } else {
                    return Err(CompileMessage::new_error(
                        import_spec.imported.loc.into_sl(filename).to_owned(),
                        ParseProgramError::ESTreeError("ImportSpecifier source must be Identifier"),
                    ));
                }
            } else {
                return Err(CompileMessage::new_error(
                    loc.into_sl(filename).to_owned(),
                    ParseProgramError::ESTreeError(
                        "Expected ImportSpecifier inside ImportDeclaration only",
                    ),
                ));
            }
        }
        Ok(())
    }
}

fn process_export_decl_validation(
    var_ctx: &ProgramPreExports,
    exports: &mut ProgramPreExports,
    export_decl: &ExportNamedDeclaration,
    loc: &Option<esSL>,
    attr: HashMap<String, Option<String>>,
    filename: Option<&str>,
) -> Result<(), CompileMessage<ParseProgramError>> {
    if !attr.is_empty() {
        Err(CompileMessage::new_error(
            loc.into_sl(filename).to_owned(),
            ParseProgramError::AttributeContentError(
                "Attributes are not allowed on import declaration",
            ),
        ))
    } else if !export_decl.declaration.is_none() {
        Err(CompileMessage::new_error(
            loc.into_sl(filename).to_owned(),
            ParseProgramError::SourceRestrictionError(
                "Combined export and variable declaration is not allowed",
            ),
        ))
    } else if !export_decl.source.is_none() {
        Err(CompileMessage::new_error(
            loc.into_sl(filename).to_owned(),
            ParseProgramError::SourceRestrictionError(
                "Combined export and import declaration is not allowed",
            ),
        ))
    } else {
        for export_spec_node in &export_decl.specifiers {
            if let Node {
                loc: _,
                kind: NodeKind::ExportSpecifier(export_spec),
            } = export_spec_node
            {
                if let Node {
                    loc: loc3,
                    kind: NodeKind::Identifier(exported_id),
                } = &*export_spec.exported
                {
                    if let Node {
                        loc: loc4,
                        kind: NodeKind::Identifier(local_id),
                    } = &*export_spec.local
                    {
                        let varvalue = var_ctx.get(local_id.name.as_str()).ok_or_else(|| {
                            CompileMessage::new_error(
                                loc4.into_sl(filename).to_owned(),
                                ParseProgramError::UndeclaredNameError(local_id.name.clone()),
                            )
                        })?;
                        // Note: this coalesce actually functions like a normal insertion, returning false if an item already exists.
                        if !exports.try_coalesce(exported_id.name.clone(), varvalue.clone()) {
                            return Err(CompileMessage::new_error(
                                loc3.into_sl(filename).to_owned(),
                                ParseProgramError::DuplicateExportError(exported_id.name.clone()),
                            ));
                        }
                    } else {
                        return Err(CompileMessage::new_error(
                            export_spec.local.loc.into_sl(filename).to_owned(),
                            ParseProgramError::ESTreeError(
                                "ExportSpecifier local must be Identifier",
                            ),
                        ));
                    }
                } else {
                    return Err(CompileMessage::new_error(
                        export_spec.exported.loc.into_sl(filename).to_owned(),
                        ParseProgramError::ESTreeError(
                            "ExportSpecifier exported must be Identifier",
                        ),
                    ));
                }
            } else {
                return Err(CompileMessage::new_error(
                    export_spec_node.loc.into_sl(filename).to_owned(),
                    ParseProgramError::ESTreeError(
                        "Expected ExportSpecifier inside ExportNamedDeclaration only",
                    ),
                ));
            }
        }
        Ok(())
    }
}

fn validate_and_extract_params(
    params: &[Node],
    depth: usize,
    filename: Option<&str>,
) -> Result<Vec<(String, VarLocId)>, CompileMessage<ParseProgramError>> {
    let mut set: HashSet<String> = HashSet::new();
    params
        .iter()
        .enumerate()
        .map(|(i, param)| {
            if let Node {
                loc: _,
                kind: NodeKind::Identifier(es_id),
            } = param
            {
                if set.insert(es_id.name.clone()) {
                    // insertion succeeded (i.e. it is not a duplicate)
                    Ok((
                        es_id.name.clone(),
                        VarLocId {
                            depth: depth,
                            index: i,
                        },
                    ))
                } else {
                    // insertion failed (i.e. it is a duplicate)
                    Err(CompileMessage::new_error(
                        param.loc.into_sl(filename).to_owned(),
                        ParseProgramError::DuplicateDeclarationError(es_id.name.clone()),
                    ))
                }
            } else {
                Err(CompileMessage::new_error(
                    param.loc.into_sl(filename).to_owned(),
                    ParseProgramError::ESTreeError("Parameter node must be an identifier"),
                ))
            }
        })
        .collect()
}
