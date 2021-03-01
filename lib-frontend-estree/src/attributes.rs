use crate::estree::*;
use crate::extensions::IntoSourceLocation;
use crate::func::ParseProgramError;
use projstd::log::CompileMessage;
use projstd::log::SourceLocationRef as plSLRef;
use std::collections::HashMap;
use std::result::Result;

/**
 * Detects attribute declarations and parse them and associate them with the statement that immediately follows.
 */
pub trait NodeForEachWithAttributes<E> {
    fn each_with_attributes<
        F: FnMut(&E, HashMap<String, Option<String>>) -> Result<(), CompileMessage<ParseProgramError>>,
    >(
        &self,
        filename: Option<&str>,
        f: F,
    ) -> Result<(), CompileMessage<ParseProgramError>>;
}

impl NodeForEachWithAttributes<Node> for [Node] {
    fn each_with_attributes<
        F: FnMut(
            &Node,
            HashMap<String, Option<String>>,
        ) -> Result<(), CompileMessage<ParseProgramError>>,
    >(
        &self,
        filename: Option<&str>,
        mut f: F,
    ) -> Result<(), CompileMessage<ParseProgramError>> {
        let mut prev_attr: Option<(HashMap<String, Option<String>>, plSLRef)> = None;
        for node in self {
            if let Node {
                loc: stmtloc,
                kind: NodeKind::ExpressionStatement(expr_stmt),
            } = node
            {
                if let Node {
                    loc: _,
                    kind:
                        NodeKind::AssignmentExpression(AssignmentExpression {
                            operator: _operator,
                            left,
                            right,
                        }),
                } = &*expr_stmt.expression
                {
                    if let Node {
                        loc: _,
                        kind: NodeKind::Identifier(ident),
                    } = &**left
                    {
                        if ident.name == "__attributes" {
                            if let Node {
                                loc: valloc,
                                kind:
                                    NodeKind::Literal(Literal {
                                        value: LiteralValue::String(strval),
                                    }),
                            } = &**right
                            {
                                let val: HashMap<String, Option<String>> =
                                    parse_attributes(&strval).map_err(|_| {
                                        CompileMessage::new_error(
                                            valloc.into_sl(filename).to_owned(),
                                            ParseProgramError::AttributeParseError,
                                        )
                                    })?;
                                if let Some((_, sl)) = prev_attr {
                                    return Err(CompileMessage::new_error(
                                        sl.to_owned(),
                                        ParseProgramError::DanglingAttributeError,
                                    ));
                                }
                                prev_attr = Some((val, stmtloc.into_sl(filename)));
                                continue;
                            } else {
                                return Err(CompileMessage::new_error(
                                    right.loc.into_sl(filename).to_owned(),
                                    ParseProgramError::AttributeNotStringLiteralError,
                                ));
                            }
                        }
                    }
                }
            }
            f(node, prev_attr.map_or_else(|| HashMap::new(), |(x, _sl)| x))?;
            prev_attr = None;
        }
        if let Some((_, sl)) = prev_attr {
            return Err(CompileMessage::new_error(
                sl.to_owned(),
                ParseProgramError::DanglingAttributeError,
            ));
        }
        Ok(())
    }
}

pub trait NodeForEachWithAttributesMut<E> {
    fn each_with_attributes_mut<
        F: FnMut(
            &mut E,
            HashMap<String, Option<String>>,
        ) -> Result<(), CompileMessage<ParseProgramError>>,
    >(
        &mut self,
        filename: Option<&str>,
        f: F,
    ) -> Result<(), CompileMessage<ParseProgramError>>;
}

impl NodeForEachWithAttributesMut<Node> for [Node] {
    fn each_with_attributes_mut<
        F: FnMut(
            &mut Node,
            HashMap<String, Option<String>>,
        ) -> Result<(), CompileMessage<ParseProgramError>>,
    >(
        &mut self,
        filename: Option<&str>,
        mut f: F,
    ) -> Result<(), CompileMessage<ParseProgramError>> {
        let mut prev_attr: Option<(HashMap<String, Option<String>>, plSLRef)> = None;
        for node in self {
            // note: '&*node' changes it to non-mut
            if let Node {
                loc: stmtloc,
                kind: NodeKind::ExpressionStatement(expr_stmt),
            } = &*node
            {
                if let Node {
                    loc: _,
                    kind:
                        NodeKind::AssignmentExpression(AssignmentExpression {
                            operator: _operator,
                            left,
                            right,
                        }),
                } = &*expr_stmt.expression
                {
                    if let Node {
                        loc: _,
                        kind: NodeKind::Identifier(ident),
                    } = &**left
                    {
                        if ident.name == "__attributes" {
                            if let Node {
                                loc: valloc,
                                kind:
                                    NodeKind::Literal(Literal {
                                        value: LiteralValue::String(strval),
                                    }),
                            } = &**right
                            {
                                let val: HashMap<String, Option<String>> =
                                    parse_attributes(&strval).map_err(|_| {
                                        CompileMessage::new_error(
                                            valloc.into_sl(filename).to_owned(),
                                            ParseProgramError::AttributeParseError,
                                        )
                                    })?;
                                if let Some((_, sl)) = prev_attr {
                                    return Err(CompileMessage::new_error(
                                        sl.to_owned(),
                                        ParseProgramError::DanglingAttributeError,
                                    ));
                                }
                                prev_attr = Some((val, stmtloc.into_sl(filename)));
                                continue;
                            } else {
                                return Err(CompileMessage::new_error(
                                    right.loc.into_sl(filename).to_owned(),
                                    ParseProgramError::AttributeNotStringLiteralError,
                                ));
                            }
                        }
                    }
                }
            }
            f(node, prev_attr.map_or_else(|| HashMap::new(), |(x, _sl)| x))?;
            prev_attr = None;
        }
        if let Some((_, sl)) = prev_attr {
            return Err(CompileMessage::new_error(
                sl.to_owned(),
                ParseProgramError::DanglingAttributeError,
            ));
        }
        Ok(())
    }
}

