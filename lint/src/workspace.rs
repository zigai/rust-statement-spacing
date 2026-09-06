use std::path::{Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};

#[derive(Clone)]
pub(crate) struct Workspace {
    root: PathBuf,
    include_generated: bool,
    exclusions: GlobSet,
}

impl Workspace {
    pub(crate) fn new(
        root: PathBuf,
        include_generated: bool,
        exclude: &[String],
    ) -> Result<Self, String> {
        let mut builder = GlobSetBuilder::new();
        for pattern in exclude {
            if pattern.starts_with('/') || pattern.split('/').any(|part| part == "..") {
                return Err(format!("exclusion must be workspace-relative: {pattern}"));
            }
            builder.add(Glob::new(pattern).map_err(|error| error.to_string())?);
        }
        Ok(Self {
            root,
            include_generated,
            exclusions: builder.build().map_err(|error| error.to_string())?,
        })
    }

    pub(crate) fn includes(&self, path: &Path, source: &str) -> bool {
        let Ok(relative) = path.strip_prefix(&self.root) else {
            return false;
        };
        let relative = relative.to_string_lossy().replace('\\', "/");
        if relative.split('/').any(|part| part == "target") || self.exclusions.is_match(&relative) {
            return false;
        }
        self.include_generated
            || !source.lines().take(20).any(|line| {
                let lower = line.to_ascii_lowercase();
                lower.contains("@generated")
                    || lower.contains("do not edit")
                    || lower.contains("automatically generated")
            })
    }
}

pub(crate) fn root() -> PathBuf {
    if let Some(root) = std::env::var_os("STATEMENT_SPACING_WORKSPACE") {
        return PathBuf::from(root);
    }
    let start = std::env::var_os("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let mut fallback = start.clone();
    for parent in start.ancestors() {
        if parent.join("dylint.toml").is_file() {
            return canonical(parent);
        }
        let Ok(contents) = std::fs::read_to_string(parent.join("Cargo.toml")) else {
            continue;
        };
        if let Ok(value) = toml::from_str::<toml::Value>(&contents) {
            if value.get("workspace").is_some() {
                return canonical(parent);
            }
            if parent == start {
                fallback = parent.to_path_buf();
            }
        }
    }
    canonical(&fallback)
}

pub(crate) fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}
