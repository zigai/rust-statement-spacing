use ra_ap_syntax::{AstNode, SyntaxKind, SyntaxNode, ast};
use rust_statement_spacing_core::{ByteRange, ItemKind, UnitKind};

pub(crate) fn range(node: &SyntaxNode) -> ByteRange {
    let range = node.text_range();
    return ByteRange::new(
        u32::from(range.start()) as usize,
        u32::from(range.end()) as usize,
    );
}

pub(crate) fn code_range(node: &SyntaxNode) -> ByteRange {
    let mut result = range(node);
    // Attributes on expression statements belong to the wrapped expression.
    // Match the compiler's code start while retaining the statement semicolon.
    if let Some(statement) = ast::ExprStmt::cast(node.clone())
        && let Some(expression) = statement.expr()
    {
        result.start = code_range(expression.syntax()).start;
        return result;
    }
    for child in node.children_with_tokens() {
        if let Some(node) = child.as_node()
            && node.kind() == SyntaxKind::ATTR
        {
            continue;
        }
        if let Some(token) = child.as_token()
            && matches!(token.kind(), SyntaxKind::WHITESPACE | SyntaxKind::COMMENT)
        {
            continue;
        }
        result.start = u32::from(child.text_range().start()) as usize;
        break;
    }
    return result;
}

pub(crate) fn is_expression(node: &SyntaxNode) -> bool {
    return ast::Expr::can_cast(node.kind());
}
pub(crate) fn is_item(node: &SyntaxNode) -> bool {
    return ast::Item::can_cast(node.kind());
}

pub(crate) fn expression_node(node: &SyntaxNode) -> SyntaxNode {
    if node.kind() == SyntaxKind::EXPR_STMT {
        return node
            .children()
            .find(is_expression)
            .unwrap_or_else(|| return node.clone());
    } else {
        return node.clone();
    }
}

fn item_kind(node: &SyntaxNode) -> ItemKind {
    match node.kind() {
        SyntaxKind::FN => {
            if node
                .children()
                .any(|child| return child.kind() == SyntaxKind::BLOCK_EXPR)
            {
                return ItemKind::Function;
            } else {
                return ItemKind::Compact;
            }
        }
        SyntaxKind::MODULE => {
            if node
                .children()
                .any(|child| return child.kind() == SyntaxKind::ITEM_LIST)
            {
                return ItemKind::Major;
            } else {
                return ItemKind::Compact;
            }
        }
        SyntaxKind::USE
        | SyntaxKind::CONST
        | SyntaxKind::STATIC
        | SyntaxKind::TYPE_ALIAS
        | SyntaxKind::EXTERN_CRATE => return ItemKind::Compact,
        SyntaxKind::MACRO_CALL | SyntaxKind::MACRO_RULES | SyntaxKind::MACRO_DEF => {
            return ItemKind::Opaque;
        }
        _ => return ItemKind::Major,
    }
}

pub(crate) fn unit_kind(node: &SyntaxNode) -> UnitKind {
    if is_item(node) {
        return UnitKind::Item(item_kind(node));
    }
    if node.kind() == SyntaxKind::LET_STMT {
        return UnitKind::Let;
    }
    let expression = expression_node(node);
    match expression.kind() {
        SyntaxKind::IF_EXPR
        | SyntaxKind::FOR_EXPR
        | SyntaxKind::WHILE_EXPR
        | SyntaxKind::LOOP_EXPR
        | SyntaxKind::MATCH_EXPR => return UnitKind::Control,
        SyntaxKind::BLOCK_EXPR => return UnitKind::Block,
        SyntaxKind::RETURN_EXPR | SyntaxKind::BREAK_EXPR | SyntaxKind::CONTINUE_EXPR => {
            return UnitKind::Exit;
        }
        SyntaxKind::MACRO_EXPR | SyntaxKind::MACRO_CALL => return UnitKind::Opaque,
        SyntaxKind::BIN_EXPR => {
            let assignment = expression
                .children_with_tokens()
                .filter_map(|n| return n.into_token())
                .any(|token| {
                    return matches!(
                        token.text(),
                        "=" | "+=" | "-=" | "*=" | "/=" | "%=" | "&=" | "|=" | "^=" | "<<=" | ">>="
                    );
                });
            if assignment {
                return UnitKind::Assignment;
            } else {
                return UnitKind::Expression;
            }
        }
        _ => return UnitKind::Expression,
    }
}

