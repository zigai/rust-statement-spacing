use std::fmt::Write as _;

use crate::Result;

// A single hunk per file keeps generation linear and the patch deterministic.
pub(crate) fn unified(name: &str, before: &str, after: &str) -> Result<String> {
    if before == after {
        return Ok(String::new());
    }
    let old: Vec<_> = before.split_inclusive('\n').collect();
    let new: Vec<_> = after.split_inclusive('\n').collect();
    let prefix = old
        .iter()
        .zip(&new)
        .take_while(|(a, b)| return a == b)
        .count();
    let suffix = old
        .iter()
        .rev()
        .zip(new.iter().rev())
        .take(old.len().min(new.len()) - prefix)
        .take_while(|(a, b)| return a == b)
        .count();
    let start = prefix.saturating_sub(3);
    let old_end = old.len() - suffix.saturating_sub(3);
    let new_end = new.len() - suffix.saturating_sub(3);
    let old_count = old_end - start;
    let new_count = new_end - start;
    let mut output = format!(
        "--- {}\n+++ {}\n@@ -{},{} +{},{} @@\n",
        quote_path(&format!("a/{name}")),
        quote_path(&format!("b/{name}")),
        start + usize::from(old_count != 0),
        old_count,
        start + usize::from(new_count != 0),
        new_count
    );
    for line in old.iter().take(prefix).skip(start) {
        append_line(&mut output, ' ', line)?;
    }
    for line in old.iter().take(old.len() - suffix).skip(prefix) {
        append_line(&mut output, '-', line)?;
    }
    for line in new.iter().take(new.len() - suffix).skip(prefix) {
        append_line(&mut output, '+', line)?;
    }
    for line in old.iter().take(old_end).skip(old.len() - suffix) {
        append_line(&mut output, ' ', line)?;
    }
    return Ok(output);
}

fn append_line(output: &mut String, prefix: char, line: &str) -> Result<()> {
    write!(output, "{prefix}{line}")?;
    if !line.ends_with('\n') {
        output.push_str("\n\\ No newline at end of file\n");
    }
    return Ok(());
}

fn quote_path(path: &str) -> String {
    let mut result = String::from("\"");
    for character in path.chars() {
        match character {
            '\\' => result.push_str("\\\\"),
            '"' => result.push_str("\\\""),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            character => result.push(character),
        }
    }
    result.push('"');
    return result;
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::process::Command;

    use super::unified;
    use crate::Result;

    #[test]
    #[expect(
        clippy::panic_in_result_fn,
        reason = "Assertions define test failure; Result propagates fallible temporary-file and git setup"
    )]
    fn patches_handle_empty_files_crlf_and_missing_newlines() -> Result<()> {
        let directory = tempfile::tempdir()?;
        for (before, after) in [
            ("", "new\n"),
            ("old\n", ""),
            ("old", "new"),
            ("old\n", "old"),
            ("a\r\nb\r\n", "a\r\n\r\nb\r\n"),
            (
                "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\n",
                "a\nb\nc\nd\n\ne\nf\ng\nh\ni\nj\n",
            ),
        ] {
            let name = "source with spaces.rs";
            let source = directory.path().join(name);
            fs::write(&source, before)?;
            let patch = directory.path().join("preview.patch");
            fs::write(&patch, unified(name, before, after)?)?;
            let output = Command::new("git")
                .args(["apply", "--no-index", "--whitespace=nowarn"])
                .arg(&patch)
                .current_dir(directory.path())
                .output()?;
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(fs::read_to_string(source)?, after);
        }
        directory.close()?;
        return Ok(());
    }
}
