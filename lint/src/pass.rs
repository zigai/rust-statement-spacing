use std::collections::BTreeMap;
use std::path::PathBuf;

use rust_statement_spacing_core::{Config, Place, Rule, RuleMask, plan};
use rust_statement_spacing_syntax::{
    Anchor, Event, EventKind, SemanticIndex, parse_source, token_fingerprint,
};
use rustc_errors::{
    Applicability, Diag, DiagCtxtHandle, Diagnostic, EmissionGuarantee, Level as DiagnosticLevel,
};
use rustc_hir::{self as hir, ExprKind, PatKind, StmtKind};
use rustc_lint::{LateContext, LateLintPass};
use rustc_session::impl_lint_pass;
use rustc_session::lint::Level;
use rustc_span::{BytePos, FileName, Span};
use sha2::{Digest, Sha256};

use crate::hir::{direct_callee, inspection, local_key, mutable_receiver, pattern_check, place};
use crate::spans::{self, DiagnosticAnchor};
use crate::workspace::{self, Workspace};
use crate::{
    STATEMENT_SPACING_AFTER_BLOCK, STATEMENT_SPACING_BINDINGS, STATEMENT_SPACING_CONTROL_FLOW,
    STATEMENT_SPACING_EXIT, STATEMENT_SPACING_EXPRESSIONS, STATEMENT_SPACING_ITEM_SPACING,
    STATEMENT_SPACING_LAYOUT, STATEMENT_SPACING_RESULT_CHECK,
};

struct FileData {
    path: PathBuf,
    start: BytePos,
    source: String,
    semantics: SemanticIndex,
}

struct SpacingDiagnostic {
    message: String,
    span: Span,
    replacement: Option<String>,
}

impl<'a, G: EmissionGuarantee> Diagnostic<'a, G> for SpacingDiagnostic {
    fn into_diag(self, dcx: DiagCtxtHandle<'a>, level: DiagnosticLevel) -> Diag<'a, G> {
        let mut diag = Diag::new(dcx, level, self.message);
        if let Some(replacement) = self.replacement {
            diag.span_suggestion(
                self.span,
                "adjust the blank-line boundary",
                replacement,
                Applicability::MachineApplicable,
            );
        } else {
            diag.help("this compact/protected layout has no safe whitespace-only fix");
        }
        diag
    }
}

pub struct Spacing {
    config: Config,
    workspace: Workspace,
    files: BTreeMap<u32, FileData>,
    anchors: Vec<DiagnosticAnchor>,
}

impl_lint_pass!(Spacing => [
    STATEMENT_SPACING_BINDINGS, STATEMENT_SPACING_EXPRESSIONS, STATEMENT_SPACING_CONTROL_FLOW,
    STATEMENT_SPACING_AFTER_BLOCK, STATEMENT_SPACING_EXIT, STATEMENT_SPACING_RESULT_CHECK,
    STATEMENT_SPACING_ITEM_SPACING, STATEMENT_SPACING_LAYOUT,
]);

impl Spacing {
    pub fn new(config: Config, workspace: Workspace) -> Self {
        Self {
            config,
            workspace,
            files: BTreeMap::new(),
            anchors: Vec::new(),
        }
    }

    fn file_range(
        &mut self,
        cx: &LateContext<'_>,
        span: Span,
    ) -> Option<(u32, rust_statement_spacing_core::ByteRange)> {
        if span.is_dummy() || span.from_expansion() {
            return None;
        }
        let file = cx.tcx.sess.source_map().lookup_source_file(span.lo());
        let FileName::Real(name) = &file.name else {
            return None;
        };
        let path = workspace::canonical(name.local_path()?);
        let source = file.src.as_ref()?;
        if !self.workspace.includes(&path, source) {
            return None;
        }
        let range = spans::normalized_range(file.start_pos, span, source.len())?;
        let key = file.start_pos.0;
        self.files.entry(key).or_insert_with(|| FileData {
            path,
            start: file.start_pos,
            source: source.to_string(),
            semantics: SemanticIndex::default(),
        });
        Some((key, range))
    }