pub(crate) fn deferred_ranges(node: &SyntaxNode) -> Vec<ByteRange> {
    return node
        .descendants()
        .filter(|child| {
            if child == node {
                return false;
            }
            let k = child.kind();
            return k == SyntaxKind::CLOSURE_EXPR
                || k == SyntaxKind::FN
                || (k == SyntaxKind::BLOCK_EXPR
                    && child
                        .children_with_tokens()
                        .filter_map(|n| return n.into_token())
                        .any(|token| return token.text() == "async"));
        })
        .map(|child| return range(&child))
        .collect();
}

/// Select written bodies without confusing a block-valued condition/scrutinee
/// with the body. HIR desugaring is deliberately not used for this distinction.
pub(crate) fn control_body_start(expression: &SyntaxNode) -> Option<usize> {
    let expressions: Vec<_> = expression.children().filter(is_expression).collect();
    let body = match expression.kind() {
        SyntaxKind::IF_EXPR => expressions.get(1).cloned(),
        SyntaxKind::FOR_EXPR | SyntaxKind::WHILE_EXPR | SyntaxKind::LOOP_EXPR => {
            expressions.last().cloned()
        }
        SyntaxKind::MATCH_EXPR => expression
            .children()
            .find(|node| return node.kind() == SyntaxKind::MATCH_ARM_LIST),
        _ => None,
    };
    return body.map(|body| return range(&body).start);
}

fn direct_operation_range(unit: &SyntaxNode) -> ByteRange {
    let expression = expression_node(unit);
    let end = if expression.kind() == SyntaxKind::BLOCK_EXPR {
        // A standalone nested block has no direct header use.
        code_range(unit).start
    } else {
        control_body_start(&expression).unwrap_or_else(|| return range(unit).end)
    };
    return ByteRange::new(code_range(unit).start, end);
}

pub(crate) fn immediate_body_first(expression: &SyntaxNode) -> Vec<ByteRange> {
    let mut result = Vec::new();
    let body_start = control_body_start(expression);
    for child in expression.children() {
        if body_start.is_some_and(|start| return range(&child).start < start) {
            continue;
        }
        if child.kind() == SyntaxKind::BLOCK_EXPR {
            if let Some(list) = child
                .children()
                .find(|n| return n.kind() == SyntaxKind::STMT_LIST)
                && let Some(unit) = list.children().find(is_unit)
            {
                // A nested control/body is not a first *direct operation*.
                // Use only its header, never recursively its entire body.
                result.push(direct_operation_range(&unit));
            }
        } else if child.kind() == SyntaxKind::MATCH_ARM_LIST {
            for arm in child
                .children()
                .filter(|n| return n.kind() == SyntaxKind::MATCH_ARM)
            {
                if let Some(body) = arm.children().find(is_expression) {
                    if body.kind() == SyntaxKind::BLOCK_EXPR {
                        if let Some(list) = body
                            .children()
                            .find(|n| return n.kind() == SyntaxKind::STMT_LIST)
                            && let Some(unit) = list.children().find(is_unit)
                        {
                            result.push(direct_operation_range(&unit));
                        }
                    } else {
                        result.push(direct_operation_range(&body));
                    }
                }
            }
        }
    }
    return result;
}

pub(crate) fn is_unit(node: &SyntaxNode) -> bool {
    return matches!(node.kind(), SyntaxKind::LET_STMT | SyntaxKind::EXPR_STMT)
        || is_item(node)
        || is_expression(node);
}

pub(crate) fn guard(expression: &SyntaxNode) -> bool {
    if expression.kind() != SyntaxKind::IF_EXPR {
        return false;
    }
    let body_start = control_body_start(expression);
    let blocks: Vec<_> = expression
        .children()
        .filter(|n| {
            return n.kind() == SyntaxKind::BLOCK_EXPR
                && body_start.is_some_and(|start| return range(n).start >= start);
        })
        .collect();
    if blocks.len() != 1 {
        return false;
    }
    if expression
        .children_with_tokens()
        .filter_map(|n| return n.into_token())
        .any(|token| return token.text() == "else")
    {
        return false;
    }
    let Some(first_block) = blocks.first() else {
        return false;
    };
    let Some(list) = first_block
        .children()
        .find(|n| return n.kind() == SyntaxKind::STMT_LIST)
    else {
        return false;
    };
    let units: Vec<_> = list.children().filter(is_unit).collect();
    let [first_unit] = units.as_slice() else {
        return false;
    };
    return unit_kind(first_unit) == UnitKind::Exit;
}
