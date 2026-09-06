#![cfg_attr(windows, allow(unsafe_code))]
use std::collections::BTreeMap;
use std::fs::{self, DirBuilder, File};
use std::io::{self, Write};
#[cfg(unix)]
use std::os::fd::OwnedFd;
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
#[cfg(windows)]
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use rustix::fs::{FlockOperation, Mode, OFlags, flock, open, openat};
#[cfg(unix)]
use rustix::io::Errno;
use serde_json::{Value, json};

use crate::cargo::check_interrupted;
use crate::workspace::{
    FileState, Snapshot, assert_no_symlink, assert_snapshot, digest, read_regular, safe_relative,
};
use crate::{Result, failure};

#[cfg(unix)]
fn fsync_dir(directory: &Path) -> Result<()> {
    let descriptor = open(
        directory,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )?;
    File::from(descriptor).sync_all()?;
    return Ok(());
}

#[cfg(windows)]
fn fsync_dir(_directory: &Path) -> Result<()> {
    Ok(())
}

fn atomic_bytes(path: &Path, data: &[u8], mode: u32) -> Result<()> {
    let parent = path
        .parent()
        .filter(|parent| return !parent.as_os_str().is_empty())
        .unwrap_or_else(|| return Path::new("."));
    let mut temporary = tempfile::Builder::new()
        .prefix(".statement-spacing-tmp-")
        .tempfile_in(parent)?;
    #[cfg(unix)]
    temporary
        .as_file()
        .set_permissions(fs::Permissions::from_mode(mode))?;
    #[cfg(windows)]
    let _ = mode;
    temporary.write_all(data)?;
    temporary.flush()?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(path)
        .map_err(|error| return error.error)?;
    return fsync_dir(parent);
}

pub(crate) fn atomic_json(path: &Path, value: &Value) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    return atomic_bytes(path, &bytes, 0o600);
}

pub(crate) struct WorkspaceLock {
    #[cfg(unix)]
    _descriptor: OwnedFd,
    #[cfg(windows)]
    _file: File,
}

fn private_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    let build_result = DirBuilder::new().mode(0o700).create(path);
    #[cfg(windows)]
    let build_result = DirBuilder::new().create(path);
    match build_result {
        Ok(()) => return Ok(()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            let info = fs::symlink_metadata(path)?;
            if !info.is_dir() || info.file_type().is_symlink() {
                return Err(failure(format!(
                    "expected a non-symlinked directory: {}",
                    path.display()
                )));
            }
            return Ok(());
        }
        Err(error) => return Err(error.into()),
    }
}

#[cfg(unix)]
pub(crate) fn workspace_lock(root: &Path) -> Result<WorkspaceLock> {
    let state = root.join(".statement-spacing");
    if state.is_symlink() {
        return Err(failure(".statement-spacing must not be a symlink"));
    }
    private_directory(&state)?;
    let directory = open(
        &state,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )?;
    let descriptor = openat(
        &directory,
        "lock",
        OFlags::RDWR | OFlags::CREATE | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::from_raw_mode(0o600),
    )?;
    match flock(&descriptor, FlockOperation::NonBlockingLockExclusive) {
        Ok(()) => {
            return Ok(WorkspaceLock {
                _descriptor: descriptor,
            });
        }
        Err(error) if error == Errno::WOULDBLOCK => {
            return Err(failure(
                "another statement-spacing transaction is already running in this workspace",
            ));
        }
        Err(error) => return Err(error.into()),
    }
}

#[cfg(windows)]
pub(crate) fn workspace_lock(root: &Path) -> Result<WorkspaceLock> {
    let state = root.join(".statement-spacing");
    if state.is_symlink() {
        return Err(failure(".statement-spacing must not be a symlink"));
    }
    private_directory(&state)?;
    let lock_path = state.join("lock");
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)?;
    let handle = file.as_raw_handle();
    // SAFETY: OVERLAPPED is initialized with all zero bytes.
    let mut overlapped: windows_sys::Win32::System::IO::OVERLAPPED = unsafe { std::mem::zeroed() };
    let flags = windows_sys::Win32::Storage::FileSystem::LOCKFILE_EXCLUSIVE_LOCK
        | windows_sys::Win32::Storage::FileSystem::LOCKFILE_FAIL_IMMEDIATELY;
    // SAFETY: `handle` is a valid handle from an open `file`, and `overlapped` points to zeroed memory.
    let success = unsafe {
        windows_sys::Win32::Storage::FileSystem::LockFileEx(
            handle as windows_sys::Win32::Foundation::HANDLE,
            flags,
            0,
            1,
            0,
            &mut overlapped,
        )
    };
    if success == 0 {
        // SAFETY: GetLastError has no preconditions and queries the calling thread's last error.
        let error_code = unsafe { windows_sys::Win32::Foundation::GetLastError() };
        if error_code == windows_sys::Win32::Foundation::ERROR_LOCK_VIOLATION {
            return Err(failure(
                "another statement-spacing transaction is already running in this workspace",
            ));
        }
        return Err(std::io::Error::from_raw_os_error(error_code as i32).into());
    }
    Ok(WorkspaceLock { _file: file })
}

