use ra_ap_syntax::{AstNode, SyntaxKind, SyntaxNode, ast};
use rust_statement_spacing_core::edits::is_trivia;
use rust_statement_spacing_core::{
    ByteRange, Facts, Rule, RuleMask, SourceModel, StatementRole, Unit, UnitKind, UnitList,
};

use crate::comments::classify_gap;
use crate::protection::{inside_token_tree, protected};
use crate::semantics::SemanticIndex;
use crate::shape;
use crate::units::{
    code_range, control_body_start, deferred_ranges, expression_node, guard, immediate_body_first,
    is_expression, is_item, is_unit, range, unit_kind,
};

fn make_unit(
    node: &SyntaxNode,
    semantics: Option<&SemanticIndex>,
    test_statements: Option<usize>,
) -> Unit {
    let code = code_range(node);
    let expression = expression_node(node);
    let mut shape = shape::shape(node);
    shape.test_statements = test_statements;
    let category = if shape.role == StatementRole::Assertion
        && test_statements.is_some()
        && node
            .parent()
            .is_some_and(|parent| return parent.kind() == SyntaxKind::STMT_LIST)
    {
        // Statement-position macros also parse as items. Known assertion
        // invocations execute here; their token trees still remain opaque.
        UnitKind::Expression
    } else {
        unit_kind(node)
    };
    let header = if category == UnitKind::Control {
        let end = control_body_start(&expression).unwrap_or(code.end);
        Some(ByteRange::new(code.start, end))
    } else if category == UnitKind::Let {
        // The initializer of a let-else is inspected; its else body is not.
        let end = node
            .children()
            .find(|child| return child.kind() == SyntaxKind::LET_ELSE)
            .map_or(code.end, |body| return range(&body).start);
        Some(ByteRange::new(code.start, end))
    } else {
        None
    };
    let declaration = if category == UnitKind::Let {
        node.children()
            .find(|child| return ast::Pat::can_cast(child.kind()))
            .map(|pat| return range(&pat))
    } else {
        None
    };
    let deferred = deferred_ranges(node);
    let opaque = node
        .descendants()
        .any(|n| matches!(n.kind(), SyntaxKind::MACRO_CALL | SyntaxKind::MACRO_EXPR))
        || !deferred.is_empty();
    let first = immediate_body_first(&expression);
    let anchor = semantics.and_then(|s| return s.anchor(code));
    let mut facts = semantics.map_or_else(Facts::default, |semantics| {
        return semantics.facts(code, declaration, header, &first, &deferred, opaque);
    });
    if expression.kind() == SyntaxKind::IF_EXPR {
        let condition = expression.children().find(is_expression);
        if condition.is_none_or(|condition| {
            return !matches!(
                condition.kind(),
                SyntaxKind::LET_EXPR | SyntaxKind::METHOD_CALL_EXPR
            );
        }) {
            facts.check_of = None;
        }
    }
    if let Some(checked) = &facts.check_of
        && facts.header_reads.iter().any(|read| return read != checked)
    {
        facts.check_of = None;
    }
    return Unit {
        range: range(node),
        code_range: code,
        kind: category,
        shape,
        is_tail: is_expression(node) && !is_item(node),
        is_guard: guard(&expression),
        is_empty_loop: expression.kind() == SyntaxKind::FOR_EXPR
            && expression.children().any(|child| {
                return child.kind() == SyntaxKind::BLOCK_EXPR
                    && control_body_start(&expression)
                        .is_some_and(|start| return range(&child).start == start)
                    && child.children().any(|list| {
                        return list.kind() == SyntaxKind::STMT_LIST
                            && !list.children().any(|node| return is_unit(&node));
                    });
            }),
        is_loop_exit: matches!(
            expression.kind(),
            SyntaxKind::BREAK_EXPR | SyntaxKind::CONTINUE_EXPR
        ) && !expression
            .children()
            .any(|node| return is_expression(&node)),
        is_bare_return: expression.kind() == SyntaxKind::RETURN_EXPR
            && !expression
                .children()
                .any(|node| return is_expression(&node)),
        active: anchor.is_some(),
        protected: protected(node),
        facts,
        enabled: anchor.map_or(RuleMask::none(), |a| return a.enabled),
        anchor: anchor.map_or(0, |a| return a.id),
    };
}

pub(crate) fn lower(
    root: &SyntaxNode,
    source: &str,
    semantics: Option<&SemanticIndex>,
) -> SourceModel {
    let mut model = SourceModel::default();
    for container in root.descendants() {
        let k = container.kind();
        let item_list = matches!(
            k,
            SyntaxKind::SOURCE_FILE
                | SyntaxKind::ITEM_LIST
                | SyntaxKind::ASSOC_ITEM_LIST
                | SyntaxKind::EXTERN_ITEM_LIST
        );
        if (k != SyntaxKind::STMT_LIST && !item_list) || inside_token_tree(&container) {
            continue;
        }
        let test_statements = shape::test_statements(&container);
        let units: Vec<_> = container
            .children()
            .filter(|node| {
                if item_list {
                    return is_item(node);
                } else {
                    return is_unit(node);
                }
            })
            .map(|node| {
                let unit = make_unit(&node, semantics, test_statements);
                if unit.active && !unit.protected && unit.enabled.has(Rule::Layout) {
                    for attribute in node
                        .children()
                        .filter(|child| return child.kind() == SyntaxKind::ATTR)
                    {
                        let Some(space) = attribute.next_sibling_or_token() else {
                            continue;
                        };
                        if space.kind() == SyntaxKind::WHITESPACE
                            && space
                                .next_sibling_or_token()
                                .is_some_and(|next| return next.kind() != SyntaxKind::COMMENT)
                        {
                            model.layout_edges.push((
                                ByteRange::new(
                                    u32::from(space.text_range().start()) as usize,
                                    u32::from(space.text_range().end()) as usize,
                                ),
                                unit.anchor,
                            ));
                        }
                    }
                }
                return unit;
            })
            .collect();
        let gaps = units
            .windows(2)
            .filter_map(|pair| {
                let [u0, u1] = pair else {
                    return None;
                };
                return Some(classify_gap(source, u0.range, u1.range));
            })
            .collect();
        let count = units
            .iter()
            .filter(|unit| return !unit.kind.is_item())
            .count();
        if k == SyntaxKind::STMT_LIST && !protected(&container) {
            let left = container
                .children_with_tokens()
                .filter_map(|n| return n.into_token())
                .find(|token| return token.text() == "{");
            let right = container
                .children_with_tokens()
                .filter_map(|n| return n.into_token())
                .find(|token| return token.text() == "}");
            if let (Some(first), Some(last), Some(left), Some(right)) =
                (units.first(), units.last(), left, right)
            {
                for (edge, unit) in [
                    (
                        ByteRange::new(
                            u32::from(left.text_range().end()) as usize,
                            first.range.start,
                        ),
                        first,
                    ),
                    (
                        ByteRange::new(
                            last.range.end,
                            u32::from(right.text_range().start()) as usize,
                        ),
                        last,
                    ),
                ] {
                    if unit.active
                        && !unit.protected
                        && unit.enabled.has(Rule::Layout)
                        && source.get(edge.as_range()).is_some_and(|text| {
                            return text.bytes().all(is_trivia);
                        })
                    {
                        model.layout_edges.push((edge, unit.anchor));
                    }
                }
            }
        }
        model.lists.push(UnitList {
            units,
            gaps,
            executable_count: count,
            item_list,
            scope: shape::scope(&container),
        });
    }
    return model;
}
