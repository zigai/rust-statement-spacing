#![cfg_attr(windows, allow(unsafe_code))]
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::error::Error as StdError;
use std::fmt::{self, Display, Formatter};
use std::fs;
use std::io;
#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::result::Result as StdResult;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

#[cfg(unix)]
use rustix::io::Errno;
#[cfg(unix)]
use rustix::process::{Pid, Signal, kill_process_group};
use serde_json::{Value, json};
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::runtime::{Builder, Runtime};
use tokio::task::JoinHandle;
use tokio::{signal, time};

use crate::protocol::{LintResult, MAX_JSON_BYTES, collect_edits};
use crate::workflow::Driver;
use crate::workspace::{canonical, relative_path};
use crate::{Result, VERSION, failure};
#[derive(Debug)]
pub(crate) struct Interrupted;

impl Display for Interrupted {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        return f.write_str("interrupted");
    }
}

impl StdError for Interrupted {}

static INTERRUPTED: AtomicBool = AtomicBool::new(false);
static EXECUTOR: LazyLock<StdResult<Executor, String>> = LazyLock::new(new_executor);

// The CLI owns this process-lifetime runtime and its single signal listener.
struct Executor {
    runtime: Runtime,
    _signals: JoinHandle<()>,
}

fn new_executor() -> StdResult<Executor, String> {
    let runtime = Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .map_err(|error| return error.to_string())?;
    let signals = {
        let _entered = runtime.enter();
        runtime.spawn(async {
            while signal::ctrl_c().await.is_ok() {
                INTERRUPTED.store(true, Ordering::Relaxed);
            }
        })
    };
    return Ok(Executor {
        runtime,
        _signals: signals,
    });
}

fn executor() -> Result<&'static Executor> {
    return EXECUTOR.as_ref().map_err(|error| {
        return failure(format!(
            "could not initialize process cancellation: {error}"
        ));
    });
}

pub(crate) fn setup_cancellation() -> Result<()> {
    return executor().map(|_| ());
}

fn interrupted() -> bool {
    return INTERRUPTED.load(Ordering::Relaxed);
}

pub(crate) fn check_interrupted() -> Result<()> {
    if interrupted() {
        return Err(Interrupted.into());
    } else {
        return Ok(());
    }
}

async fn cancellation() {
    return while !interrupted() {
        time::sleep(Duration::from_millis(10)).await;
    };
}

pub(crate) struct RunResult {
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    pub(crate) code: i32,
}

fn tail(text: &str) -> &str {
    let start = text
        .char_indices()
        .rev()
        .nth(7999)
        .map_or(0, |(index, _)| return index);
    return text.get(start..).unwrap_or("");
}

fn decoded(bytes: &[u8]) -> String {
    // Python's text-mode communicate uses universal newline conversion.
    return String::from_utf8_lossy(bytes)
        .replace("\r\n", "\n")
        .replace('\r', "\n");
}

#[cfg(unix)]
fn terminate_group(pid: Pid, signal: Signal) -> io::Result<()> {
    match kill_process_group(pid, signal) {
        Ok(()) | Err(Errno::SRCH) => return Ok(()),
        Err(error) => return Err(error.into()),
    }
}

#[cfg(windows)]
struct JobObject {
    handle: windows_sys::Win32::Foundation::HANDLE,
}