#[cfg(windows)]
fn number_of_links(path: &Path) -> Result<u32> {
    let file = File::open(path)?;
    let handle = file.as_raw_handle() as windows_sys::Win32::Foundation::HANDLE;
    let mut info: windows_sys::Win32::Storage::FileSystem::BY_HANDLE_FILE_INFORMATION =
        // SAFETY: zeroing memory for C-compatible struct.
        unsafe { std::mem::zeroed() };
    // SAFETY: `handle` is a valid open file handle, and `info` points to valid memory.
    let success = unsafe {
        windows_sys::Win32::Storage::FileSystem::GetFileInformationByHandle(handle, &mut info)
    };
    if success == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(info.nNumberOfLinks)
}

fn journal_text<'entry>(entry: &'entry Value, key: &str) -> Result<&'entry str> {
    return entry
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| return failure("unsupported or malformed recovery journal"));
}

fn read_journal(directory: &Path) -> Result<Value> {
    let path = directory.join("journal.json");
    if directory.is_symlink() || path.is_symlink() {
        return Err(failure("refusing a symlinked recovery journal"));
    }
    return Ok(serde_json::from_slice(&read_regular(&path)?)?);
}

fn rollback(root: &Path, journal_dir: &Path) -> Result<Vec<String>> {
    let journal = read_journal(journal_dir)?;
    if journal.get("schema").and_then(Value::as_u64) != Some(1)
        || !journal.get("files").is_some_and(Value::is_array)
    {
        return Err(failure("unsupported or malformed recovery journal"));
    }
    let entries = journal
        .get("files")
        .and_then(Value::as_array)
        .ok_or_else(|| return failure("unsupported or malformed recovery journal"))?;
    let mut conflicts = Vec::new();
    for entry in entries {
        let name = journal_text(entry, "path")?;
        let relative = safe_relative(name)?;
        let destination = root.join(&relative);
        assert_no_symlink(root, &destination)?;
        let backup = journal_dir.join(safe_relative(journal_text(entry, "backup")?)?);
        if backup.is_symlink() {
            return Err(failure("refusing a symlinked transaction backup"));
        }
        assert_no_symlink(journal_dir, &backup)?;
        let data = read_regular(&backup)?;
        let before = journal_text(entry, "before")?;
        let after = journal_text(entry, "after")?;
        let mode = entry
            .get("mode")
            .and_then(Value::as_u64)
            .and_then(|mode| return u32::try_from(mode).ok())
            .filter(|mode| return *mode <= 0o7777)
            .ok_or_else(|| return failure("unsupported or malformed recovery journal"))?;
        if digest(&data) != before {
            return Err(failure(format!(
                "corrupt recovery backup for {}",
                relative.display()
            )));
        }
        let current = if destination.is_file() {
            Some(digest(&read_regular(&destination)?))
        } else {
            None
        };
        if current.as_deref() == Some(before) {
            continue;
        }
        if current.as_deref() != Some(after) {
            conflicts.push(name.to_owned());
            continue;
        }
        atomic_bytes(&destination, &data, mode)?;
    }
    if conflicts.is_empty() {
        fs::remove_dir_all(journal_dir)?;
        fsync_dir(
            journal_dir
                .parent()
                .ok_or_else(|| return failure("journal has no parent directory"))?,
        )?;
    }
    return Ok(conflicts);
}

pub(crate) fn recover(root: &Path) -> Result<Vec<String>> {
    let transactions = root.join(".statement-spacing/transactions");
    if transactions.is_symlink() {
        return Err(failure("transaction directory must not be a symlink"));
    }
    if !transactions.try_exists()? {
        return Ok(Vec::new());
    }
    assert_no_symlink(root, &transactions)?;
    let mut directories = fs::read_dir(&transactions)?
        .map(|entry| return entry.map(|entry| return entry.path()))
        .collect::<io::Result<Vec<_>>>()?;
    directories.sort();
    let mut restored = Vec::new();
    for directory in directories {
        if directory.is_symlink() || !directory.is_dir() {
            return Err(failure(format!(
                "unexpected recovery entry: {}",
                directory.display()
            )));
        }
        let journal_path = directory.join("journal.json");
        if !journal_path.try_exists()? {
            return Err(failure(format!(
                "incomplete preparation at {}; inspect it before removing it",
                directory.display()
            )));
        }
        let journal = read_journal(&directory)?;
        if journal.get("phase").and_then(Value::as_str) == Some("complete") {
            fs::remove_dir_all(&directory)?;
            fsync_dir(&transactions)?;
            continue;
        }
        let conflicts = rollback(root, &directory)?;
        if !conflicts.is_empty() {
            return Err(failure(format!(
                "recovery preserved newer edits in {}; original backups remain in {}",
                conflicts.join(", "),
                directory.display()
            )));
        }
        restored.push(
            directory
                .file_name()
                .and_then(|name| return name.to_str())
                .ok_or_else(|| return failure("recovery directory name is not UTF-8"))?
                .to_owned(),
        );
    }
    return Ok(restored);
}

