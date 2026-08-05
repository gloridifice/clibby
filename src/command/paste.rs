use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgMatches, Command as ClapCommand, value_parser};

use crate::{
    history::{ClipEntry, EntryKind, ensure_existing_content, history_group},
    platform::sync_system_clipboard,
};

use super::{Command, CommandContext};

pub(super) struct PasteCommand;

impl Command for PasteCommand {
    fn name(&self) -> &'static str {
        "paste"
    }

    fn cli_command(&self) -> Option<ClapCommand> {
        Some(
            ClapCommand::new(self.name())
                .visible_alias("p")
                .about("Copy a referenced history item, or write a text item to standard output")
                .arg(
                    Arg::new("target")
                        .value_parser(value_parser!(PathBuf))
                        .help("Destination for a file or directory; invalid for text"),
                )
                .arg(
                    Arg::new("index")
                        .short('i')
                        .long("index")
                        .short_alias('n')
                        .visible_short_alias('n')
                        .default_value("0")
                        .value_parser(value_parser!(usize))
                        .help("History index; 0 is the most recent item"),
                ),
        )
    }

    fn execute(&self, context: &CommandContext, matches: &ArgMatches) -> Result<()> {
        sync_system_clipboard(context.store());
        let target = matches.get_one::<PathBuf>("target").map(PathBuf::as_path);
        let index = matches
            .get_one::<usize>("index")
            .copied()
            .context("paste requires a history index")?;
        let entries = history_group(context.store(), index)?;

        if let Some(text_entry) = entries.iter().find(|entry| entry.kind == EntryKind::Text) {
            if entries.len() != 1 {
                bail!("cannot paste a text entry together with other history items");
            }
            validate_text_paste_target(target)?;

            let source = context.store().entry_path(text_entry)?;
            ensure_existing_content(&source, text_entry)?;
            let mut stdout = std::io::stdout().lock();
            write_text_clipboard(&source, &mut stdout)
                .context("could not write text clipboard content to standard output")?;
            return Ok(());
        }

        let mut sources = Vec::with_capacity(entries.len());
        for entry in &entries {
            let source = context.store().entry_path(entry)?;
            ensure_existing_content(&source, entry)?;
            sources.push(source);
        }

        let destinations = paste_destinations(target, &entries)?;
        for (entry, (source, destination)) in
            entries.iter().zip(sources.iter().zip(&destinations))
        {
            if destination.exists() {
                bail!(
                    "destination already exists; refusing to overwrite it: {}",
                    destination.display()
                );
            }
            if entry.kind == EntryKind::Directory && destination.starts_with(source) {
                bail!(
                    "cannot paste a directory inside itself: {}",
                    destination.display()
                );
            }
        }

        let mut pasted: Vec<usize> = Vec::new();
        let result = (|| -> Result<()> {
            for (index, (entry, (source, destination))) in entries
                .iter()
                .zip(sources.iter().zip(&destinations))
                .enumerate()
            {
                if let Some(parent) = destination.parent() {
                    fs::create_dir_all(parent).with_context(|| {
                        format!(
                            "could not create destination directory: {}",
                            parent.display()
                        )
                    })?;
                }
                copy_path(source, destination, entry.kind)?;
                pasted.push(index);
                println!("Pasted to: {}", destination.display());
            }
            Ok(())
        })();
        if let Err(error) = result {
            for &index in pasted.iter().rev() {
                let _ = remove_path(&destinations[index], entries[index].kind);
            }
            return Err(error);
        }

        Ok(())
    }
}

fn validate_text_paste_target(target: Option<&Path>) -> Result<()> {
    if let Some(target) = target {
        bail!(
            "a destination path is invalid when pasting text; text is written to standard output: {}",
            target.display()
        );
    }
    Ok(())
}

fn write_text_clipboard(source: &Path, output: &mut impl Write) -> Result<()> {
    let text = fs::read(source)
        .with_context(|| format!("could not read text clipboard cache: {}", source.display()))?;
    output
        .write_all(&text)
        .context("could not write text clipboard content")
}

fn paste_destinations(target: Option<&Path>, entries: &[ClipEntry]) -> Result<Vec<PathBuf>> {
    let current_dir =
        env::current_dir().context("could not determine the current working directory")?;
    let destinations = match (target, entries.len()) {
        // No target: every item lands in the current directory.
        (None, _) => entries
            .iter()
            .map(|entry| current_dir.join(&entry.name))
            .collect(),
        // Exact destination name is only meaningful for a single item.
        (Some(target), 1) if !has_trailing_separator(target) => {
            vec![absolute_path(&current_dir, target)]
        }
        // A trailing separator means "paste into this directory".
        (Some(target), _) if has_trailing_separator(target) => {
            let directory = absolute_path(&current_dir, target);
            entries
                .iter()
                .map(|entry| directory.join(&entry.name))
                .collect()
        }
        (Some(target), count) => bail!(
            "a file destination is invalid when pasting {count} items (use a directory with a trailing separator): {}",
            target.display()
        ),
    };

    Ok(destinations)
}

