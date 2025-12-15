mod cli;
mod desired_state_file;
mod error;
mod state;

use error::AppResult;

fn main() -> AppResult<()> {
    env_logger::init();
    cli::run()
}
