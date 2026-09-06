use ra_ap_syntax::{AstNode, Edition, SourceFile, SyntaxKind, SyntaxNode};
use rust_statement_spacing_core::{ByteRange, SourceModel};

use crate::lower::lower;
use crate::semantics::SemanticIndex;

/// Lossless syntax tree and its derived source-unit model.
pub struct ParsedSource {
    root: SyntaxNode,
    /// Spacing model; units remain inactive until compiler facts are attached.
    pub model: SourceModel,
}

/// Parses normalized Rust source for the requested edition.
///
/// # Errors
/// Rejects unsupported edition strings and source containing parser errors.
/// Recovery trees are never used as a basis for editing.
pub fn parse_source(source: &str, edition: &str) -> Result<ParsedSource, String> {
    let root = parse_root(source, edition)?;
    let model = lower(&root, source, None);
    return Ok(ParsedSource { root, model });
}

fn parse_root(source: &str, edition: &str) -> Result<SyntaxNode, String> {
    let edition = match edition {
        "2015" => Edition::Edition2015,
        "2018" => Edition::Edition2018,
        "2021" => Edition::Edition2021,
        "2024" => Edition::Edition2024,
        _ => return Err(format!("unsupported Rust edition: {edition}")),
    };
    let parse = SourceFile::parse(source, edition);
    if !parse.errors().is_empty() {
        return Err(format!(
            "source parser rejected this file: {:?}",
            parse.errors()
        ));
    }
    return Ok(parse.tree().syntax().clone());
}

impl ParsedSource {
    /// Syntax ranges suitable for synthetic *structural-only* test anchors.
    pub fn code_ranges(&self) -> Vec<ByteRange> {
        return self
            .model
            .lists
            .iter()
            .flat_map(|list| return list.units.iter().map(|unit| return unit.code_range))
            .collect();
    }

    /// Rebuilds the model using active compiler anchors and resolved facts.
    ///
    /// `source` must be the same normalized text passed to [`parse_source`].
    pub fn attach(&mut self, source: &str, semantics: &SemanticIndex) {
        self.model = lower(&self.root, source, Some(semantics));
    }
}

/// Returns token kinds and exact text, excluding only whitespace tokens.
/// Literal contents, comments, and macro token interiors are preserved.
///
/// # Errors
/// Rejects unsupported editions and syntax errors as in [`parse_source`].
pub fn token_fingerprint(source: &str, edition: &str) -> Result<Vec<(String, String)>, String> {
    let root = parse_root(source, edition)?;
    return Ok(root
        .descendants_with_tokens()
        .filter_map(|element| return element.into_token())
        .filter(|token| return token.kind() != SyntaxKind::WHITESPACE)
        .map(|token| return (format!("{:?}", token.kind()), token.text().to_string()))
        .collect());
}
