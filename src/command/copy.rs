use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Arg, ArgMatches, Command as ClapCommand, value_parser};

use crate::{history::record_path_reference, platform::publish_system_clipboard};

use super::{Command, CommandContext};

pub(super) struct CopyCommand;

impl Command for CopyCommand {
    fn name(&self) -> &'static str {
        "copy"
    }

    fn cli_command(&self) -> Option<ClapCommand> {
        Some(
            ClapCommand::new(self.name())
                .visible_alias("c")
                .about("Record a file or directory reference in clipboard history")
                .arg(
                    Arg::new("path")
                        .required(true)
                        .value_parser(value_parser!(PathBuf))
                        .help("File or directory to copy"),
                ),
        )
    }

    fn execute(&self, context: &CommandContext, matches: &ArgMatches) -> Result<()> {
        let source = matches
            .get_one::<PathBuf>("path")
            .context("copy requires a source path")?;
        let system_source = record_path_reference(context.store(), source)?;

        publish_system_clipboard(&[system_source]).context(
            "recorded in clb history, but could not publish the file to the system clipboard",
        )?;

        println!(
            "Recorded reference in history and system clipboard: {}",
            source.display()
        );
        Ok(())
    }
}
