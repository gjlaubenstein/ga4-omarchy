use clap::Parser;
use ga4_omarchy::{
    cli::{execute, Cli},
    error::AppError,
};

#[tokio::main]
async fn main() {
    if let Err(error) = color_eyre::install() {
        eprintln!("warning: could not install error handler: {error}");
    }
    if let Err(error) = execute(Cli::parse()).await {
        eprintln!("ga4: {error}");
        std::process::exit(exit_code(&error));
    }
}

fn exit_code(error: &AppError) -> i32 {
    i32::from(error.exit_code())
}
