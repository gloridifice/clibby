use std::collections::HashMap;

use anyhow::{Context, Result};
use clap::{ArgMatches, Command as ClapCommand};

use crate::history::Store;

mod clean;
mod copy;
mod invoke;
mod list;
mod paste;
mod show;

pub(crate) struct CommandContext {
    store: Store,
}

impl CommandContext {
    pub(crate) fn open() -> Result<Self> {
        Ok(Self {
            store: Store::open()?,
        })
    }

    pub(crate) fn store(&self) -> &Store {
        &self.store
    }
}

pub(crate) trait Command {
    fn name(&self) -> &'static str;
    fn cli_command(&self) -> Option<ClapCommand>;
    fn execute(&self, context: &CommandContext, matches: &ArgMatches) -> Result<()>;
}

pub(crate) struct CommandRegistry {
    commands: HashMap<&'static str, Box<dyn Command>>,
    order: Vec<&'static str>,
}

impl CommandRegistry {
    pub(crate) fn new() -> Self {
        let mut registry = Self {
            commands: HashMap::new(),
            order: Vec::new(),
        };
        registry.register(copy::CopyCommand);
        registry.register(paste::PasteCommand);
        registry.register(list::ListCommand);
        registry.register(clean::CleanCommand);
        registry.register(invoke::InvokeCommand);
        registry.register(show::ShowCommand);
        registry
    }

    pub(crate) fn clap_command(&self) -> ClapCommand {
        self.order
            .iter()
            .filter_map(|name| self.commands.get(name))
            .filter_map(|command| command.cli_command())
            .fold(
                ClapCommand::new("clb")
                    .version(env!("CARGO_PKG_VERSION"))
                    .about("A lightweight file clipboard with persistent history")
                    .subcommand_required(false)
                    .arg_required_else_help(false),
                |cli, command| cli.subcommand(command),
            )
    }

    pub(crate) fn execute(
        &self,
        name: &str,
        context: &CommandContext,
        matches: &ArgMatches,
    ) -> Result<()> {
        self.commands
            .get(name)
            .with_context(|| format!("unregistered command: {name}"))?
            .execute(context, matches)
    }

    fn register(&mut self, command: impl Command + 'static) {
        let name = command.name();
        assert!(
            self.commands.insert(name, Box::new(command)).is_none(),
            "duplicate command registration: {name}"
        );
        self.order.push(name);
    }
}
