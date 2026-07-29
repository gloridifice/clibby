use std::{
    env,
    ffi::OsStr,
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct ClipEntry {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) kind: EntryKind,
    #[serde(default)]
    pub(crate) system_source: Option<PathBuf>,
    #[serde(default)]
    pub(crate) system_text: Option<String>,
    #[serde(default)]
    pub(crate) reference_only: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) enum EntryKind {
    File,
    Directory,
    Text,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct History {
    pub(crate) entries: Vec<ClipEntry>,
}

pub(crate) struct Store {
    items_dir: PathBuf,
    history_path: PathBuf,
}

impl Store {
    pub(crate) fn open() -> Result<Self> {
        let configured_root = data_root()?;
        let root = if configured_root.is_absolute() {
            configured_root
        } else {
            env::current_dir()
                .context("could not determine the current working directory")?
                .join(configured_root)
        };
        let items_dir = root.join("items");
        fs::create_dir_all(&items_dir).with_context(|| {
            format!(
                "could not create clipboard data directory: {}",
                items_dir.display()
            )
        })?;

        Ok(Self {
            history_path: root.join("history.json"),
            items_dir,
        })
    }

    pub(crate) fn load_history(&self) -> Result<History> {
        match fs::read(&self.history_path) {
            Ok(bytes) => serde_json::from_slice(&bytes).with_context(|| {
                format!(
                    "invalid clipboard history file: {}",
                    self.history_path.display()
                )
            }),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(History::default()),
            Err(error) => Err(error).with_context(|| {
                format!(
                    "could not read history file: {}",
                    self.history_path.display()
                )
            }),
        }
    }

    pub(crate) fn save_history(&self, history: &History) -> Result<()> {
        let contents =
            serde_json::to_vec_pretty(history).context("could not serialize clipboard history")?;
        fs::write(&self.history_path, contents).with_context(|| {
            format!(
                "could not write history file: {}",
                self.history_path.display()
            )
        })
    }

    fn content_path(&self, entry: &ClipEntry) -> Result<PathBuf> {
        if !is_safe_component(&entry.id) || !is_safe_component(&entry.name) {
            bail!("clipboard history contains an unsafe path component")
        }
        Ok(self
            .items_dir
            .join(&entry.id)
            .join("content")
            .join(&entry.name))
    }

    pub(crate) fn entry_path(&self, entry: &ClipEntry) -> Result<PathBuf> {
        if entry.reference_only {
            return entry
                .system_source
                .clone()
                .context("reference-only history entry has no source path");
        }
        self.content_path(entry)
    }

    pub(crate) fn clean(&self) -> Result<()> {
        if self.history_path.exists() {
            fs::remove_file(&self.history_path).with_context(|| {
                format!(
                    "could not remove history file: {}",
                    self.history_path.display()
                )
            })?;
        }
        if self.items_dir.exists() {
            fs::remove_dir_all(&self.items_dir).with_context(|| {
                format!(
                    "could not remove clipboard cache directory: {}",
                    self.items_dir.display()
                )
            })?;
        }
        Ok(())
    }
}

pub(crate) fn record_path_reference(store: &Store, source: &Path) -> Result<PathBuf> {
    let source_metadata = fs::symlink_metadata(source)
        .with_context(|| format!("could not read source path: {}", source.display()))?;
    if source_metadata.file_type().is_symlink() {
        bail!(
            "recording symbolic links is not supported: {}",
            source.display()
        );
    }

    let kind = if source_metadata.is_file() {
        EntryKind::File
    } else if source_metadata.is_dir() {
        EntryKind::Directory
    } else {
        bail!(
            "only regular files and directories are supported: {}",
            source.display()
        );
    };
    let system_source = fs::canonicalize(source)
        .with_context(|| format!("could not resolve source path: {}", source.display()))?;

    let name = source
        .file_name()
        .and_then(OsStr::to_str)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .with_context(|| {
            format!(
                "could not obtain a name for source path: {}",
                source.display()
            )
        })?;
    if !is_safe_component(&name) {
        bail!("source path has an invalid name: {name}");
    }

    let entry = ClipEntry {
        id: new_entry_id(),
        name,
        kind,
        system_source: Some(system_source.clone()),
        system_text: None,
        reference_only: true,
    };
    let mut history = store.load_history()?;
    history.entries.push(entry);
    store.save_history(&history)?;

    Ok(system_source)
}

pub(crate) fn snapshot_text_to_history(store: &Store, text: &str) -> Result<ClipEntry> {
    let id = new_entry_id();
    let name = "clipboard.txt".to_owned();
    let entry_dir = store.items_dir.join(&id);
    let content_dir = entry_dir.join("content");
    let stored_path = content_dir.join(&name);

    let result = (|| -> Result<()> {
        fs::create_dir_all(&content_dir).with_context(|| {
            format!(
                "could not create clipboard cache: {}",
                content_dir.display()
            )
        })?;
        fs::write(&stored_path, text.as_bytes()).with_context(|| {
            format!(
                "could not write text clipboard cache: {}",
                stored_path.display()
            )
        })
    })();
    if let Err(error) = result {
        let _ = fs::remove_dir_all(&entry_dir);
        return Err(error);
    }

    let entry = ClipEntry {
        id,
        name,
        kind: EntryKind::Text,
        system_source: None,
        system_text: Some(text.to_owned()),
        reference_only: false,
    };
    let mut history = store.load_history()?;
    history.entries.push(entry.clone());
    if let Err(error) = store.save_history(&history) {
        let _ = fs::remove_dir_all(&entry_dir);
        return Err(error);
    }

    Ok(entry)
}

pub(crate) fn history_entry(store: &Store, index: usize) -> Result<ClipEntry> {
    let history = store.load_history()?;
    history
        .entries
        .iter()
        .rev()
        .nth(index)
        .cloned()
        .with_context(|| {
            format!(
                "history index {index} does not exist; history contains {} item(s)",
                history.entries.len()
            )
        })
}

pub(crate) fn ensure_existing_content(path: &Path, entry: &ClipEntry) -> Result<()> {
    let exists = match entry.kind {
        EntryKind::File | EntryKind::Text => path.is_file(),
        EntryKind::Directory => path.is_dir(),
    };
    if !exists {
        bail!("clipboard cache is missing: {}", path.display());
    }
    Ok(())
}

fn data_root() -> Result<PathBuf> {
    if let Some(path) = env::var_os("CLB_DATA_DIR") {
        return Ok(PathBuf::from(path));
    }

    #[cfg(target_os = "windows")]
    {
        let base = env::var_os("LOCALAPPDATA")
            .or_else(|| env::var_os("APPDATA"))
            .context("LOCALAPPDATA and APPDATA are both unavailable")?;
        Ok(PathBuf::from(base).join("clipboarry"))
    }

    #[cfg(target_os = "macos")]
    {
        let home = env::var_os("HOME").context("HOME is unavailable")?;
        Ok(PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("clipboarry"))
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let base = match env::var_os("XDG_DATA_HOME") {
            Some(base) => PathBuf::from(base),
            None => PathBuf::from(env::var_os("HOME").context("HOME is unavailable")?)
                .join(".local")
                .join("share"),
        };
        Ok(base.join("clipboarry"))
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", unix)))]
    bail!("cannot determine a clipboard data directory on this operating system")
}

fn is_safe_component(component: &str) -> bool {
    !component.is_empty()
        && component != "."
        && component != ".."
        && !component.contains(['/', '\\'])
}

fn new_entry_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{nanos}-{}", std::process::id())
}

#[cfg(test)]
mod tests {
    use super::is_safe_component;

    #[test]
    fn safe_component_rejects_path_traversal() {
        assert!(is_safe_component("report.txt"));
        assert!(!is_safe_component(".."));
        assert!(!is_safe_component("nested/file.txt"));
    }
}
