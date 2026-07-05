mod cli;
mod daemon;
mod tray;

fn main() -> anyhow::Result<()> {
    cli::run()
}
