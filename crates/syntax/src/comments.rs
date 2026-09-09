use rust_statement_spacing_core::edits::is_trivia;
use rust_statement_spacing_core::{ByteRange, Gap};

/// Interpret only the trivia between two complete syntax nodes. Ambiguous block
/// comments and section comments are barriers, not material to rearrange.
pub(crate) fn classify_gap(source: &str, left: ByteRange, right: ByteRange) -> Gap {
    let range = ByteRange::new(left.end, right.start);
    let Some(text) = source.get(range.as_range()) else {
        return protected(range);
    };

    let mut start = range.start;
    let mut end = range.end;

    if text.contains("/*") || text.contains("*/") {
        return protected(range);
    }

    if text.contains("//") {
        let first_newline = text.find('\n');
        let first_comment = text.find("//");
        // A trailing line comment belongs to the preceding statement.
        if let Some(comment) = first_comment
            && first_newline.is_none_or(|newline| return comment < newline)
        {
            let Some(newline) = first_newline else {
                return protected(range);
            };
            start = range.start + newline;
        }

        let Some(remaining) = source.get(start..end) else {
            return protected(range);
        };

        if let Some(comment) = remaining.find("//") {
            let absolute_comment = start + comment;
            let comment_line_start = source
                .get(..absolute_comment)
                .and_then(|prefix| return prefix.rfind('\n'))
                .map_or(start, |newline| return newline + 1);

            // A leading comment is attached only when every remaining line is
            // a comment and there is no empty line before the following unit.
            let Some(suffix) = source.get(comment_line_start..end) else {
                return protected(range);
            };

            let mut lines = suffix.split('\n').peekable();
            while let Some(line) = lines.next() {
                if lines.peek().is_none() {
                    if !line.bytes().all(|b| matches!(b, b' ' | b'\t')) {
                        return protected(range);
                    }
                } else if !line.trim_start().starts_with("//") {
                    return protected(range);
                }
            }

            end = comment_line_start;
        }

        // Keep comments out of a mandatory joining pair, even when a leading
        // comment has a clear owner. Separation before that owner is still safe.
        let Some(old) = source.get(start..end) else {
            return protected(range);
        };
        if !old.bytes().all(is_trivia) {
            return protected(range);
        }
        return Gap {
            range: ByteRange::new(start, end),
            blank_lines: old.matches('\n').count().saturating_sub(1),
            vertical: old.contains('\n'),
            protected: false,
            joinable: false,
        };
    }

    if !text.bytes().all(is_trivia) {
        return protected(range);
    }
    return Gap {
        range,
        blank_lines: text.matches('\n').count().saturating_sub(1),
        vertical: text.contains('\n'),
        protected: false,
        joinable: true,
    };
}

fn protected(range: ByteRange) -> Gap {
    return Gap {
        range,
        blank_lines: 0,
        vertical: false,
        protected: true,
        joinable: false,
    };
}
