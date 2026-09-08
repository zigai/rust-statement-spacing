use ra_ap_syntax::{
    AstNode, SyntaxKind, SyntaxNode,
    ast::{self, HasArgList as _},
};
use rust_statement_spacing_core::{ByteRange, Form, Scope, Shape, StatementRole};

use crate::units::{
    control_body_start, deferred_ranges, expression_node, is_expression, is_item, is_unit, range,
};

fn unwrapped(mut node: SyntaxNode) -> SyntaxNode {
    while matches!(
        node.kind(),
        SyntaxKind::TRY_EXPR | SyntaxKind::PAREN_EXPR | SyntaxKind::RETURN_EXPR
    ) {
        let Some(inner) = node.children().find(is_expression) else {
            break;
        };
        node = inner;
    }
    return node;
}

fn terminal(node: SyntaxNode) -> SyntaxNode {
    let node = expression_node(&node);
    if node.kind() == SyntaxKind::CLOSURE_EXPR
        && let Some(body) = node.children().find(is_expression)
    {
        return terminal(body);
    }
    if node.kind() == SyntaxKind::BLOCK_EXPR
        && let Some(last) = node
            .children()
            .find(|n| return n.kind() == SyntaxKind::STMT_LIST)
            .and_then(|list| return list.children().filter(is_unit).last())
    {
        return terminal(last);
    }
    return unwrapped(node);
}

fn error_sink(node: SyntaxNode) -> Option<String> {
    let node = terminal(node);
    let path = match node.kind() {
        SyntaxKind::PATH_EXPR | SyntaxKind::RECORD_EXPR => node
            .children()
            .find(|n| return n.kind() == SyntaxKind::PATH),
        SyntaxKind::CALL_EXPR => node
            .children()
            .find(is_expression)
            .filter(|n| return n.kind() == SyntaxKind::PATH_EXPR)
            .and_then(|n| return n.children().find(|n| return n.kind() == SyntaxKind::PATH)),
        _ => None,
    };
    return path.map(|path| return path.text().to_string());
}

fn handler(mut node: SyntaxNode) -> Option<String> {
    loop {
        node = unwrapped(node);
        let call = ast::MethodCallExpr::cast(node.clone())?;
        if call.name_ref().is_some_and(|name| {
            return matches!(name.text().as_str(), "map_err" | "ok_or_else" | "or_else");
        }) {
            return call
                .arg_list()
                .and_then(|args| return args.args().last())
                .and_then(|arg| return error_sink(arg.syntax().clone()));
        }
        let receiver = call.receiver()?;
        node = receiver.syntax().clone();
    }
}

fn assertion(node: &SyntaxNode) -> bool {
    let call = ast::MacroCall::cast(node.clone()).or_else(|| {
        return node.children().find_map(ast::MacroCall::cast);
    });
    let Some(path) = call.and_then(|call| return call.path()) else {
        return false;
    };
    return matches!(
        path.syntax().text().to_string().as_str(),
        "assert"
            | "assert_eq"
            | "assert_ne"
            | "std::assert"
            | "std::assert_eq"
            | "std::assert_ne"
            | "core::assert"
            | "core::assert_eq"
            | "core::assert_ne"
    );
}

fn extraction(node: &SyntaxNode) -> bool {
    match node.kind() {
        SyntaxKind::PATH_EXPR => return true,
        SyntaxKind::FIELD_EXPR => {
            return node
                .children()
                .find(is_expression)
                .is_some_and(|base| return extraction(&base));
        }
        SyntaxKind::METHOD_CALL_EXPR => {
            let Some(call) = ast::MethodCallExpr::cast(node.clone()) else {
                return false;
            };
            return call.name_ref().is_some_and(|name| {
                return matches!(name.text().as_str(), "unwrap" | "expect");
            }) && call.receiver().is_some_and(|receiver| {
                return extraction(receiver.syntax());
            }) && call.arg_list().is_none_or(|args| {
                return args
                    .args()
                    .all(|arg| return arg.syntax().kind() == SyntaxKind::LITERAL);
            });
        }
        _ => return false,
    }
}

pub(crate) fn test_statements(container: &SyntaxNode) -> Option<usize> {
    let function = container
        .ancestors()
        .find(|node| return node.kind() == SyntaxKind::FN)?;
    let test = function
        .children()
        .filter(|node| return node.kind() == SyntaxKind::ATTR)
        .any(|attr| {
            let mut tokens = attr
                .descendants_with_tokens()
                .filter_map(|element| return element.into_token())
                .filter(|token| return !token.kind().is_trivia());
            return ["#", "[", "test", "]"].iter().all(|expected| {
                return tokens
                    .next()
                    .is_some_and(|token| return token.text() == *expected);
            }) && tokens.next().is_none();
        });
    if !test {
        return None;
    }
    return function
        .children()
        .find(|node| return node.kind() == SyntaxKind::BLOCK_EXPR)
        .and_then(|body| {
            return body
                .children()
                .find(|node| return node.kind() == SyntaxKind::STMT_LIST);
        })
        .map(|body| {
            return body
                .children()
                .filter(is_unit)
                .filter(|node| return !is_item(node) || assertion(node))
                .count();
        });
}

