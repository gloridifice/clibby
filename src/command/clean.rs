use anyhow::Result;
use clap::{ArgMatches, Command as ClapCommand};

use super::{Command, CommandContext};

pub(super) struct CleanCommand;

impl Command for CleanCommand {
    fn name(&self) -> &'static str {
        "clean"
    }

    fn cli_command(&self) -> Option<ClapCommand> {
        Some(
            ClapCommand::new(self.name())
                .about("Remove clb history and cached text without modifying the system clipboard"),
        )
    }

    fn execute(&self, context: &CommandContext, _matches: &ArgMatches) -> Result<()> {
        context.store().clean()?;
        println!("Cleared clb history and cache. The system clipboard was not modified.");
        Ok(())
    }
}