#[cfg(windows)]
impl JobObject {
    fn new() -> Result<Self> {
        // SAFETY: CreateJobObjectW with null attributes creates an anonymous job object.
        let handle = unsafe {
            windows_sys::Win32::System::JobObjects::CreateJobObjectW(
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        if handle.is_null() {
            return Err(failure(format!(
                "could not create Windows Job Object: {}",
                std::io::Error::last_os_error()
            )));
        }
        let mut info: windows_sys::Win32::System::JobObjects::JOBOBJECT_EXTENDED_LIMIT_INFORMATION =
            // SAFETY: zeroing memory for C-compatible struct.
            unsafe { std::mem::zeroed() };
        info.BasicLimitInformation.LimitFlags =
            windows_sys::Win32::System::JobObjects::JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        // SAFETY: SetInformationJobObject sets limits on the valid job handle.
        let success = unsafe {
            windows_sys::Win32::System::JobObjects::SetInformationJobObject(
                handle,
                windows_sys::Win32::System::JobObjects::JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                std::mem::size_of_val(&info) as u32,
            )
        };
        if success == 0 {
            // SAFETY: handle is a valid job object handle created above.
            unsafe { windows_sys::Win32::Foundation::CloseHandle(handle) };
            return Err(failure(format!(
                "could not configure Windows Job Object: {}",
                std::io::Error::last_os_error()
            )));
        }
        Ok(Self { handle })
    }

    fn assign(&self, process_handle: windows_sys::Win32::Foundation::HANDLE) -> Result<()> {
        // SAFETY: handle and process_handle are valid handles.
        let success = unsafe {
            windows_sys::Win32::System::JobObjects::AssignProcessToJobObject(
                self.handle,
                process_handle,
            )
        };
        if success == 0 {
            return Err(failure(format!(
                "could not assign child process to Windows Job Object: {}",
                std::io::Error::last_os_error()
            )));
        }
        Ok(())
    }

    fn terminate(&self) -> Result<()> {
        // SAFETY: handle is a valid job object handle.
        let success =
            unsafe { windows_sys::Win32::System::JobObjects::TerminateJobObject(self.handle, 1) };
        if success == 0 {
            return Err(failure(format!(
                "could not terminate Windows Job Object: {}",
                std::io::Error::last_os_error()
            )));
        }
        Ok(())
    }
}
#[cfg(unix)]
fn random_bytes(buffer: &mut [u8]) -> io::Result<()> {
    use std::io::Read as _;
    return fs::File::open("/dev/urandom")?.read_exact(buffer);
}

#[cfg(windows)]
fn random_bytes(buffer: &mut [u8]) -> io::Result<()> {
    #[link(name = "advapi32")]
    unsafe extern "system" {
        #[link_name = "SystemFunction036"]
        fn rtl_gen_random(buffer: *mut u8, len: u32) -> u8;
    }
    // SAFETY: buffer is a valid mutable slice of bytes of length buffer.len().
    let success = unsafe { rtl_gen_random(buffer.as_mut_ptr(), buffer.len() as u32) };
    if success == 0 {
        return Err(io::Error::last_os_error());
    }
    return Ok(());
}

#[cfg(windows)]
impl Drop for JobObject {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            // SAFETY: CloseHandle closes the job object handle, triggering JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE.
            unsafe { windows_sys::Win32::Foundation::CloseHandle(self.handle) };
        }
    }
}
fn is_lint_library_name(name: &str) -> bool {
    return name.contains("statement_spacing@")
        && (name.ends_with(".so") || name.ends_with(".dylib") || name.ends_with(".dll"));
}

fn find_prebuilt_library(path: &Path) -> Option<PathBuf> {
    if path.is_file() {
        if let Some(ext) = path.extension().and_then(|ext| return ext.to_str())
            && (ext == "so" || ext == "dylib" || ext == "dll")
        {
            return Some(path.to_path_buf());
        }
        return None;
    }
    let candidates = [path.join("target/release")];
    for dir in candidates {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let candidate = entry.path();
                if !candidate.is_file() {
                    continue;
                }
                if let Some(name) = candidate.file_name().and_then(|n| return n.to_str())
                    && is_lint_library_name(name)
                {
                    return Some(candidate);
                }
            }
        }
    }
    if let Ok(entries) = fs::read_dir(path.join("target/dylint/libraries")) {
        for toolchain_entry in entries.flatten() {
            let release_dir = toolchain_entry.path().join("release");
            if let Ok(lib_entries) = fs::read_dir(release_dir) {
                for lib_entry in lib_entries.flatten() {
                    let candidate = lib_entry.path();
                    if !candidate.is_file() {
                        continue;
                    }
                    if let Some(name) = candidate.file_name().and_then(|n| return n.to_str())
                        && is_lint_library_name(name)
                    {
                        return Some(candidate);
                    }
                }
            }
        }
    }
    return None;
}