    fn anchor(&mut self, cx: &LateContext<'_>, id: hir::HirId, span: Span) {
        let Some((key, range)) = self.file_range(cx, span) else {
            return;
        };
        let mut enabled = RuleMask::all();
        for rule in Rule::ALL {
            if cx
                .tcx
                .lint_level_spec_at_node(crate::lint(rule), id)
                .level()
                == Level::Allow
            {
                enabled = enabled.without(rule);
            }
        }
        let index = self.anchors.len();
        self.anchors.push(DiagnosticAnchor { hir_id: id, span });
        self.files
            .get_mut(&key)
            .expect("file inserted above")
            .semantics
            .anchors
            .push(Anchor {
                range,
                id: index,
                enabled,
            });
    }

    fn event(&mut self, cx: &LateContext<'_>, span: Span, kind: EventKind, place: Option<Place>) {
        let Some((key, range)) = self.file_range(cx, span) else {
            return;
        };
        self.files
            .get_mut(&key)
            .expect("file inserted above")
            .semantics
            .events
            .push(Event { range, kind, place });
    }

    fn finish(&mut self, cx: &LateContext<'_>) {
        let edition = cx.tcx.sess.edition().to_string();
        let mut checked = Vec::new();
        let mut token_hashes = BTreeMap::new();
        let mut skipped_boundaries = 0;
        for (_, file) in std::mem::take(&mut self.files) {
            let result = (|| -> Result<(), String> {
                let text = spans::read_source(&file.path, &file.source)?;
                let mut parsed = parse_source(&text.normalized, &edition)?;
                parsed.attach(&text.normalized, &file.semantics);
                let plan = plan(&self.config, &text.normalized, &parsed.model)?;
                skipped_boundaries += plan.skipped_boundaries;
                let candidate =
                    rust_statement_spacing_core::apply_edits(&text.normalized, &plan.edits())?;
                let tokens = token_fingerprint(&text.normalized, &edition)?;
                if tokens != token_fingerprint(&candidate, &edition)? {
                    return Err(
                        "internal error: a proposed fix changed a code/comment/literal token"
                            .into(),
                    );
                }
                let encoded = serde_json::to_vec(&tokens).map_err(|error| error.to_string())?;
                token_hashes.insert(
                    file.path.display().to_string(),
                    format!("{:x}", Sha256::digest(encoded)),
                );
                for finding in plan.findings {
                    let Some(anchor) = self.anchors.get(finding.anchor) else {
                        return Err("internal error: missing diagnostic source anchor".into());
                    };
                    let span = spans::source_span(anchor.span, file.start, finding.range)
                        .ok_or("source span overflow")?;
                    cx.tcx.emit_node_span_lint(
                        crate::lint(finding.rule),
                        anchor.hir_id,
                        span,
                        SpacingDiagnostic {
                            message: finding.message,
                            span,
                            replacement: finding
                                .edit
                                .map(|edit| text.original_newlines(&edit.replacement)),
                        },
                    );
                }
                checked.push(file.path.display().to_string());
                Ok(())
            })();
            if let Err(error) = result {
                cx.tcx.sess.dcx().err(format!(
                    "statement_spacing: {}: {error}",
                    file.path.display()
                ));
            }
        }
        if let Err(error) = crate::protocol::emit(&checked, &token_hashes, skipped_boundaries) {
            cx.tcx.sess.dcx().err(format!(
                "statement_spacing: cannot write verification handshake: {error}"
            ));
        }
    }
}

impl<'tcx> LateLintPass<'tcx> for Spacing {
    fn check_item(&mut self, cx: &LateContext<'tcx>, item: &'tcx hir::Item<'tcx>) {
        self.anchor(cx, item.hir_id(), item.span);
    }
    fn check_trait_item(&mut self, cx: &LateContext<'tcx>, item: &'tcx hir::TraitItem<'tcx>) {
        self.anchor(cx, item.hir_id(), item.span);
    }
    fn check_impl_item(&mut self, cx: &LateContext<'tcx>, item: &'tcx hir::ImplItem<'tcx>) {
        self.anchor(cx, item.hir_id(), item.span);
    }
    fn check_foreign_item(&mut self, cx: &LateContext<'tcx>, item: &'tcx hir::ForeignItem<'tcx>) {
        self.anchor(cx, item.hir_id(), item.span);
    }
    fn check_block(&mut self, cx: &LateContext<'tcx>, block: &'tcx hir::Block<'tcx>) {
        self.anchor(cx, block.hir_id, block.span);
    }
    fn check_stmt(&mut self, cx: &LateContext<'tcx>, stmt: &'tcx hir::Stmt<'tcx>) {
        self.anchor(cx, stmt.hir_id, stmt.span);
        if let StmtKind::Expr(expression) | StmtKind::Semi(expression) = stmt.kind {
            if let Some(callee) = direct_callee(cx, expression) {
                self.event(cx, expression.span, EventKind::DirectCallee(callee), None);
            }
        }
        if let StmtKind::Let(local) = stmt.kind {
            if local.els.is_some() {
                if let Some(init) = local.init {
                    if let Some(checked) = pattern_check(cx, local.pat, init) {
                        self.event(cx, init.span, EventKind::Inspect, Some(checked));
                    }
                }
            }
        }
    }
    fn check_pat(&mut self, cx: &LateContext<'tcx>, pat: &'tcx hir::Pat<'tcx>) {
        if let PatKind::Binding(_, id, ident, _) = pat.kind {
            let mut place = Place::local(local_key(id));
            place.is_self = ident.name.as_str() == "self";
            self.event(cx, ident.span, EventKind::Define, Some(place));
        }
    }
    fn check_expr(&mut self, cx: &LateContext<'tcx>, expression: &'tcx hir::Expr<'tcx>) {
        // A written `for` is lowered to DropTemps(Match(ForLoopDesugar)).
        // Its outer span carries a compiler expansion mark even at tail
        // position, where no source statement supplies a second anchor.
        // Recover only that wrapper's immediate call site: macro-generated
        // loops must retain their macro context and remain ineligible.
        let span = match expression.kind {
            ExprKind::DropTemps(inner)
                if matches!(
                    inner.kind,
                    ExprKind::Match(_, _, hir::MatchSource::ForLoopDesugar)
                ) =>
            {
                expression.span.ctxt().outer_expn_data().call_site
            }
            _ => expression.span,
        };
        self.anchor(cx, expression.hir_id, span);
        if let Some(place) = place(expression) {
            self.event(cx, expression.span, EventKind::Read, Some(place));
        }
        match expression.kind {
            ExprKind::AddrOf(_, hir::Mutability::Mut, inner) => {
                // Mutable references and raw addresses expose this place to
                // mutation, including through nested FFI calls and pointer
                // casts. This is a possible output, not proof of a write.
                if let Some(place) = place(inner) {
                    self.event(cx, inner.span, EventKind::Write, Some(place));
                }
            }
            ExprKind::Assign(left, _, _) | ExprKind::AssignOp(_, left, _) => {
                if let Some(place) = place(left) {
                    self.event(cx, left.span, EventKind::Write, Some(place));
                } else {
                    self.event(cx, left.span, EventKind::Unknown, None);
                }
            }
            ExprKind::MethodCall(_, receiver, _, _) => {
                if let Some(place) = place(receiver) {
                    let kind = if mutable_receiver(cx, receiver) {
                        self.event(cx, receiver.span, EventKind::Receiver, Some(place.clone()));
                        EventKind::MutatingReceiver
                    } else {
                        EventKind::Receiver
                    };
                    self.event(cx, receiver.span, kind, Some(place));
                }
            }
            _ => {}
        }
        if let Some(checked) = inspection(cx, expression) {
            // Match spans include their bodies; put the inspection on the
            // scrutinee/header so the source adapter can classify it precisely.
            let span = match expression.kind {
                ExprKind::Match(scrutinee, _, _) => scrutinee.span,
                _ => expression.span,
            };
            self.event(cx, span, EventKind::Inspect, Some(checked));
        }
    }
    fn check_crate_post(&mut self, cx: &LateContext<'tcx>) {
        self.finish(cx);
    }
}
