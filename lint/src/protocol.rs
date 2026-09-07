use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Serialize;

#[derive(Serialize)]
struct Probe<'a> {
    schema: u32,
    library: &'static str,
    version: &'static str,
    compiler: &'static str,
    run_id: &'a str,
    files: &'a [String],
    skipped_boundaries: usize,
    token_hashes: &'a BTreeMap<String, String>,
}

pub(crate) fn emit(
    files: &[String],
    token_hashes: &BTreeMap<String, String>,
    skipped_boundaries: usize,
) -> std::io::Result<()> {
    let Some(directory) = std::env::var_os("STATEMENT_SPACING_PROBE_DIR") else {
        return Ok(());
    };
    let Some(run_id) = std::env::var_os("STATEMENT_SPACING_RUN_ID") else {
        return Ok(());
    };
    let path = PathBuf::from(directory).join(format!("{}.json", std::process::id()));
    let run_id = run_id.to_string_lossy();
    let data = Probe {
        schema: 1,
        library: "statement_spacing",
        version: env!("CARGO_PKG_VERSION"),
        compiler: include_str!(concat!(env!("OUT_DIR"), "/compiler.txt")),
        run_id: &run_id,
        files,
        skipped_boundaries,
        token_hashes,
    };
    let temporary = path.with_extension("tmp");
    std::fs::write(
        &temporary,
        serde_json::to_vec(&data).map_err(std::io::Error::other)?,
    )?;
    std::fs::rename(temporary, path)
}
