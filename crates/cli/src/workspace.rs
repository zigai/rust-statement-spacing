#[cfg(unix)]
use rustix::fs::{Mode, OFlags, open};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs::{self, File};
use std::io;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};
use std::str;

use crate::{Result, failure};

const IGNORED_DIRS: [&str; 4] = [".git", "target", ".statement-spacing", "__pycache__"];

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FileState {
    pub(crate) digest: String,
    pub(crate) mode: u32,
    pub(crate) size: u64,
}

pub(crate) type Snapshot = BTreeMap<String, FileState>;

pub(crate) fn digest(bytes: &[u8]) -> String {
    return format!("{:x}", Sha256::digest(bytes));
}

pub(crate) fn canonical(path: &Path) -> Result<PathBuf> {
    let resolved = fs::canonicalize(path)
        .map_err(|error| return failure(format!("cannot resolve {}: {error}", path.display())))?;
    #[cfg(windows)]
    {
        let path_str = resolved.to_str().unwrap_or("");
        if let Some(stripped) = path_str.strip_prefix(r"\\?\UNC\") {
            return Ok(PathBuf::from(format!(r"\\{stripped}")));
        } else if let Some(stripped) = path_str.strip_prefix(r"\\?\") {
            return Ok(PathBuf::from(stripped));
        }
    }
    return Ok(resolved);
}

#[cfg(unix)]
fn file_mode(info: &fs::Metadata) -> u32 {
    return info.permissions().mode() & 0o7777;
}

#[cfg(windows)]
fn file_mode(_info: &fs::Metadata) -> u32 {
    0o644
}

pub(crate) fn relative_path(root: &Path, name: &Path) -> Result<(PathBuf, String)> {
    let path = canonical(&root.join(name))?;
    let relative = path.strip_prefix(root).map_err(|_| {
        return failure(format!(
            "source path is outside the supported workspace: {}",
            path.display()
        ));
    })?;
    let relative = relative
        .to_str()
        .ok_or_else(|| return failure("workspace path is not UTF-8"))?
        .to_owned();
    return Ok((path, relative));
}

pub(crate) fn safe_relative(name: &str) -> Result<PathBuf> {
    let path: PathBuf = Path::new(name)
        .components()
        .filter(|part| return !matches!(part, Component::CurDir))
        .collect();
    if name.is_empty()
        || path.components().next().is_none()
        || path
            .components()
            .any(|part| return !matches!(part, Component::Normal(_)))
    {
        return Err(failure(format!("unsafe relative path: {name:?}")));
    }
    return Ok(path);
}

pub(crate) fn assert_no_symlink(root: &Path, path: &Path) -> Result<()> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| return failure(format!("path escaped workspace: {}", path.display())))?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        if !matches!(component, Component::Normal(_)) {
            return Err(failure(format!(
                "path escaped workspace: {}",
                path.display()
            )));
        }
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(info) if info.file_type().is_symlink() => {
                return Err(failure(format!(
                    "symlinks are not supported in verified workspaces: {}",
                    current.display()
                )));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    return Ok(());
}

#[cfg(unix)]
pub(crate) fn read_regular(path: &Path) -> Result<Vec<u8>> {
    let descriptor = open(
        path,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )?;
    let mut file = File::from(descriptor);
    if !file.metadata()?.is_file() {
        return Err(failure(format!(
            "non-regular workspace file is not supported: {}",
            path.display()
        )));
    }
    let mut data = Vec::new();
    io::copy(&mut file, &mut data)?;
    return Ok(data);
}

#[cfg(windows)]
pub(crate) fn read_regular(path: &Path) -> Result<Vec<u8>> {
    let info = fs::symlink_metadata(path)?;
    if info.file_type().is_symlink() {
        return Err(failure(format!(
            "symlinks are not supported in verified workspaces: {}",
            path.display()
        )));
    }
    if !info.is_file() {
        return Err(failure(format!(
            "non-regular workspace file is not supported: {}",
            path.display()
        )));
    }
    let mut file = File::open(path)?;
    let mut data = Vec::new();
    file.read_to_end(&mut data)?;
    Ok(data)
}

pub(crate) fn scan(root: &Path, limit: u64) -> Result<Snapshot> {
    let mut result = BTreeMap::new();
    let mut total = 0_u64;
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let mut entries = fs::read_dir(directory)?.collect::<io::Result<Vec<_>>>()?;
        entries.sort_by_key(fs::DirEntry::file_name);
        for entry in entries {
            let name = entry.file_name();
            if IGNORED_DIRS.iter().any(|ignored| return name == *ignored) {
                continue;
            }
            let path = entry.path();
            let info = fs::symlink_metadata(&path)?;
            if info.is_dir() {
                pending.push(path);
                continue;
            }
            if info.file_type().is_symlink() && path.is_dir() {
                return Err(failure(format!(
                    "symlinked directory is not supported: {}",
                    path.display()
                )));
            }
            if name
                .as_encoded_bytes()
                .starts_with(b".statement-spacing-tmp-")
            {
                continue;
            }
            if !info.is_file() {
                return Err(failure(format!(
                    "non-regular workspace file is not supported: {}",
                    path.display()
                )));
            }
            total = total
                .checked_add(info.len())
                .filter(|size| return *size <= limit)
                .ok_or_else(|| {
                    return failure(
                        "workspace snapshot exceeds --max-snapshot-mib; nothing was changed",
                    );
                })?;
            let data = read_regular(&path)?;
            if data.len() as u64 != info.len() {
                return Err(failure(format!(
                    "file changed during snapshot: {}",
                    path.display()
                )));
            }
            let relative = path.strip_prefix(root)?;
            let name = relative
                .to_str()
                .ok_or_else(|| return failure("workspace path is not UTF-8"))?
                .to_owned();
            result.insert(
                name,
                FileState {
                    digest: digest(&data),
                    mode: file_mode(&info),
                    size: data.len() as u64,
                },
            );
        }
    }
    return Ok(result);
}

