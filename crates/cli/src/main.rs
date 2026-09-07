#![cfg_attr(windows, allow(unsafe_code))]
//! Native transactional Cargo subcommand for rustfmt-verified Dylint fixes.

mod baseline;
mod cargo;
mod diff;
mod protocol;
mod report;
mod transaction;
mod workflow;
mod workspace;

use std::env;
use std::error::Error as StdError;
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::result::Result as StdResult;

use clap::Parser;
use clap::error::ErrorKind;

pub(crate) const VERSION: &str = env!("CARGO_PKG_VERSION");
pub(crate) type Error = Box<dyn StdError + Send + Sync>;
pub(crate) type Result<T, E = Error> = StdResult<T, E>;

pub(crate) fn failure(message: impl Into<String>) -> Error {
    return io::Error::other(message.into()).into();
}

#[derive(Parser)]
#[command(name = "statement-spacing", bin_name = "cargo statement-spacing", version = VERSION,
    about = "Dylint blank-line checks and transactional, rustfmt-verified fixes.")]
#[expect(
    clippy::struct_excessive_bools,
    reason = "These independent CLI switches control formatting, Cargo features, network access, and report output."
)]
pub(crate) struct Options {
    #[arg(value_parser = ["check", "fix", "recover"])]
    pub(crate) command: String,
    #[arg(long, default_value = "Cargo.toml")]
    pub(crate) manifest_path: String,
    #[arg(
        long,
        help = "Path to the statement_spacing Dylint crate or explicit shared library; otherwise use metadata"
    )]
    pub(crate) library_path: Option<String>,
    #[arg(
        long,
        help = "Canonical formatter toolchain; defaults to the project's active one"
    )]
    pub(crate) format_toolchain: Option<String>,
    #[arg(long, help = "Normalize inside the transaction first (fix only)")]
    pub(crate) format_first: bool,
    #[arg(
        long,
        help = "Verify proposed fixes without writing source files (fix only)"
    )]
    pub(crate) dry_run: bool,
    #[arg(
        long,
        requires = "dry_run",
        help = "Include a unified diff of verified proposed fixes"
    )]
    pub(crate) diff: bool,
    #[arg(
        long,
        conflicts_with = "write_baseline",
        help = "Report only findings absent from this baseline (check only)"
    )]
    pub(crate) baseline: Option<PathBuf>,
    #[arg(
        long,
        help = "Save current findings as a baseline .json file (check only)"
    )]
    pub(crate) write_baseline: Option<PathBuf>,
    #[arg(long, action = clap::ArgAction::Append)]
    pub(crate) features: Vec<String>,
    #[arg(long, conflicts_with = "features")]
    pub(crate) all_features: bool,
    #[arg(long)]
    pub(crate) no_default_features: bool,
    #[arg(
        long,
        help = "Cargo compilation target; formatter still checks the entire workspace"
    )]
    pub(crate) target: Option<String>,
    #[arg(long)]
    pub(crate) offline: bool,
    #[arg(long, help = "Write the verification report to a .json file")]
    pub(crate) report: Option<PathBuf>,
    #[arg(long, help = "Emit the report as JSON on stdout")]
    pub(crate) json: bool,
    #[arg(
        long,
        env = "STATEMENT_SPACING_CARGO",
        default_value = "cargo",
        hide = true
    )]
    pub(crate) cargo: String,
    #[arg(long, default_value_t = 900, value_parser = clap::value_parser!(u64).range(1..), help = "Per-command timeout in seconds")]
    pub(crate) timeout: u64,
    #[arg(long, default_value_t = 512, value_parser = clap::value_parser!(u64).range(1..=u64::MAX / (1024 * 1024)))]
    pub(crate) max_snapshot_mib: u64,
    #[arg(long, action = clap::ArgAction::Append, help = "Omit a workspace-relative file or directory from verification snapshots (repeatable; no globs)")]
    pub(crate) snapshot_exclude: Vec<String>,
}

fn main() -> ExitCode {
    let mut arguments: Vec<_> = env::args_os().collect();
    if arguments
        .get(1)
        .is_some_and(|argument| return argument == "statement-spacing")
    {
        arguments.remove(1);
    }
    let options = Options::parse_from(arguments);
    let invalid = if (options.format_first || options.dry_run) && options.command != "fix" {
        Some("--format-first and --dry-run are valid only with fix")
    } else if (options.baseline.is_some() || options.write_baseline.is_some())
        && options.command != "check"
    {
        Some("baseline options are valid only with check")
    } else if options
        .baseline
        .iter()
        .chain(options.write_baseline.iter())
        .any(|path| {
            return path
                .extension()
                .is_none_or(|extension| return extension != "json");
        })
    {
        Some("baseline paths must name .json files")
    } else if options.report.as_ref().is_some_and(|report| {
        return options
            .baseline
            .iter()
            .chain(options.write_baseline.iter())
            .any(|baseline| return same_destination(report, baseline));
    }) {
        Some("report and baseline paths must differ")
    } else if options.report.as_ref().is_some_and(|path| {
        return path
            .extension()
            .is_none_or(|extension| return extension != "json");
    }) {
        Some("--report must name a .json file")
    } else {
        None
    };
    if let Some(message) = invalid {
        use clap::CommandFactory;
        Options::command()
            .error(ErrorKind::ArgumentConflict, message)
            .exit();
    }
    return execute(options);
}

fn same_destination(left: &Path, right: &Path) -> bool {
    let resolve = |path: &Path| {
        return path.canonicalize().or_else(|_| {
            let parent = path
                .parent()
                .filter(|parent| return !parent.as_os_str().is_empty())
                .unwrap_or_else(|| return Path::new("."));
            return parent.canonicalize().map(|parent| {
                return path
                    .file_name()
                    .map_or_else(|| return parent.clone(), |name| return parent.join(name));
            });
        });
    };
    return left == right
        || matches!((resolve(left), resolve(right)), (Ok(left), Ok(right)) if left == right);
}

fn execute(options: Options) -> ExitCode {
    let mut driver = workflow::Driver::new(options);
    let code = match driver.execute() {
        Ok(code) => code,
        Err(error) => {
            let (status, error_message, exit_code) = if error.is::<cargo::Interrupted>() {
                (
                    "interrupted",
                    "interrupted; inspect recover if the process was killed".to_string(),
                    130,
                )
            } else {
                ("error", error.to_string(), 2)
            };
            if let Some(map) = driver.report.as_object_mut() {
                map.insert("status".into(), status.into());
                map.insert("error".into(), error_message.into());
            }
            exit_code
        }
    };
    return ExitCode::from(report::emit(
        &driver.options,
        &driver.report,
        driver.toolchain.as_deref(),
        code,
    ));
}
