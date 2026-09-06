//! Explicit raw-file <-> compiler-normalized byte offsets, including CRLF and BOM.

#[derive(Clone, Debug)]
/// Raw source and normalized UTF-8 with a byte-offset translation table.
pub struct SourceText {
    /// Original file contents, including any BOM and CRLF newlines.
    pub raw: String,
    /// Source with an initial BOM removed and CRLF converted to LF.
    pub normalized: String,
    /// Whether all observed line endings are CRLF.
    pub crlf: bool,
    /// Whether the input mixes CRLF and bare LF line endings.
    pub mixed_newlines: bool,
    normalized_to_raw: Vec<usize>,
}

impl SourceText {
    /// Normalizes line endings and records the corresponding raw byte offsets.
    pub fn new(raw: String) -> Self {
        let bytes = raw.as_bytes();
        let mut i = if raw.starts_with('\u{feff}') { 3 } else { 0 };
        let mut map = Vec::with_capacity(bytes.len() + 1);
        let mut saw_crlf = false;
        let mut saw_lf = false;
        while i < bytes.len() {
            map.push(i);
            let byte = bytes.get(i).copied();
            if byte == Some(b'\r') && bytes.get(i + 1) == Some(&b'\n') {
                i += 2;
                saw_crlf = true;
            } else {
                saw_lf |= byte == Some(b'\n');
                i += 1;
            }
        }
        map.push(bytes.len());
        let normalized = raw
            .strip_prefix('\u{feff}')
            .unwrap_or(&raw)
            .replace("\r\n", "\n");
        return Self {
            raw,
            normalized,
            crlf: saw_crlf && !saw_lf,
            mixed_newlines: saw_crlf && saw_lf,
            normalized_to_raw: map,
        };
    }

    /// Maps a normalized byte offset to raw source, or returns `None` beyond EOF.
    pub fn raw_offset(&self, normalized: usize) -> Option<usize> {
        return self.normalized_to_raw.get(normalized).copied();
    }

    /// Maps a raw offset, returning `None` for removed bytes or offsets beyond EOF.
    pub fn normalized_offset(&self, raw: usize) -> Option<usize> {
        return self.normalized_to_raw.binary_search(&raw).ok();
    }

    /// Converts LF replacements to CRLF only when the input uses uniform CRLF.
    pub fn original_newlines(&self, replacement: &str) -> String {
        if self.crlf {
            return replacement.replace('\n', "\r\n");
        } else {
            return replacement.to_string();
        }
    }
}
