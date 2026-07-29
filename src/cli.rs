use anyhow::Result;

use crate::command::{CommandContext, CommandRegistry};

pub(crate) fn run() -> Result<()> {
    let registry = CommandRegistry::new();
    let matches = registry.clap_command().get_matches();
    let context = CommandContext::open()?;

    match matches.subcommand() {
        Some((name, command_matches)) => registry.execute(name, &context, command_matches),
        None => registry.execute("show", &context, &matches),
    }
}
