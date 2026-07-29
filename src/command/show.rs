use anyhow::Result;
use clap::{ArgMatches, Command as ClapCommand};

use crate::platform::sync_system_clipboard;

use super::{Command, CommandContext, list::list_history};

pub(super) struct ShowCommand;

impl Command for ShowCommand {
    fn name(&self) -> &'static str {
        "show"
    }

    fn cli_command(&self) -> Option<ClapCommand> {
        None
    }

    fn execute(&self, context: &CommandContext, _matches: &ArgMatches) -> Result<()> {
        sync_system_clipboard(context.store());
        list_history(context.store(), 1)
    }
}