pub trait NodeForEachWithAttributesInto<E> {
    fn each_with_attributes_into<
        F: FnMut(E, HashMap<String, Option<String>>) -> Result<(), CompileMessage<ParseProgramError>>,
    >(
        self,
        filename: Option<&str>,
        f: F,
    ) -> Result<(), CompileMessage<ParseProgramError>>;
}

impl<C: IntoIterator<Item = Node>> NodeForEachWithAttributesInto<Node> for C {
    fn each_with_attributes_into<
        F: FnMut(
            Node,
            HashMap<String, Option<String>>,
        ) -> Result<(), CompileMessage<ParseProgramError>>,
    >(
        self,
        filename: Option<&str>,
        mut f: F,
    ) -> Result<(), CompileMessage<ParseProgramError>> {
        let mut prev_attr: Option<(HashMap<String, Option<String>>, plSLRef)> = None;
        for node in self {
            if let Node {
                loc: stmtloc,
                kind: NodeKind::ExpressionStatement(expr_stmt),
            } = &node
            {
                if let Node {
                    loc: _,
                    kind:
                        NodeKind::AssignmentExpression(AssignmentExpression {
                            operator: _operator,
                            left,
                            right,
                        }),
                } = &*expr_stmt.expression
                {
                    if let Node {
                        loc: _,
                        kind: NodeKind::Identifier(ident),
                    } = &**left
                    {
                        if ident.name == "__attributes" {
                            if let Node {
                                loc: valloc,
                                kind:
                                    NodeKind::Literal(Literal {
                                        value: LiteralValue::String(strval),
                                    }),
                            } = &**right
                            {
                                let val: HashMap<String, Option<String>> =
                                    parse_attributes(&strval).map_err(|_| {
                                        CompileMessage::new_error(
                                            valloc.into_sl(filename).to_owned(),
                                            ParseProgramError::AttributeParseError,
                                        )
                                    })?;
                                if let Some((_, sl)) = prev_attr {
                                    return Err(CompileMessage::new_error(
                                        sl.to_owned(),
                                        ParseProgramError::DanglingAttributeError,
                                    ));
                                }
                                prev_attr = Some((val, stmtloc.into_sl(filename)));
                                continue;
                            } else {
                                return Err(CompileMessage::new_error(
                                    right.loc.into_sl(filename).to_owned(),
                                    ParseProgramError::AttributeNotStringLiteralError,
                                ));
                            }
                        }
                    }
                }
            }
            f(node, prev_attr.map_or_else(|| HashMap::new(), |(x, _sl)| x))?;
            prev_attr = None;
        }
        if let Some((_, sl)) = prev_attr {
            return Err(CompileMessage::new_error(
                sl.to_owned(),
                ParseProgramError::DanglingAttributeError,
            ));
        }
        Ok(())
    }
}

/**
 * Parse an attribute string into an attribute HashMap.
 * Repeated keys are not allowed.
 * Keys and values must be non-empty, but values can be omitted.
 */
fn parse_attributes(text: &str) -> Result<HashMap<String, Option<String>>, ()> {
    // note: an attribute is of the form:
    // __attributes = "x;y=val;z;k"
    // or:
    // __attributes = "x;y=val;z;k;"
    // leading or trailing spaces of each part are trimmed

    let mut ret: HashMap<String, Option<String>> = HashMap::new();

    for raw_item in text.split_terminator(';') {
        let item = raw_item.trim();
        let (key, val) = if let Some(idx) = item.find('=') {
            let (raw_key, tmp) = item.split_at(idx);
            let key = raw_key.trim();
            let val = tmp['='.len_utf8()..].trim();
            if key.is_empty() || val.is_empty() {
                return Err(());
            }
            (key, Some(val))
        } else {
            if item.is_empty() {
                return Err(());
            }
            (item, None)
        };
        if ret.contains_key(key) {
            return Err(());
        } else {
            ret.insert(key.to_owned(), val.map(|x| x.to_owned()));
        }
    }
    Ok(ret)
}