pub(crate) fn assert_snapshot(root: &Path, expected: &Snapshot, limit: u64) -> Result<()> {
    let actual = scan(root, limit)?;
    if actual == *expected {
        return Ok(());
    }
    let actual_names: BTreeSet<_> = actual.keys().collect();
    let expected_names: BTreeSet<_> = expected.keys().collect();
    let changed = actual_names
        .symmetric_difference(&expected_names)
        .copied()
        .chain(
            actual_names
                .intersection(&expected_names)
                .copied()
                .filter(|name| return actual.get(*name) != expected.get(*name)),
        )
        .take(10)
        .map(String::as_str)
        .collect::<Vec<_>>();
    return Err(failure(format!(
        "workspace changed during validation: {}",
        changed.join(", ")
    )));
}

pub(crate) fn copy_snapshot(root: &Path, replica: &Path, files: &Snapshot) -> Result<()> {
    if replica.try_exists()? {
        return Err(failure(format!(
            "replica already exists: {}",
            replica.display()
        )));
    }
    fs::create_dir_all(replica)?;
    for (name, state) in files {
        let source = root.join(name);
        assert_no_symlink(root, &source)?;
        let data = read_regular(&source)?;
        if digest(&data) != state.digest {
            return Err(failure(format!("source changed while copying: {name}")));
        }
        let destination = replica.join(name);
        let parent = destination
            .parent()
            .ok_or_else(|| return failure("replica file has no parent"))?;
        fs::create_dir_all(parent)?;
        fs::write(&destination, data)?;
        #[cfg(unix)]
        fs::set_permissions(destination, fs::Permissions::from_mode(state.mode))?;
    }
    return Ok(());
}

pub(crate) fn reject_ancestor_config(root: &Path) -> Result<()> {
    for parent in root.ancestors().skip(1) {
        for name in [
            "rustfmt.toml",
            ".rustfmt.toml",
            ".cargo/config",
            ".cargo/config.toml",
        ] {
            let candidate = parent.join(name);
            if candidate.exists() {
                return Err(failure(format!(
                    "inherited configuration outside workspace is not relocatable: {}. Put this configuration in the workspace before using verified mode.",
                    candidate.display()
                )));
            }
        }
    }
    if let Some(rustfmt) = env::var_os("RUSTFMT")
        && !rustfmt.is_empty()
        && !Path::new(&rustfmt).is_absolute()
    {
        return Err(failure("RUSTFMT must be an absolute path in verified mode"));
    }
    return Ok(());
}

pub(crate) fn validate_metadata(root: &Path, metadata: &Value) -> Result<()> {
    let text = |value: &Value| {
        return value
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| return failure("malformed Cargo metadata"));
    };
    if canonical(Path::new(&text(&metadata["workspace_root"])?))? != root {
        return Err(failure(
            "Cargo resolved a different workspace in the verification replica",
        ));
    }
    let members = metadata["workspace_members"]
        .as_array()
        .ok_or_else(|| return failure("malformed Cargo metadata"))?;
    let packages = metadata["packages"]
        .as_array()
        .ok_or_else(|| return failure("malformed Cargo metadata"))?;
    let mut found = false;
    for package in packages
        .iter()
        .filter(|package| return members.contains(&package["id"]))
    {
        found = true;
        relative_path(root, Path::new(&text(&package["manifest_path"])?))?;
        for target in package["targets"]
            .as_array()
            .ok_or_else(|| return failure("malformed Cargo metadata"))?
        {
            relative_path(root, Path::new(&text(&target["src_path"])?))?;
        }
        for dependency in package["dependencies"]
            .as_array()
            .ok_or_else(|| return failure("malformed Cargo metadata"))?
        {
            if let Some(path) = dependency["path"]
                .as_str()
                .filter(|path| return !path.is_empty())
            {
                relative_path(root, Path::new(path))?;
            }
        }
    }
    if !found {
        return Err(failure("the workspace has no compilable member packages"));
    }
    return Ok(());
}

fn validate_manifest_value(path: &Path, value: &toml::Value) -> Result<()> {
    match value {
        toml::Value::Table(table) => {
            for (key, child) in table {
                if matches!(key.as_str(), "path" | "build")
                    && let Some(name) = child
                        .as_str()
                        .filter(|name| return Path::new(name).is_absolute())
                {
                    return Err(failure(format!(
                        "absolute manifest path cannot be relocated safely: {}: {name}",
                        path.display()
                    )));
                }
                validate_manifest_value(path, child)?;
            }
        }
        toml::Value::Array(values) => {
            for child in values {
                validate_manifest_value(path, child)?;
            }
        }
        _ => {}
    }
    return Ok(());
}

pub(crate) fn validate_manifest_paths(root: &Path) -> Result<()> {
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            if kind.is_dir()
                && !IGNORED_DIRS
                    .iter()
                    .any(|ignored| return entry.file_name() == *ignored)
            {
                pending.push(entry.path());
            } else if entry.file_name() == "Cargo.toml" && !kind.is_dir() {
                let path = entry.path();
                let bytes = read_regular(&path)?;
                let document = str::from_utf8(&bytes).map_err(|error| {
                    return failure(format!(
                        "invalid Cargo manifest {}: {error}",
                        path.display()
                    ));
                })?;
                let value = toml::from_str(document).map_err(|error| {
                    return failure(format!(
                        "invalid Cargo manifest {}: {error}",
                        path.display()
                    ));
                })?;
                validate_manifest_value(&path, &value)?;
            }
        }
    }
    return Ok(());
}
