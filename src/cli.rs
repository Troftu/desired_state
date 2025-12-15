use crate::state::DesiredState;
use anyhow::{Context, Result, anyhow};
use semver::VersionReq;
use std::env;
use std::path::PathBuf;

pub fn run() -> Result<()> {
    let (state_path, args) = parse_args(env::args().skip(1).collect())?;
    let mut state = DesiredState::load(state_path)?;

    match args.first().map(String::as_str) {
        Some("list") if args.len() == 1 => list_services(&state),
        Some("set") if args.len() == 3 => {
            let name = &args[1];
            let version_req = VersionReq::parse(&args[2]).with_context(|| {
                format!("invalid version requirement string for service {name}")
            })?;
            state.set_service(name.to_owned(), version_req.clone())?;
            println!("set {} to {}", name, version_req);
        }
        Some("remove") if args.len() == 2 => {
            let name = &args[1];
            if state.remove_service(name)? {
                println!("removed {name}");
            } else {
                println!("{name} was not present");
            }
        }
        _ => {
            print_usage();
            if args.is_empty() {
                anyhow::bail!("no command provided");
            } else {
                anyhow::bail!("unknown arguments: {}", args.join(" "));
            }
        }
    }

    Ok(())
}

fn list_services(state: &DesiredState) {
    for service in state.list() {
        println!("{} {}", service.name, service.version_req);
    }
}

fn parse_args(args: Vec<String>) -> Result<(PathBuf, Vec<String>)> {
    let mut desired_file =
        env::var("DESIRED_STATE_FILE").unwrap_or_else(|_| "desired_state.yml".to_string());

    let mut idx = 0;
    while idx < args.len() {
        match args[idx].as_str() {
            "--file" => {
                let value = args
                    .get(idx + 1)
                    .ok_or_else(|| anyhow!("--file requires a path"))?;
                desired_file = value.clone();
                idx += 2;
            }
            _ => break,
        }
    }

    Ok((PathBuf::from(desired_file), args[idx..].to_vec()))
}

fn print_usage() {
    eprintln!("Usage:");
    eprintln!("  desired_state [--file path] list");
    eprintln!("  desired_state [--file path] set <service> <version-req>");
    eprintln!("  desired_state [--file path] remove <service>");
    eprintln!();
    eprintln!("File defaults to desired_state.yml or $DESIRED_STATE_FILE if set.");
}
