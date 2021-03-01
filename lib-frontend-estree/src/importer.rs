use super::frontendvar::*;
use super::ParseState;
use super::ProgramPreExports;
use crate::error::ImportsParseError;
use ir::Import;
use ir::ImportValType;
use projstd::log::CompileMessage;
use projstd::log::SourceLocationRef as plSLRef;
use std::boxed::Box;
use std::collections::HashMap;
use std::result::Result;

// The import file has "@SourceImports" on the first line,
// and subsequent lines are either empty or are of the following syntax:
// <local name> <imported namespace> <imported name> <return type> <param types...>

pub struct ImportSpec {
    pub content: Vec<(String, Import)>,
}

pub fn has_imports_header(import_spec: &str) -> bool {
    return import_spec.lines().next() == Some("@SourceImports");
}

pub fn parse_imports(
    filename: &str,
    import_spec: &str,
) -> Result<ImportSpec, CompileMessage<ImportsParseError>> {
    let mut ret: Vec<(String, Import)> = Vec::new();
    let mut iter = import_spec.lines().enumerate();
    iter.next()
        .and_then(|(_, line)| {
            if line == "@SourceImports" {
                Some(())
            } else {
                None
            }
        })
        .ok_or_else(|| {
            CompileMessage::new_error(
                plSLRef::entire_line(1, Some(filename)).to_owned(),
                ImportsParseError::InvalidHeader,
            )
            .into_cm()
        })?;
    for (i, line) in iter {
        if let Some(name_import) = parse_import(filename, line, i as i32 + 1)? {
            ret.push(name_import);
        }
    }
    Ok(ImportSpec { content: ret })
}

fn parse_import(
    filename: &str,
    import_line: &str,
    line_num: i32,
) -> Result<Option<(String, Import)>, CompileMessage<ImportsParseError>> {
    //"__ffi_display misc display undefined string";
    fn error_if_none<'a>(
        x: Option<&'a str>,
        err: ImportsParseError,
        filename: &str,
        line: i32,
        col: i32,
    ) -> Result<&'a str, CompileMessage<ImportsParseError>> {
        x.ok_or_else(|| {
            CompileMessage::new_error(
                plSLRef::new(line, col, line + 1, 0, Some(filename)).to_owned(),
                err,
            )
        })
    };

    fn error_if_not_valid_vartype(
        x: Option<ImportValType>,
        bad_name: &str,
        filename: &str,
        line_num: i32,
        line: &str,
    ) -> Result<ImportValType, CompileMessage<ImportsParseError>> {
        x.ok_or_else(|| {
            CompileMessage::new_error(
                plSLRef::within_line(line_num, bad_name, line, Some(filename)).to_owned(),
                ImportsParseError::InvalidVarType(bad_name.to_owned()),
            )
        })
    };

    // we allow any number of spaces between tokens
    let mut it = import_line.split(' ').filter(|x| !x.is_empty());

    // this allows for empty lines
    let source_name = if let Some(s) = it.next() {
        s
    } else {
        return Ok(None);
    };

    let host_module = error_if_none(
        it.next(),
        ImportsParseError::MissingHostModuleName,
        filename,
        line_num,
        import_line.len() as i32,
    )?;
    let host_entity = error_if_none(
        it.next(),
        ImportsParseError::MissingHostEntityName,
        filename,
        line_num,
        import_line.len() as i32,
    )?;
    let return_type_str = error_if_none(
        it.next(),
        ImportsParseError::MissingReturnType,
        filename,
        line_num,
        import_line.len() as i32,
    )?;

    let return_type = error_if_not_valid_vartype(
        make_vartype(return_type_str),
        return_type_str,
        filename,
        line_num,
        import_line,
    )?;

    let param_types = it
        .map(|x| error_if_not_valid_vartype(make_vartype(x), x, filename, line_num, import_line))
        .collect::<Result<Box<[ImportValType]>, CompileMessage<ImportsParseError>>>()?;

    Ok(Some((
        source_name.to_owned(),
        Import {
            module_name: host_module.to_owned(),
            entity_name: host_entity.to_owned(),
            params: param_types,
            result: return_type,
        },
    )))
}

fn make_vartype(type_name: &str) -> Option<ImportValType> {
    match type_name {
        "undefined" => Some(ImportValType::Undefined),
        "number" => Some(ImportValType::Number),
        "string" => Some(ImportValType::String),
        _ => None,
    }
}

/*pub fn add_import_spec_to_state(
    state: &mut compact_state::CompactState<compact_state::FrontendVar>,
    import_spec: ImportSpec,
    order: usize,
    import_funcidx_map: &HashMap<ir::Import, ir::FuncIdx>,
) {
    for (name, import) in import_spec.content {
        // note: we can safely unwrap because it is guaranteed to exist (because we added it in earlier)
        let funcidx = *import_funcidx_map.get(&import).unwrap();
        let overload = compact_state::MaterializedOverload {
            params: import.params.into_iter().map(|p| (*p).into()).collect(),
            result: import.result.into(),
            funcidx: funcidx,
            order: order,
        };
        state.insert(name, compact_state::FrontendVar::new_overload(overload));
    }
}*/

pub fn make_export_state(
    import_spec: ImportSpec,
    _order: usize,
    import_funcidx_map: &HashMap<ir::Import, ir::FuncIdx>,
) -> (ProgramPreExports, ParseState) {
    let mut pre_exports: ProgramPreExports = VarCtx::new();
    let mut parse_ctx: ParseState = ParseState::default();

    for (name, import) in import_spec.content {
        // note: we can safely unwrap because it is guaranteed to exist (because we added it in earlier)

        let ir_params: Box<[ir::VarType]> =
            import.params.into_iter().map(|p| (*p).into()).collect();

        // pre_exports
        pre_exports.coalesce(name.clone(), VarValue::new_direct(ir_params.clone()));

        // parse_ctx
        let funcidx = *import_funcidx_map.get(&import).unwrap();
        parse_ctx.add_direct(name, OverloadSet::from_single((ir_params, funcidx)));
    }

    (pre_exports, parse_ctx)
}
