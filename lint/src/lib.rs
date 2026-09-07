#![feature(rustc_private)]

extern crate rustc_data_structures;
extern crate rustc_errors;
extern crate rustc_hir;
extern crate rustc_lint;
extern crate rustc_middle;
extern crate rustc_session;
extern crate rustc_span;

dylint_linting::dylint_library!();

mod config;
mod hir;
mod pass;
mod protocol;
mod spans;
mod workspace;

use rust_statement_spacing_core::Rule;
use rustc_data_structures::marker::IntoDynSyncSend;
use rustc_lint::LintStore;
use rustc_session::lint::{Lint, LintId};
use rustc_session::{Session, declare_lint};

declare_lint!(pub STATEMENT_SPACING_BINDINGS, Warn, "separate unrelated binding groups");
declare_lint!(pub STATEMENT_SPACING_EXPRESSIONS, Warn, "separate unrelated operations");
declare_lint!(pub STATEMENT_SPACING_CONTROL_FLOW, Warn, "keep bounded relevant control-flow setup");
declare_lint!(pub STATEMENT_SPACING_AFTER_BLOCK, Warn, "separate phases after standalone blocks");
declare_lint!(pub STATEMENT_SPACING_EXIT, Warn, "separate explicit exits and final block values");
declare_lint!(pub STATEMENT_SPACING_RESULT_CHECK, Warn, "join immediate Result/Option checks");
declare_lint!(pub STATEMENT_SPACING_ITEM_SPACING, Warn, "separate major items and functions");
declare_lint!(pub STATEMENT_SPACING_LAYOUT, Warn, "remove surplus vertical whitespace");

pub(crate) fn lint(rule: Rule) -> &'static Lint {
    match rule {
        Rule::Bindings => STATEMENT_SPACING_BINDINGS,
        Rule::Expressions => STATEMENT_SPACING_EXPRESSIONS,
        Rule::ControlFlow => STATEMENT_SPACING_CONTROL_FLOW,
        Rule::AfterBlock => STATEMENT_SPACING_AFTER_BLOCK,
        Rule::Exit => STATEMENT_SPACING_EXIT,
        Rule::ResultCheck => STATEMENT_SPACING_RESULT_CHECK,
        Rule::ItemSpacing => STATEMENT_SPACING_ITEM_SPACING,
        Rule::Layout => STATEMENT_SPACING_LAYOUT,
    }
}

#[unsafe(no_mangle)]
pub fn register_lints(sess: &Session, store: &mut LintStore) {
    let lints: Vec<_> = Rule::ALL.into_iter().map(lint).collect();
    store.register_lints(&lints);
    store.register_group(
        true,
        "statement_spacing",
        None,
        lints.iter().map(|l| LintId::of(l)).collect(),
    );
    if let Err(error) = dylint_linting::try_init_config(sess) {
        sess.dcx().err(format!(
            "statement_spacing: cannot initialize dylint.toml: {error}"
        ));
        return;
    }
    let config = match dylint_linting::config::<config::ConfigTable>("statement_spacing") {
        Ok(value) => value.unwrap_or_default(),
        Err(error) => {
            sess.dcx()
                .err(format!("statement_spacing: invalid configuration: {error}"));
            return;
        }
    };
    let (config, workspace) = match config.resolve(workspace::root()) {
        Ok(value) => value,
        Err(error) => {
            sess.dcx().err(format!("statement_spacing: {error}"));
            return;
        }
    };
    // Bridge standard Send/Sync for globset's trait objects to rustc's dynamic markers.
    let workspace = IntoDynSyncSend(workspace);
    store.register_late_pass(move |_| {
        Box::new(pass::Spacing::new(config.clone(), workspace.clone().0))
    });
}