pub(crate) fn commit(
    root: &Path,
    original: &Snapshot,
    changed: &BTreeMap<String, Vec<u8>>,
    final_check: impl FnOnce() -> Result<()>,
    limit: u64,
) -> Result<()> {
    check_interrupted()?;
    assert_snapshot(root, original, limit)?;
    for name in changed.keys() {
        if !original.contains_key(name) || !name.ends_with(".rs") {
            return Err(failure(format!(
                "verified mode only commits existing Rust source files: {name}"
            )));
        }
        let path = root.join(safe_relative(name)?);
        assert_no_symlink(root, &path)?;
        #[cfg(unix)]
        let multiply_linked = fs::metadata(&path)?.nlink() != 1;
        #[cfg(windows)]
        let multiply_linked = number_of_links(&path).unwrap_or(1) != 1;
        if multiply_linked {
            return Err(failure(format!(
                "refusing to replace a multiply hard-linked source file: {name}"
            )));
        }
    }
    let transactions = root.join(".statement-spacing/transactions");
    if transactions.is_symlink() {
        return Err(failure("transaction directory must not be a symlink"));
    }
    assert_no_symlink(root, &transactions)?;
    private_directory(&transactions)?;
    if fs::read_dir(&transactions)?.next().transpose()?.is_some() {
        return Err(failure(
            "an unfinished transaction exists; run cargo statement-spacing recover first",
        ));
    }
    let journal_dir: PathBuf = tempfile::Builder::new()
        .prefix("")
        .rand_bytes(12)
        .tempdir_in(&transactions)?
        .keep();
    let mut entries = Vec::with_capacity(changed.len());
    // Persist every backup and the prepared journal before replacing any source.
    let preparation = (|| -> Result<Value> {
        for (index, (name, data)) in changed.iter().enumerate() {
            check_interrupted()?;
            let source = root.join(name);
            assert_no_symlink(root, &source)?;
            let before = read_regular(&source)?;
            let before_hash = digest(&before);
            if before_hash != original[name].digest {
                return Err(failure(format!("concurrent source modification: {name}")));
            }
            let backup_name = format!("{index:06}.original");
            atomic_bytes(&journal_dir.join(&backup_name), &before, 0o600)?;
            entries.push(json!({
                "path": name, "backup": backup_name, "before": before_hash,
                "after": digest(data), "mode": original[name].mode,
            }));
        }
        let journal = json!({"schema": 1, "phase": "prepared", "files": entries});
        atomic_json(&journal_dir.join("journal.json"), &journal)?;
        fsync_dir(&transactions)?;
        fsync_dir(
            transactions
                .parent()
                .ok_or_else(|| return failure("transaction directory has no parent"))?,
        )?;
        return Ok(journal);
    })();
    let mut journal = match preparation {
        Ok(journal) => journal,
        Err(error) => {
            fs::remove_dir_all(&journal_dir)?;
            fsync_dir(&transactions)?;
            return Err(error);
        }
    };
    let result = (|| -> Result<()> {
        for entry in &entries {
            check_interrupted()?;
            let name = journal_text(entry, "path")?;
            let path = root.join(name);
            assert_no_symlink(root, &path)?;
            if digest(&read_regular(&path)?) != journal_text(entry, "before")? {
                return Err(failure(format!("source changed at commit time: {name}")));
            }
            atomic_bytes(&path, &changed[name], original[name].mode)?;
        }
        check_interrupted()?;
        final_check()?;
        let mut expected = original.clone();
        for (name, data) in changed {
            expected.insert(
                name.clone(),
                FileState {
                    digest: digest(data),
                    mode: original[name].mode,
                    size: data.len() as u64,
                },
            );
        }
        assert_snapshot(root, &expected, limit)?;
        check_interrupted()?;
        if let Some(map) = journal.as_object_mut() {
            map.insert("phase".into(), Value::from("complete"));
        }
        atomic_json(&journal_dir.join("journal.json"), &journal)?;
        return Ok(());
    })();
    if let Err(error) = result {
        match rollback(root, &journal_dir) {
            Err(rollback_error) => {
                return Err(failure(format!(
                    "commit failed ({error}); recovery also failed ({rollback_error}); durable backups remain at {}",
                    journal_dir.display()
                )));
            }
            Ok(conflicts) if !conflicts.is_empty() => {
                return Err(failure(format!(
                    "commit failed ({error}); newer editor changes were preserved in {conflicts:?}; recover original content from {}",
                    journal_dir.display()
                )));
            }
            Ok(_) => return Err(error),
        }
    }
    fs::remove_dir_all(&journal_dir)?;
    return fsync_dir(&transactions);
}