pub(crate) fn shape(node: &SyntaxNode) -> Shape {
    let expression = expression_node(node);
    let binding = ast::LetStmt::cast(node.clone());
    let initializer = binding
        .as_ref()
        .and_then(ast::LetStmt::initializer)
        .map_or_else(
            || return expression.clone(),
            |expr| return expr.syntax().clone(),
        );
    let deferred = deferred_ranges(&initializer);
    let root = unwrapped(initializer.clone());
    let form = if node
        .children()
        .any(|child| return child.kind() == SyntaxKind::LET_ELSE)
    {
        Form::LetElse
    } else if root.kind() == SyntaxKind::IF_EXPR
        && root
            .children()
            .find(is_expression)
            .filter(|condition| return condition.kind() == SyntaxKind::LET_EXPR)
            .and_then(|condition| return condition.children().find(is_expression))
            .is_some_and(|value| {
                return matches!(
                    unwrapped(value).kind(),
                    SyntaxKind::CALL_EXPR | SyntaxKind::METHOD_CALL_EXPR
                );
            })
    {
        Form::CallCheck
    } else {
        match root.kind() {
            SyntaxKind::LITERAL => Form::Literal,
            SyntaxKind::PATH_EXPR
            | SyntaxKind::ARRAY_EXPR
            | SyntaxKind::TUPLE_EXPR
            | SyntaxKind::RECORD_EXPR => Form::Value,
            SyntaxKind::CALL_EXPR => Form::Call,
            SyntaxKind::METHOD_CALL_EXPR => Form::Method,
            SyntaxKind::IF_EXPR => Form::Conditional,
            SyntaxKind::MATCH_EXPR => Form::Branch,
            SyntaxKind::FOR_EXPR | SyntaxKind::WHILE_EXPR | SyntaxKind::LOOP_EXPR => Form::Loop,
            SyntaxKind::MACRO_CALL | SyntaxKind::MACRO_EXPR => Form::Macro,
            SyntaxKind::CLOSURE_EXPR => Form::Closure,
            _ => Form::Other,
        }
    };
    let exiting_guard = expression.kind() == SyntaxKind::IF_EXPR
        && expression
            .children()
            .find(|body| {
                return body.kind() == SyntaxKind::BLOCK_EXPR
                    && control_body_start(&expression) == Some(range(body).start);
            })
            .and_then(|body| {
                return body
                    .children()
                    .find(|n| return n.kind() == SyntaxKind::STMT_LIST);
            })
            .and_then(|body| return body.children().filter(is_unit).last())
            .is_some_and(|last| {
                return matches!(
                    expression_node(&last).kind(),
                    SyntaxKind::RETURN_EXPR | SyntaxKind::BREAK_EXPR | SyntaxKind::CONTINUE_EXPR
                );
            });
    return Shape {
        form,
        role: if binding.is_none() && form == Form::Macro && assertion(&root) {
            StatementRole::Assertion
        } else if binding.is_some() && extraction(&root) {
            StatementRole::Extraction
        } else if root.kind() == SyntaxKind::CALL_EXPR
            || ast::MethodCallExpr::cast(root).is_some_and(|call| {
                return call.name_ref().is_some_and(|name| {
                    return matches!(name.text().as_str(), "unwrap" | "expect");
                }) && call.receiver().is_some_and(|receiver| {
                    return receiver.syntax().kind() == SyntaxKind::CALL_EXPR;
                });
            })
        {
            StatementRole::FunctionCall
        } else {
            StatementRole::Ordinary
        },
        test_statements: None,
        mutable: binding
            .and_then(|stmt| return stmt.pat())
            .is_some_and(|pat| {
                return pat
                    .syntax()
                    .descendants_with_tokens()
                    .any(|token| return token.kind() == SyntaxKind::MUT_KW);
            }),
        fallible: initializer.descendants().any(|child| {
            return child.kind() == SyntaxKind::TRY_EXPR
                && !deferred
                    .iter()
                    .any(|body| return body.contains(range(&child)));
        }),
        error_handler: handler(initializer.clone()),
        exiting_guard,
        string_inputs: initializer
            .descendants_with_tokens()
            .filter_map(|element| return element.into_token())
            .filter(|token| return token.kind() == SyntaxKind::STRING)
            .map(|token| {
                return ByteRange::new(
                    u32::from(token.text_range().start()) as usize,
                    u32::from(token.text_range().end()) as usize,
                );
            })
            .filter(|range| return !deferred.iter().any(|body| return body.contains(*range)))
            .collect(),
    };
}

pub(crate) fn scope(container: &SyntaxNode) -> Scope {
    return match container
        .parent()
        .and_then(|parent| return parent.parent())
        .map(|owner| return owner.kind())
    {
        Some(SyntaxKind::FOR_EXPR | SyntaxKind::WHILE_EXPR | SyntaxKind::LOOP_EXPR) => Scope::Loop,
        Some(SyntaxKind::CLOSURE_EXPR) => Scope::Closure,
        _ => Scope::Ordinary,
    };
}
