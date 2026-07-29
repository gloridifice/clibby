use anyhow::{Context, Result};
use clap::{Arg, ArgMatches, Command as ClapCommand, value_parser};

use crate::{
    history::{ensure_existing_content, history_entry},
    platform::{open_with_default_application, sync_system_clipboard},
};

use super::{Command, CommandContext};

pub(super) struct InvokeCommand;

impl Command for InvokeCommand {
    fn name(&self) -> &'static str {
        "invoke"
    }

    fn cli_command(&self) -> Option<ClapCommand> {
        Some(
            ClapCommand::new(self.name())
                .visible_alias("i")
                .about("Open a history item with its system-default application")
                .arg(
                    Arg::new("index")
                        .default_value("0")
                        .value_parser(value_parser!(usize))
                        .help("History index; 0 is the most recent item"),
                ),
        )
    }

    fn execute(&self, context: &CommandContext, matches: &ArgMatches) -> Result<()> {
        sync_system_clipboard(context.store());
        let index = matches
            .get_one::<usize>("index")
            .copied()
            .context("invoke requires a history index")?;
        let entry = history_entry(context.store(), index)?;
        let path = context.store().entry_path(&entry)?;
        ensure_existing_content(&path, &entry)?;

        open_with_default_application(&path).with_context(|| {
            format!(
                "could not open with the default application: {}",
                path.display()
            )
        })?;
        println!("Opened: {}", path.display());
        Ok(())
    }
}
