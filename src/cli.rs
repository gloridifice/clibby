use std::io::{self, IsTerminal, Read};

use anyhow::{Context, Result};

use crate::{
    command::{CommandContext, CommandRegistry},
    history::snapshot_text_to_history,
    platform::publish_system_clipboard_text,
};

pub(crate) fn run() -> Result<()> {
    let registry = CommandRegistry::new();
    let matches = registry.clap_command().get_matches();
    let piped_text =
        should_capture_piped_text(matches.subcommand().is_some(), io::stdin().is_terminal())
            .then(read_piped_text)
            .transpose()?;
    let context = CommandContext::open()?;

    if let Some(text) = piped_text {
        snapshot_text_to_history(context.store(), &text)?;
        publish_system_clipboard_text(&text).context(
            "recorded piped text in clb history, but could not publish it to the system clipboard",
        )?;
        return Ok(());
    }

    match matches.subcommand() {
        Some((name, command_matches)) => registry.execute(name, &context, command_matches),
        None => registry.execute("show", &context, &matches),
    }
}

fn should_capture_piped_text(has_subcommand: bool, stdin_is_terminal: bool) -> bool {
    !has_subcommand && !stdin_is_terminal
}

fn read_piped_text() -> Result<String> {
    let mut text = String::new();
    io::stdin()
        .lock()
        .read_to_string(&mut text)
        .context("could not read UTF-8 text from standard input")?;
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::should_capture_piped_text;

    #[test]
    fn only_a_no_subcommand_invocation_with_redirected_stdin_captures_text() {
        assert!(should_capture_piped_text(false, false));
        assert!(!should_capture_piped_text(false, true));
        assert!(!should_capture_piped_text(true, false));
    }
}
