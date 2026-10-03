mod archive;
mod cleanup;
mod cli;
mod declaration;
mod distribution;
mod error;
mod files;
mod identity;
mod install;
mod links;
mod manager;
mod proxy;
mod selection;
mod state;
mod toolchain;

use std::process::ExitCode;

fn main() -> ExitCode {
    let mut arguments = std::env::args_os();
    let invoked = arguments.next().unwrap_or_default();
    let args: Vec<_> = arguments.collect();
    let result = if env!("CARGO_BIN_NAME") == "qleisliup-init" {
        manager::init_cli(&args)
    } else {
        match proxy::invoked_tool(&invoked) {
            Some(tool) => proxy::run(tool, &args),
            None => cli::run(&args),
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(error.status)
        }
    }
}
