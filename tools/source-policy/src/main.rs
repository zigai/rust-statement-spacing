//! Repository stub policy uses Rust tokens, including those in macro definitions.

use std::error::Error;
use std::fs;
use std::path::Path;

use ra_ap_syntax::{AstNode, Edition, SourceFile, SyntaxKind};

fn stub_calls(source: &str) -> Vec<usize> {
    let parse = SourceFile::parse(source, Edition::Edition2024);
    let tokens: Vec<_> = parse
        .tree()
        .syntax()
        .descendants_with_tokens()
        .filter_map(|element| return element.into_token())
        .filter(|token| {
            return !matches!(token.kind(), SyntaxKind::WHITESPACE | SyntaxKind::COMMENT);
        })
        .collect();
    return tokens
        .windows(3)
        .filter(|tokens| {
            let [t0, t1, t2] = tokens else {
                return false;
            };
            return t0.kind() == SyntaxKind::IDENT
                && matches!(t0.text().trim_start_matches("r#"), "todo" | "unimplemented")
                && t1.kind() == SyntaxKind::BANG
                && matches!(
                    t2.kind(),
                    SyntaxKind::L_PAREN | SyntaxKind::L_BRACK | SyntaxKind::L_CURLY
                );
        })
        .filter_map(|tokens| {
            let [t0, ..] = tokens else {
                return None;
            };
            return Some(u32::from(t0.text_range().start()) as usize);
        })
        .collect();
}

fn scan(root: &Path) -> Result<Vec<String>, Box<dyn Error>> {
    let mut directories = vec![root.to_path_buf()];
    let mut violations = Vec::new();
    // Match the source-tree boundary of scripts/static_checks.py. Do not follow
    // symlinks into other projects, or inspect generated build/cache output.
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let name = entry.file_name();
            if matches!(
                name.to_str(),
                Some(
                    "target" | ".git" | ".statement-spacing" | "__pycache__" | "validation-output"
                )
            ) {
                continue;
            }
            let kind = entry.file_type()?;
            let path = entry.path();
            #[expect(
                clippy::filetype_is_file,
                reason = "Source policy scanner requires regular Rust source files and excludes symlinks, fifos, and devices"
            )]
            let is_regular_file = kind.is_file();
            if kind.is_dir() {
                directories.push(path);
            } else if is_regular_file
                && path
                    .extension()
                    .is_some_and(|extension| return extension == "rs")
            {
                let source = fs::read_to_string(&path)?;
                for offset in stub_calls(&source) {
                    violations.push(format!(
                        "{}:byte {offset}",
                        path.strip_prefix(root)?.display()
                    ));
                }
            }
        }
    }
    violations.sort();
    return Ok(violations);
}

fn main() -> Result<(), Box<dyn Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .ok_or("source-policy must be inside the repository tools directory")?;
    let violations = scan(root)?;
    if !violations.is_empty() {
        return Err(format!("unfinished stubs: {violations:#?}").into());
    }
    println!("source policy passed");
    return Ok(());
}

#[cfg(test)]
mod tests {
    use super::stub_calls;

    #[test]
    fn detects_stub_invocations_including_macro_bodies() {
        for source in [
            "fn f() { todo!(); }",
            "fn f() { std::unimplemented!(\"reason\"); }",
            "fn f() { todo /* reason */ ! {}; }",
            "fn f() { r#unimplemented![]; }",
            "macro_rules! later { () => { todo!() }; }",
        ] {
            assert_eq!(stub_calls(source).len(), 1, "{source}");
        }
    }

    #[test]
    fn allows_stub_spelling_as_data_and_ordinary_identifiers() {
        for source in [
            "// todo!()\nfn f() {}",
            "/* unimplemented!() /* nested */ */ fn f() {}",
            "fn f() { let example = \"todo!()\"; }",
            "fn f() { let example = r###\"unimplemented!{}\"###; }",
            "fn f() { let example = b\"todo!()\"; }",
            "fn f() { example!(\"todo!()\"); }",
            "#[doc = \"unimplemented!()\"] fn f() {}",
            "fn todo() {} fn f() { todo(); }",
            "macro_rules! todo { () => {} }",
        ] {
            assert!(stub_calls(source).is_empty(), "{source}");
        }
    }
}
