use std::path::Path;

use rust_statement_spacing_core::{ByteRange, text::SourceText};
use rustc_hir::HirId;
use rustc_span::{BytePos, Span};

pub struct DiagnosticAnchor {
    pub hir_id: HirId,
    pub span: Span,
}

pub fn normalized_range(start: BytePos, span: Span, length: usize) -> Option<ByteRange> {
    let lo = span.lo().0.checked_sub(start.0)? as usize;
    let hi = span.hi().0.checked_sub(start.0)? as usize;
    (lo <= hi && hi <= length).then_some(ByteRange::new(lo, hi))
}

pub fn source_span(anchor: Span, start: BytePos, range: ByteRange) -> Option<Span> {
    let lo = u32::try_from(range.start).ok()?.checked_add(start.0)?;
    let hi = u32::try_from(range.end).ok()?.checked_add(start.0)?;
    Some(anchor.with_lo(BytePos(lo)).with_hi(BytePos(hi)))
}

pub fn read_source(path: &Path, compiler_source: &str) -> Result<SourceText, String> {
    let raw = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    let text = SourceText::new(raw);
    if text.normalized != compiler_source {
        return Err("on-disk source differs from rustc's source snapshot".into());
    }
    if text.mixed_newlines {
        return Err("mixed newline styles: run the project's formatter first".into());
    }
    Ok(text)
}
