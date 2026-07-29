mod cli;
mod command;
mod history;
mod platform;

fn main() -> anyhow::Result<()> {
    cli::run()
}