fn absolute_path(current_dir: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        current_dir.join(path)
    }
}

fn has_trailing_separator(path: &Path) -> bool {
    path.as_os_str().to_string_lossy().ends_with(['/', '\\'])
}

fn copy_path(source: &Path, destination: &Path, kind: EntryKind) -> Result<()> {
    match kind {
        EntryKind::File | EntryKind::Text => {
            fs::copy(source, destination).with_context(|| {
                format!(
                    "could not copy file: {} -> {}",
                    source.display(),
                    destination.display()
                )
            })?;
        }
        EntryKind::Directory => copy_directory(source, destination)?,
    }
    Ok(())
}

fn copy_directory(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir(destination).with_context(|| {
        format!(
            "could not create destination directory: {} -> {}",
            source.display(),
            destination.display()
        )
    })?;

    for child in fs::read_dir(source)
        .with_context(|| format!("could not read directory: {}", source.display()))?
    {
        let child =
            child.with_context(|| format!("could not traverse directory: {}", source.display()))?;
        let child_source = child.path();
        let child_destination = destination.join(child.file_name());
        let file_type = child
            .file_type()
            .with_context(|| format!("could not read file type: {}", child_source.display()))?;

        if file_type.is_symlink() {
            bail!(
                "copying symbolic links inside directories is not supported: {}",
                child_source.display()
            );
        }
        if file_type.is_dir() {
            copy_directory(&child_source, &child_destination)?;
        } else if file_type.is_file() {
            fs::copy(&child_source, &child_destination).with_context(|| {
                format!(
                    "could not copy file: {} -> {}",
                    child_source.display(),
                    child_destination.display()
                )
            })?;
        } else {
            bail!(
                "directory contains an unsupported special file: {}",
                child_source.display()
            );
        }
    }
    Ok(())
}

fn remove_path(path: &Path, kind: EntryKind) -> Result<()> {
    match kind {
        EntryKind::File | EntryKind::Text => fs::remove_file(path),
        EntryKind::Directory => fs::remove_dir_all(path),
    }
    .with_context(|| {
        format!(
            "could not remove incomplete destination: {}",
            path.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use std::{env, fs, path::Path};

    use crate::history::{ClipEntry, EntryKind};

    use super::{
        has_trailing_separator, paste_destinations, validate_text_paste_target, write_text_clipboard,
    };

    fn file_entry(name: &str) -> ClipEntry {
        ClipEntry {
            id: name.to_owned(),
            name: name.to_owned(),
            kind: EntryKind::File,
            system_source: None,
            system_text: None,
            reference_only: false,
            group: Some("g1".to_owned()),
        }
    }

    #[test]
    fn trailing_separator_means_directory_target() {
        assert!(has_trailing_separator(Path::new("foo/")));
        assert!(has_trailing_separator(Path::new("foo\\")));
        assert!(!has_trailing_separator(Path::new("foo")));
    }

    #[test]
    fn text_paste_rejects_a_destination_path() {
        assert!(validate_text_paste_target(None).is_ok());
        assert!(validate_text_paste_target(Some(Path::new("output.txt"))).is_err());
    }

    #[test]
    fn text_paste_writes_exact_cached_bytes() {
        let path = env::temp_dir().join(format!("clb-test-{}", std::process::id()));
        let expected = b"first line\r\nsecond line\n";
        fs::write(&path, expected).unwrap();

        let mut output = Vec::new();
        write_text_clipboard(&path, &mut output).unwrap();
        let _ = fs::remove_file(&path);

        assert_eq!(output, expected);
    }

    #[test]
    fn group_paste_without_target_uses_current_directory() {
        let entries = vec![file_entry("a.txt"), file_entry("b.txt"), file_entry("c.txt")];
        let destinations = paste_destinations(None, &entries).unwrap();
        let current = env::current_dir().unwrap();
        assert_eq!(
            destinations,
            vec![
                current.join("a.txt"),
                current.join("b.txt"),
                current.join("c.txt"),
            ]
        );
    }

    #[test]
    fn group_paste_into_directory_target_keeps_each_name() {
        let entries = vec![file_entry("a.txt"), file_entry("b.txt")];
        let destinations =
            paste_destinations(Some(Path::new("out\\")), &entries).unwrap();
        let current = env::current_dir().unwrap();
        assert_eq!(
            destinations,
            vec![current.join("out").join("a.txt"), current.join("out").join("b.txt")]
        );
    }

    #[test]
    fn group_paste_rejects_an_exact_file_target() {
        let entries = vec![file_entry("a.txt"), file_entry("b.txt")];
        assert!(paste_destinations(Some(Path::new("out.txt")), &entries).is_err());
    }

    #[test]
    fn single_item_paste_keeps_exact_destination_name() {
        let entries = vec![file_entry("a.txt")];
        let destinations = paste_destinations(Some(Path::new("out.txt")), &entries).unwrap();
        assert_eq!(
            destinations,
            vec![env::current_dir().unwrap().join("out.txt")]
        );
    }
}
