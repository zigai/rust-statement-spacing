use crate::units::{
    control_body_start, expression_node, is_deferred, is_expression, is_item, is_unit, range,
};
use ra_ap_syntax::{
    AstNode, Edition, NodeOrToken, SyntaxKind, SyntaxNode, WalkEvent,
    ast::{self, HasArgList as _},
};
use rust_statement_spacing_core::{ByteRange, Form, Scope, Shape, StatementRole};

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

fn pins_value(root: &SyntaxNode) -> bool {
    let call = ast::MacroCall::cast(root.clone())
        .or_else(|| return root.children().find_map(ast::MacroCall::cast));
    let Some(call) = call else {
        return false;
    };
    if !call.path().is_some_and(|path| {
        return matches!(
            path.syntax().text().to_string().as_str(),
            "assert_eq"
                | "assert_ne"
                | "std::assert_eq"
                | "std::assert_ne"
                | "core::assert_eq"
                | "core::assert_ne"
        );
    }) {
        return false;
    }
    let Some(tree) = call.token_tree() else {
        return false;
    };
    let text = tree.syntax().text().to_string();
    let Some(body) = text.get(1..text.len().saturating_sub(1)) else {
        return false;
    };
    let arguments = format!("arguments({body})");
    let Ok(ast::Expr::CallExpr(call)) = ast::Expr::parse(&arguments, Edition::Edition2024).ok()
    else {
        return false;
    };
    let Some(arguments) = call.arg_list() else {
        return false;
    };
    let mut arguments = arguments.args();
    let (Some(actual), Some(expected)) = (arguments.next(), arguments.next()) else {
        return false;
    };
    let literal = match expected {
        ast::Expr::Literal(_) => true,
        ast::Expr::PrefixExpr(prefix) => {
            prefix.op_kind() == Some(ast::UnaryOp::Neg)
                && prefix
                    .expr()
                    .is_some_and(|value| return matches!(value, ast::Expr::Literal(_)))
        }
        _ => false,
    };
    return literal
        && !actual
            .syntax()
            .descendants()
            .any(|node| return ast::MethodCallExpr::can_cast(node.kind()))
        && !is_size_named(&actual.syntax().text().to_string());
}

fn is_size_named(argument: &str) -> bool {
    let stripped = argument.trim().trim_matches(|mark| {
        return mark == '(' || mark == ')' || mark == '&' || mark == '*' || mark == ' ';
    });
    let tail = stripped.rsplit(['.', ':']).next().unwrap_or(stripped);
    let name = tail.trim();
    if name.is_empty() {
        return false;
    }
    return ["len", "count", "counts", "byte", "bytes", "size", "entries"]
        .iter()
        .any(|word| return name == *word || name.ends_with(&format!("_{word}")));
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

fn loop_body_statements(expression: &SyntaxNode) -> Option<usize> {
    if !matches!(
        expression.kind(),
        SyntaxKind::FOR_EXPR | SyntaxKind::WHILE_EXPR | SyntaxKind::LOOP_EXPR
    ) {
        return None;
    }
    let start = control_body_start(expression)?;
    let body = expression.children().find(|child| {
        return child.kind() == SyntaxKind::BLOCK_EXPR && range(child).start == start;
    })?;
    let list = body
        .children()
        .find(|child| return child.kind() == SyntaxKind::STMT_LIST)?;
    return Some(
        list.children()
            .filter(is_unit)
            .filter(|node| return !is_item(node))
            .count(),
    );
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

    let (fallible, string_inputs) = immediate_inputs(&initializer);
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
        loop_body_statements: loop_body_statements(&expression),
        pins_value: pins_value(&root),
        conditional_action: ast::IfExpr::cast(expression).and_then(|conditional| {
            if conditional.else_branch().is_some()
                || conditional.condition()?.syntax().descendants().any(|node| {
                    return node.kind() == SyntaxKind::LET_EXPR;
                })
            {
                return None;
            }

            let body = conditional.then_branch()?.stmt_list()?;
            let mut statements = body.syntax().children().filter(is_unit);
            let statement = statements.next()?;
            if statements.next().is_some()
                || expression_node(&statement).kind() != SyntaxKind::METHOD_CALL_EXPR
            {
                return None;
            }
            return Some(range(&statement));
        }),
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
        fallible,
        error_handler: handler(initializer),
        exiting_guard,
        string_inputs,
    };
}

fn immediate_inputs(initializer: &SyntaxNode) -> (bool, Vec<ByteRange>) {
    let mut fallible = false;
    let mut string_inputs = Vec::new();
    let mut walk = initializer.preorder_with_tokens();
    while let Some(event) = walk.next() {
        match event {
            WalkEvent::Enter(NodeOrToken::Node(node)) => {
                if node != *initializer && is_deferred(&node) {
                    walk.skip_subtree();
                } else if node.kind() == SyntaxKind::TRY_EXPR {
                    fallible = true;
                }
            }
            WalkEvent::Enter(NodeOrToken::Token(token)) if token.kind() == SyntaxKind::STRING => {
                string_inputs.push(ByteRange::new(
                    u32::from(token.text_range().start()) as usize,
                    u32::from(token.text_range().end()) as usize,
                ));
            }
            WalkEvent::Enter(NodeOrToken::Token(_)) | WalkEvent::Leave(_) => {}
        }
    }
    return (fallible, string_inputs);
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