impl Driver {
    pub(crate) fn run(
        &mut self,
        args: Vec<String>,
        cwd: &Path,
        env: Option<&BTreeMap<String, String>>,
        require_success: bool,
    ) -> Result<RunResult> {
        let executor = executor()?;
        check_interrupted()?;
        let (program, rest) = args
            .split_first()
            .ok_or_else(|| return failure("empty command"))?;
        let started = Instant::now();
        let result = executor.runtime.block_on(async {
            #[cfg(windows)]
            let job = JobObject::new()?;
            #[cfg(windows)]
            let mut command = if Path::new(program)
                .extension()
                .is_some_and(|ext| return ext.eq_ignore_ascii_case("py"))
            {
                let python_bin = env::var("PYTHON").unwrap_or_else(|_| return "python".to_string());
                let mut cmd = Command::new(python_bin);
                cmd.arg("-S");
                cmd.arg(program);
                cmd
            } else {
                Command::new(program)
            };
            #[cfg(not(windows))]
            let mut command = Command::new(program);
            command
                .args(rest)
                .current_dir(cwd)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true);
            #[cfg(unix)]
            command.process_group(0);
            if let Some(env) = env {
                command.envs(env);
            }
            let mut child = command
                .spawn()
                .map_err(|error| return failure(format!("could not execute {program}: {error}")))?;
            #[cfg(unix)]
            let pid = child
                .id()
                .and_then(|id| return i32::try_from(id).ok())
                .and_then(Pid::from_raw)
                .ok_or_else(|| return failure("spawned command has no process ID"))?;
            #[cfg(windows)]
            {
                let handle = child
                    .raw_handle()
                    .ok_or_else(|| failure("spawned command has no process handle"))?;
                job.assign(handle as windows_sys::Win32::Foundation::HANDLE)?;
            }
            let mut stdout = child
                .stdout
                .take()
                .ok_or_else(|| return failure("spawned command has no stdout pipe"))?;
            let mut stderr = child
                .stderr
                .take()
                .ok_or_else(|| return failure("spawned command has no stderr pipe"))?;
            let mut out = Vec::new();
            let mut err = Vec::new();
            let status = {
                let communication = async {
                    let (status, out_result, err_result) = tokio::join!(
                        child.wait(),
                        stdout.read_to_end(&mut out),
                        stderr.read_to_end(&mut err)
                    );
                    return Ok::<_, io::Error>((status?, out_result?, err_result?));
                };
                tokio::pin!(communication);
                let outcome = tokio::select! {
                    biased;
                    () = cancellation() => None,
                    result = &mut communication => Some(result),
                    () = time::sleep(Duration::from_secs(self.options.timeout)) => None,
                };
                if let Some(result) = outcome {
                    result
                        .map_err(|error| {
                            return failure(format!("could not execute {program}: {error}"));
                        })?
                        .0
                } else {
                    #[cfg(unix)]
                    {
                        let signal_error = terminate_group(pid, Signal::TERM).err();
                        if time::timeout(Duration::from_secs(3), &mut communication)
                            .await
                            .is_err()
                        {
                            let kill_error = terminate_group(pid, Signal::KILL).err();
                            // Reap the child and drain both pipes, including inherited descendant pipes.
                            communication.await.map_err(|error| {
                                return failure(format!(
                                    "could not reap terminated command: {error}"
                                ));
                            })?;
                            if let Some(error) = kill_error {
                                return Err(failure(format!(
                                    "could not terminate command: {error}"
                                )));
                            }
                        }
                        if interrupted() {
                            return Err(Interrupted.into());
                        }
                        if let Some(error) = signal_error {
                            return Err(failure(format!("could not terminate command: {error}")));
                        }
                        return Err(failure(format!(
                            "command timed out and was terminated: {}",
                            args.join(" ")
                        )));
                    }
                    #[cfg(windows)]
                    {
                        let term_result = job.terminate();
                        communication.await.map_err(|error| {
                            failure(format!("could not reap terminated command: {error}"))
                        })?;
                        if interrupted() {
                            return Err(Interrupted.into());
                        }
                        term_result?;
                        return Err(failure(format!(
                            "command timed out and was terminated: {}",
                            args.join(" ")
                        )));
                    }
                }
            };
            return Ok(RunResult {
                stdout: decoded(&out),
                stderr: decoded(&err),
                #[cfg(unix)]
                code: status
                    .code()
                    .unwrap_or_else(|| return -status.signal().unwrap_or(1)),
                #[cfg(windows)]
                code: status.code().unwrap_or(1),
            });
        })?;
        self.report
            .get_mut("commands")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| return failure("invalid command report"))?
            .push(json!({
                "argv": args, "cwd": cwd, "exit": result.code,
                "seconds": (started.elapsed().as_secs_f64() * 1000.0).round() / 1000.0,
                "stderr_tail": tail(&result.stderr),
            }));
        if require_success && result.code != 0 {
            return Err(failure(format!(
                "{} failed:\n{}\n{}",
                args.join(" "),
                tail(&result.stderr),
                tail(&result.stdout)
            )));
        }
        check_interrupted()?;
        return Ok(result);
    }

    pub(crate) fn formatter_cargo(&self) -> Vec<String> {
        return vec![
            self.options.cargo.clone(),
            format!("+{}", self.toolchain.as_deref().unwrap_or("")),
        ];
    }

    pub(crate) fn locate(&mut self) -> Result<PathBuf> {
        let cwd = env::current_dir()?;
        let mut manifest = cwd.join(&self.options.manifest_path);
        if !manifest.exists() {
            if self.options.manifest_path == "Cargo.toml"
                && let Some(parent) = cwd
                    .ancestors()
                    .skip(1)
                    .find(|parent| return parent.join("Cargo.toml").is_file())
            {
                manifest = parent.join("Cargo.toml");
            }
            if !manifest.exists() {
                return Err(failure(format!(
                    "Cargo manifest does not exist: {}",
                    manifest.display()
                )));
            }
        }
        manifest = canonical(&manifest)?;
        let parent = manifest
            .parent()
            .ok_or_else(|| return failure("Cargo manifest has no parent directory"))?;
        if self.toolchain.as_ref().is_none_or(String::is_empty) {
            let active = self.run(
                vec!["rustup".into(), "show".into(), "active-toolchain".into()],
                parent,
                None,
                true,
            )?;
            self.toolchain = active.stdout.split_whitespace().next().map(str::to_owned);
            if self.toolchain.is_none() {
                return Err(failure(
                    "could not identify the project's canonical Rust toolchain",
                ));
            }
        }
        let mut args = self.formatter_cargo();
        args.extend([
            "locate-project".into(),
            "--workspace".into(),
            "--message-format=json".into(),
            "--manifest-path".into(),
            manifest.to_string_lossy().into_owned(),
        ]);
        let result = self.run(args, parent, None, true)?;
        let value: Value = serde_json::from_str(&result.stdout)
            .map_err(|_| return failure("invalid cargo locate-project output"))?;
        let root_manifest = value
            .get("root")
            .and_then(Value::as_str)
            .ok_or_else(|| return failure("invalid cargo locate-project output"))?;
        let root = canonical(
            Path::new(root_manifest)
                .parent()
                .ok_or_else(|| return failure("invalid cargo locate-project output"))?,
        )?;
        if let Some(map) = self.report.as_object_mut() {
            map.insert("workspace".into(), json!(root));
        }
        return Ok(root);
    }

    pub(crate) fn identity(&mut self, root: &Path) -> Result<Value> {
        let mut args = self.formatter_cargo();
        args.push("--version".into());
        let cargo = self.run(args, root, None, true)?.stdout.trim().to_owned();
        let mut args = self.formatter_cargo();
        args.extend(["fmt".into(), "--version".into()]);
        let rustfmt = self.run(args, root, None, true)?.stdout.trim().to_owned();
        if cargo.is_empty() || rustfmt.is_empty() {
            return Err(failure("formatter identity could not be established"));
        }
        return Ok(json!({"toolchain": self.toolchain, "cargo": cargo, "rustfmt": rustfmt}));
    }

    pub(crate) fn fmt(&mut self, root: &Path, write: bool) -> Result<()> {
        let mut args = self.formatter_cargo();
        args.extend([
            "fmt".into(),
            "--all".into(),
            "--manifest-path".into(),
            root.join("Cargo.toml").to_string_lossy().into_owned(),
        ]);
        if !write {
            args.extend(["--".into(), "--check".into()]);
        }
        let result = self.run(args, root, None, false)?;
        if result.code != 0 {
            let message = if write {
                "initial formatting failed"
            } else {
                "cargo fmt rejected the candidate"
            };
            return Err(failure(format!(
                "{message}; nothing is silently reformatted:\n{}\n{}",
                tail(&result.stdout),
                tail(&result.stderr)
            )));
        }
        return Ok(());
    }

    pub(crate) fn lint(
        &mut self,
        root: &Path,
        temporary: &Path,
        label: &str,
        original_root: &Path,
    ) -> Result<LintResult> {
        let probes = temporary.join(format!("{label}-probes"));
        fs::create_dir_all(&probes)?;
        let target = temporary.join(format!("{label}-target"));
        let mut random = [0_u8; 24];
        random_bytes(&mut random)?;
        let mut nonce = String::with_capacity(48);
        const HEX: &[u8; 16] = b"0123456789abcdef";
        for byte in random {
            if let (Some(&hi), Some(&lo)) = (
                HEX.get(usize::from(byte >> 4)),
                HEX.get(usize::from(byte & 15)),
            ) {
                nonce.push(char::from(hi));
                nonce.push(char::from(lo));
            }
        }
        let mut env = BTreeMap::<String, String>::new();
        env.extend([
            (
                "STATEMENT_SPACING_WORKSPACE".into(),
                root.to_string_lossy().into_owned(),
            ),
            ("STATEMENT_SPACING_RUN_ID".into(), nonce.clone()),
            (
                "STATEMENT_SPACING_PROBE_DIR".into(),
                probes.to_string_lossy().into_owned(),
            ),
            (
                "CARGO_TARGET_DIR".into(),
                target.to_string_lossy().into_owned(),
            ),
            ("CARGO_TERM_COLOR".into(), "never".into()),
        ]);
        if self.options.offline {
            env.insert("CARGO_NET_OFFLINE".into(), "true".into());
        }
        let mut args = vec![self.options.cargo.clone(), "dylint".into()];
        if let Some(library) = &self.options.library_path {
            let library = canonical(Path::new(library))?;
            let library = library.strip_prefix(original_root).map_or_else(
                |_| return library.clone(),
                |relative| return root.join(relative),
            );
            if let Some(prebuilt) = find_prebuilt_library(&library) {
                args.extend(["--lib-path".into(), prebuilt.to_string_lossy().into_owned()]);
            } else {
                args.extend(["--path".into(), library.to_string_lossy().into_owned()]);
            }
        } else {
            args.push("--all".into());
        }
        args.extend([
            "--workspace".into(),
            "--".into(),
            "--all-targets".into(),
            "--locked".into(),
            "--message-format=json".into(),
            "--manifest-path".into(),
            root.join("Cargo.toml").to_string_lossy().into_owned(),
        ]);
        if self.options.all_features {
            args.push("--all-features".into());
        }
        if self.options.no_default_features {
            args.push("--no-default-features".into());
        }
        if !self.options.features.is_empty() {
            args.extend(["--features".into(), self.options.features.join(",")]);
        }
        if let Some(target) = &self.options.target {
            args.extend(["--target".into(), target.clone()]);
        }
        let result = self.run(args, root, Some(&env), false)?;
        let (findings, edits, errors) = collect_edits(root, &result.stdout, &self.package_roots)?;
        if !errors.is_empty() {
            return Err(failure(format!(
                "compilation produced non-statement_spacing errors: {}",
                errors.into_iter().take(5).collect::<Vec<_>>().join("; ")
            )));
        }
        let mut paths = fs::read_dir(&probes)?
            .map(|entry| return entry.map(|entry| return entry.path()))
            .collect::<io::Result<Vec<_>>>()?;
        paths.retain(|path| {
            return path
                .extension()
                .is_some_and(|extension| return extension == "json");
        });
        paths.sort();
        let mut handshakes = Vec::new();
        for path in paths {
            let metadata = fs::symlink_metadata(&path)?;
            if !metadata.is_file() || metadata.len() > MAX_JSON_BYTES {
                return Err(failure("invalid Dylint handshake file"));
            }
            let data: Value = serde_json::from_str(&fs::read_to_string(path)?)?;
            if data.get("schema").and_then(Value::as_i64) != Some(1)
                || data.get("library").and_then(Value::as_str) != Some("statement_spacing")
                || data.get("version").and_then(Value::as_str) != Some(VERSION)
                || data.get("run_id").and_then(Value::as_str) != Some(&nonce)
            {
                return Err(failure(
                    "Dylint handshake identity or run nonce did not match",
                ));
            }
            handshakes.push(data);
        }
        if handshakes.is_empty() {
            return Err(failure(format!(
                "statement_spacing did not report a fresh compiler run. Configure its Dylint metadata or pass --library-path; cached diagnostics are not accepted as verification.\n{}",
                tail(&result.stderr)
            )));
        }
        if result.code != 0 && findings.is_empty() {
            return Err(failure(format!(
                "Dylint failed without actionable statement_spacing diagnostics:\n{}",
                tail(&result.stderr)
            )));
        }
        let mut files = BTreeSet::new();
        let mut token_hashes = BTreeMap::new();
        let mut skipped_boundaries = 0_u64;
        for data in handshakes {
            let hashes = data
                .get("token_hashes")
                .and_then(Value::as_object)
                .ok_or_else(|| {
                    return failure("Dylint handshake lacks source token fingerprints");
                })?;
            if let Some(names) = data.get("files") {
                for name in names
                    .as_array()
                    .ok_or_else(|| return failure("invalid Dylint handshake files"))?
                {
                    let name = name
                        .as_str()
                        .ok_or_else(|| return failure("invalid Dylint handshake source path"))?;
                    let (_, relative) = relative_path(root, Path::new(name))?;
                    let fingerprint = hashes
                        .get(name)
                        .and_then(Value::as_str)
                        .filter(|value| {
                            return value.len() == 64
                                && value.bytes().all(|byte| {
                                    return byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte);
                                });
                        })
                        .ok_or_else(|| {
                            return failure(format!(
                                "invalid/missing source token fingerprint for {relative}"
                            ));
                        })?;
                    if token_hashes
                        .get(&relative)
                        .is_some_and(|previous| return previous != fingerprint)
                    {
                        return Err(failure(format!(
                            "target-specific token fingerprints disagree for {relative}"
                        )));
                    }
                    token_hashes.insert(relative.clone(), fingerprint.to_owned());
                    files.insert(relative);
                }
            }
            let skipped = match data.get("skipped_boundaries") {
                Some(value) => value
                    .as_u64()
                    .ok_or_else(|| return failure("invalid Dylint handshake skipped boundaries"))?,
                None => 0,
            };
            skipped_boundaries = skipped_boundaries
                .checked_add(skipped)
                .ok_or_else(|| return failure("invalid Dylint handshake skipped boundaries"))?;
        }
        if edits
            .iter()
            .any(|edit| return !token_hashes.contains_key(&edit.relative))
        {
            return Err(failure(
                "a suggested edit lacks a checked source token fingerprint",
            ));
        }
        return Ok(LintResult {
            findings,
            edits,
            checked_files: files.into_iter().collect(),
            skipped_boundaries,
            token_hashes,
        });
    }
}
