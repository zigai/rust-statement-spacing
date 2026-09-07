use ra_ap_syntax::{
    AstNode, SyntaxKind, SyntaxNode,
    ast::{self, HasArgList as _},
};
use rust_statement_spacing_core::{ByteRange, Form, Scope, Shape};

use crate::units::{
    control_body_start, deferred_ranges, expression_node, is_expression, is_unit, range,
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
