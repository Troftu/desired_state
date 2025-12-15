mod cli;
mod state;

fn main() -> anyhow::Result<()> {
    cli::run()
}
