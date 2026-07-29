use std::{ffi::OsStr, path::Path};

use anyhow::Result;
use clap::{Arg, ArgMatches, Command as ClapCommand, value_parser};

use crate::{
    history::{ClipEntry, EntryKind, Store},
    platform::sync_system_clipboard,
};

use super::{Command, CommandContext};

pub(super) struct ListCommand;

impl Command for ListCommand {
    fn name(&self) -> &'static str {
        "list"
    }

    fn cli_command(&self) -> Option<ClapCommand> {
        Some(
            ClapCommand::new(self.name())
                .visible_alias("ls")
                .about("List clipboard history and sync the current system clipboard")
                .arg(
                    Arg::new("number")
                        .short('n')
                        .long("number")
                        .default_value("8")
                        .value_parser(value_parser!(usize))
                        .help("Number of recent items to show; 0 prints nothing"),
                ),
        )
    }

    fn execute(&self, context: &CommandContext, matches: &ArgMatches) -> Result<()> {
        let number = matches
            .get_one::<usize>("number")
            .copied()
            .expect("list number has a Clap default");
        sync_system_clipboard(context.store());
        list_history(context.store(), number)
    }
}

pub(crate) fn list_history(store: &Store, number: usize) -> Result<()> {
    if number == 0 {
        return Ok(());
    }

    let history = store.load_history()?;
    if history.entries.is_empty() {
        println!("Clipboard history is empty.");
        return Ok(());
    }

    for (index, entry) in history.entries.iter().rev().take(number).enumerate() {
        println!(
            "[{index}][{}] {}",
            display_type(entry),
            display_content(store, entry)
        );
    }
    Ok(())
}

fn display_type(entry: &ClipEntry) -> String {
    match entry.kind {
        EntryKind::Text => "text".to_owned(),
        EntryKind::Directory => "directory".to_owned(),
        EntryKind::File => entry
            .system_source
            .as_deref()
            .unwrap_or_else(|| Path::new(&entry.name))
            .extension()
            .and_then(OsStr::to_str)
            .filter(|extension| !extension.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| "file".to_owned()),
    }
}

fn display_content(store: &Store, entry: &ClipEntry) -> String {
    match entry.kind {
        EntryKind::Text => text_preview(entry.system_text.as_deref().unwrap_or_default()),
        EntryKind::File | EntryKind::Directory => entry
            .system_source
            .clone()
            .or_else(|| store.entry_path(entry).ok())
            .map(|path| display_path(&path))
            .unwrap_or_else(|| entry.name.clone()),
    }
}

fn display_path(path: &Path) -> String {
    let path = path.display().to_string();

    #[cfg(target_os = "windows")]
    {
        if let Some(path) = path.strip_prefix("\\\\?\\UNC\\") {
            return format!("\\\\{path}");
        }
        if let Some(path) = path.strip_prefix("\\\\?\\") {
            return path.to_owned();
        }
    }

    path
}

fn text_preview(text: &str) -> String {
    const PREVIEW_LIMIT: usize = 100;
    let mut preview = text.replace("\r\n", " ").replace(['\r', '\n'], " ");
    if preview.chars().count() > PREVIEW_LIMIT {
        preview = preview.chars().take(PREVIEW_LIMIT).collect::<String>();
        preview.push_str("...");
    }
    preview
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::history::{ClipEntry, EntryKind};

    use super::{display_type, text_preview};

    #[test]
    fn display_type_uses_file_extension_or_file() {
        let mut entry = ClipEntry {
            id: "id".to_owned(),
            name: "README".to_owned(),
            kind: EntryKind::File,
            system_source: Some(PathBuf::from("C:/work/README")),
            system_text: None,
            reference_only: true,
        };
        assert_eq!(display_type(&entry), "file");

        entry.system_source = Some(PathBuf::from("C:/work/main.rs"));
        assert_eq!(display_type(&entry), "rs");
    }

    #[test]
    fn text_preview_is_limited_to_one_hundred_characters() {
        let text = "x".repeat(101);
        assert_eq!(text_preview(&text), format!("{}...", "x".repeat(100)));
        assert_eq!(text_preview("first\r\nsecond"), "first second");
    }
}
