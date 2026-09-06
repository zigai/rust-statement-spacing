//! Public API integration tests.

#[test]
fn exposes_the_package_name() {
    assert_eq!(rust_statement_spacing::package_name(), env!("CARGO_PKG_NAME"));
}
