#![cfg_attr(windows, allow(unsafe_code))]
//! Native transactional Cargo subcommand for rustfmt-verified Dylint fixes.

mod cargo;
mod protocol;
mod report;
mod transaction;
mod workflow;
mod workspace;

use std::env;
use std::error::Error as StdError;
use std::io;
use std::path::PathBuf;
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
        help = "Path to the statement_spacing Dylint crate; otherwise use metadata"
    )]
    pub(crate) library_path: Option<String>,
    #[arg(
        long,
        help = "Canonical formatter toolchain; defaults to the project's active one"
    )]
    pub(crate) format_toolchain: Option<String>,
    #[arg(long, help = "Normalize inside the transaction first (fix only)")]
    pub(crate) format_first: bool,
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
    let invalid = if options.format_first && options.command != "fix" {
        Some("--format-first is valid only with fix")
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
