//! Policy defaults and rule membership.

use rust_statement_spacing_core::{Config, Rule};

#[test]
fn default_settings() {
    let c = Config::default();
    assert!(c.enabled().has(Rule::Bindings));
    assert!(!c.enabled().has(Rule::Layout));
    assert_eq!(c.grouping.max_before_control, 1);
    assert_eq!(c.exits.short_block_max_statements, 4);
}

#[test]
fn enable_and_disable_overrides() {
    let mut c = Config::default();
    c.disable.push(Rule::Bindings);
    c.enable.push(Rule::Layout);
    assert!(!c.enabled().has(Rule::Bindings));
    assert!(c.enabled().has(Rule::Layout));
    assert!(c.enabled().has(Rule::Exit));

    let only_exit = Config::only(&[Rule::Exit]);
    assert!(only_exit.enabled().has(Rule::Exit));
    assert!(!only_exit.enabled().has(Rule::Bindings));
    assert!(!only_exit.enabled().has(Rule::Layout));
    assert!(only_exit.validate().is_ok());
}
