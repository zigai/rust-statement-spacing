use std::fs;
use std::io::{self, Write as _};
use std::path::Path;

use serde_json::Value;

use crate::transaction::atomic_json;
use crate::{Options, Result};

pub(crate) fn emit(options: &Options, report: &Value, toolchain: Option<&str>, mut code: u8) -> u8 {
    if let Some(path) = &options.report {
        let result = (|| -> Result<()> {
            if let Some(parent) = path
                .parent()
                .filter(|parent| return !parent.as_os_str().is_empty())
            {
                fs::create_dir_all(parent)?;
            }
            return write_report(path, report);
        })();
        if let Err(error) = result {
            let _ = writeln!(io::stderr(), "could not write report: {error}");
            code = 2;
        }
    }
    if options.json {
        match serde_json::to_string_pretty(report) {
            Ok(report) => println!("{report}"),
            Err(error) => {
                let _ = writeln!(io::stderr(), "could not serialize report: {error}");
                return 2;
            }
        }
    } else {
        let status = report["status"].as_str().unwrap_or("error");
        println!("statement-spacing: {status}");
        if let Some(findings) = report["findings"].as_array() {
            for finding in findings.iter().take(50) {
                println!(
                    "  {}:{}: {}: {}",
                    text(&finding["file"]),
                    text(&finding["line"]),
                    text(&finding["rule"]),
                    text(&finding["message"])
                );
            }
        }
        if let Some(error) = report["error"].as_str() {
            let _ = writeln!(io::stderr(), "{error}");
        }
        if matches!(status, "passed" | "fixed") {
            let count = report["changed_files"].as_array().map_or(0, Vec::len);
            println!(
                "  {count} file(s) changed; formatter: {}",
                toolchain.unwrap_or("None")
            );
            if let Some(skipped) = report["skipped_boundaries"]
                .as_u64()
                .filter(|count| return *count != 0)
            {
                println!("  {skipped} protected/inactive boundaries preserved");
            }
        }
    }
    return code;
}

#[cfg(unix)]
fn write_report(path: &Path, report: &Value) -> Result<()> {
    return atomic_json(path, report);
}

#[cfg(not(unix))]
fn write_report(path: &Path, report: &Value) -> Result<()> {
    use std::io::Write;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temporary = tempfile::Builder::new()
        .prefix(".statement-spacing-tmp-")
        .tempfile_in(parent)?;
    serde_json::to_writer_pretty(&mut temporary, report)?;
    temporary.write_all(b"\n")?;
    temporary.flush()?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}

fn text(value: &Value) -> String {
    match value {
        Value::Null => return "None".into(),
        Value::String(value) => return value.clone(),
        value => return value.to_string(),
    }
}
