use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Arg, ArgMatches, Command as ClapCommand, value_parser};

use crate::{
    history::{new_group_id, record_path_reference},
    platform::publish_system_clipboard,
};

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
                .about("Record file or directory references in clipboard history")
                .arg(
                    Arg::new("path")
                        .required(true)
                        .num_args(1..)
                        .value_parser(value_parser!(PathBuf))
                        .help("File or directory to copy; multiple paths are copied as one selection"),
                ),
        )
    }

    fn execute(&self, context: &CommandContext, matches: &ArgMatches) -> Result<()> {
        let sources: Vec<&PathBuf> = matches
            .get_many::<PathBuf>("path")
            .context("copy requires at least one source path")?
            .collect();

        // Multiple paths form one clipboard selection, so they share a group
        // id and `paste` restores all of them at once.
        let group = (sources.len() > 1).then(new_group_id);
        let mut system_sources = Vec::with_capacity(sources.len());
        for source in sources {
            let system_source = record_path_reference(context.store(), source, group.as_deref())?;
            system_sources.push(system_source);
            println!(
                "Recorded reference in history and system clipboard: {}",
                source.display()
            );
        }

        publish_system_clipboard(&system_sources).context(
            "recorded in clb history, but could not publish the files to the system clipboard",
        )?;
        Ok(())
    }
}
