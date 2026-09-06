//! Compiler-version-sensitive semantic extraction. No lexical name resolver.

use rust_statement_spacing_core::Place;
use rustc_hir::def::Res;
use rustc_hir::{self as hir, Expr, ExprKind, Pat, PatKind, QPath};
use rustc_lint::LateContext;
use rustc_middle::ty;
use rustc_span::Symbol;

pub fn local_key(id: hir::HirId) -> String {
    format!("{id:?}")
}

pub fn place(expression: &Expr<'_>) -> Option<Place> {
    match expression.kind {
        ExprKind::Path(QPath::Resolved(_, path)) => {
            if let Res::Local(id) = path.res {
                let is_self = path
                    .segments
                    .last()
                    .is_some_and(|s| s.ident.name.as_str() == "self");
                Some(Place {
                    local: local_key(id),
                    projections: Vec::new(),
                    is_self,
                })
            } else {
                None
            }
        }
        ExprKind::Field(base, field) => {
            let mut place = place(base)?;
            place.projections.push(format!(".{}", field.name));
            Some(place)
        }
        ExprKind::Index(base, _, _) => {
            let mut place = place(base)?;
            place.projections.push("[]".into());
            Some(place)
        }
        ExprKind::AddrOf(_, _, inner) | ExprKind::DropTemps(inner) => place(inner),
        // Dereference/alias analysis is intentionally not invented here.
        _ => None,
    }
}

fn standard_adt(cx: &LateContext<'_>, expression: &Expr<'_>) -> bool {
    let ty = cx.typeck_results().expr_ty(expression).peel_refs();
    let ty::Adt(adt, _) = ty.kind() else {
        return false;
    };
    ["Result", "Option"]
        .iter()
        .any(|name| cx.tcx.is_diagnostic_item(Symbol::intern(name), adt.did()))
}

fn inspected_local(cx: &LateContext<'_>, expression: &Expr<'_>) -> Option<Place> {
    let place = place(expression)?;
    if !place.projections.is_empty() || !standard_adt(cx, expression) {
        return None;
    }
    Some(place)
}

pub fn pattern_check(cx: &LateContext<'_>, pat: &Pat<'_>, init: &Expr<'_>) -> Option<Place> {
    // A typed refutable non-binding pattern on standard Option/Result cannot be
    // a user-defined enum which merely has a variant spelled Some or Err.
    if matches!(pat.kind, PatKind::Binding(..) | PatKind::Wild) {
        return None;
    }
    inspected_local(cx, init)
}

pub fn inspection(cx: &LateContext<'_>, expression: &Expr<'_>) -> Option<Place> {
    match expression.kind {
        ExprKind::Let(let_expr) => pattern_check(cx, let_expr.pat, let_expr.init),
        ExprKind::Match(scrutinee, _, hir::MatchSource::Normal) => inspected_local(cx, scrutinee),
        ExprKind::MethodCall(segment, receiver, arguments, _) if arguments.is_empty() => {
            if !matches!(
                segment.ident.name.as_str(),
                "is_err" | "is_ok" | "is_some" | "is_none"
            ) {
                return None;
            }
            let checked = inspected_local(cx, receiver)?;
            let def = cx
                .typeck_results()
                .type_dependent_def_id(expression.hir_id)?;
            // Resolve the actual method definition, not its spelling. Numbered
            // impl path components are intentionally not hard-coded.
            let path = cx.tcx.def_path_str(def);
            if path.starts_with("core::result::") || path.starts_with("core::option::") {
                Some(checked)
            } else {
                None
            }
        }
        _ => None,
    }
}
