//! Core library API for `rust-statement-spacing`.

/// Returns this package's Cargo name.
///
/// # Examples
///
/// ```
/// assert_eq!(rust_statement_spacing::package_name(), "rust-statement-spacing");
/// ```
#[must_use]
pub const fn package_name() -> &'static str {
    env!("CARGO_PKG_NAME")
}

#[cfg(test)]
mod tests {
    use super::package_name;

    #[test]
    fn package_name_matches_cargo_metadata() {
        assert_eq!(package_name(), env!("CARGO_PKG_NAME"));
    }
}
