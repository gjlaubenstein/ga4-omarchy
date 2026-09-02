use std::{fs::File, io, path::PathBuf};

use chrono::NaiveDate;
use clap::{Args, Parser, Subcommand};
use serde::Serialize;

use crate::{
    auth::AuthService,
    cache::{CacheStore, DEFAULT_TTL},
    config::{Config, Paths},
    error::{AppError, Result},
    export::{write_report, ExportFormat},
    ga4::{DateRange, Ga4Client},
};

#[derive(Debug, Parser)]
#[command(name = "ga4", version, about = "Google Analytics 4 in your terminal")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Manage Google OAuth authorization.
    Auth {
        #[command(subcommand)]
        command: AuthCommand,
    },
    /// List accessible Google Analytics properties.
    Properties {
        #[arg(long)]
        json: bool,
    },
    /// Export an Overview report as CSV or JSON.
    Export(ExportArgs),
    /// Manage locally cached historical reports.
    Cache {
        #[command(subcommand)]
        command: CacheCommand,
    },
    /// Inspect configuration and desktop integration.
    Doctor {
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum AuthCommand {
    /// Authorize using a user-provided Google Desktop OAuth client.
    Login {
        #[arg(long, value_name = "PATH")]
        client_secret: PathBuf,
    },
    /// Show whether a token exists in the system keyring.
    Status,
    /// Remove the OAuth token while retaining preferences and report cache.
    Logout,
}

#[derive(Debug, Subcommand)]
pub enum CacheCommand {
    /// Remove all cached historical report responses.
    Clear,
}

#[derive(Debug, Args)]
pub struct ExportArgs {
    #[arg(long)]
    property: String,
    #[arg(long, value_parser = parse_date)]
    start: NaiveDate,
    #[arg(long, value_parser = parse_date)]
    end: NaiveDate,
    #[arg(long, value_enum)]
    format: ExportFormat,
    #[arg(long)]
    output: Option<PathBuf>,
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    comparison: bool,
}

fn parse_date(value: &str) -> std::result::Result<NaiveDate, String> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| "expected YYYY-MM-DD".into())
}

pub async fn execute(cli: Cli) -> Result<()> {
    let paths = Paths::discover()?;
    let mut config = Config::load(&paths)?;
    let auth = AuthService::new(paths.clone());
    match cli.command {
        None => {
            let authenticated = auth.status()?;
            let client = Ga4Client::new(auth)?;
            crate::tui::run(paths, config, client, authenticated).await
        }
        Some(Command::Auth { command }) => match command {
            AuthCommand::Login { client_secret } => {
                auth.login(&client_secret).await?;
                println!("Authorized successfully.");
                Ok(())
            }
            AuthCommand::Status => {
                println!(
                    "{}",
                    if auth.status()? {
                        "authenticated"
                    } else {
                        "not authenticated"
                    }
                );
                Ok(())
            }
            AuthCommand::Logout => {
                auth.logout()?;
                println!("Signed out; cached reports and preferences were kept.");
                Ok(())
            }
        },
        Some(Command::Properties { json }) => {
            let properties = Ga4Client::new(auth)?.properties().await?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&properties)
                        .map_err(|error| AppError::Other(error.to_string()))?
                );
            } else {
                for property in &properties {
                    println!(
                        "{}\t{}\t{}",
                        property.id(),
                        property.account_name,
                        property.property_name
                    );
                }
            }
            if config.selected_property.is_none() && properties.len() == 1 {
                config.selected_property = Some(properties[0].id().into());
                config.selected_property_name = Some(properties[0].property_name.clone());
                config.save(&paths)?;
            }
            Ok(())
        }
        Some(Command::Export(args)) => {
            let range = DateRange::new(args.start, args.end)?;
            let cache = CacheStore::new(&paths);
            let key = CacheStore::key(&[
                &args.property,
                &args.start.to_string(),
                &args.end.to_string(),
                if args.comparison { "compare" } else { "single" },
            ]);
            let report = if config.cache_enabled {
                cache
                    .get(&key, DEFAULT_TTL)
                    .filter(|hit| hit.fresh)
                    .map(|hit| hit.value)
            } else {
                None
            };
            let report = match report {
                Some(report) => report,
                None => {
                    let report = Ga4Client::new(auth)?
                        .overview(&args.property, range, args.comparison)
                        .await?;
                    if config.cache_enabled {
                        cache.put(&key, report.fetched_at, &report)?;
                    }
                    report
                }
            };
            match args.output {
                Some(path) => write_report(
                    File::create(path).map_err(|error| AppError::Other(error.to_string()))?,
                    args.format,
                    &report,
                ),
                None => write_report(io::stdout().lock(), args.format, &report),
            }
        }
        Some(Command::Cache {
            command: CacheCommand::Clear,
        }) => {
            CacheStore::new(&paths).clear()?;
            println!("Report cache cleared.");
            Ok(())
        }
        Some(Command::Doctor { json }) => {
            let report = DoctorReport {
                version: crate::VERSION,
                platform: std::env::consts::OS,
                config_file: paths.config_file.display().to_string(),
                config_valid: true,
                oauth_client_present: paths.oauth_client_file.exists(),
                keyring_token_present: auth.status().unwrap_or(false),
                selected_property: config.selected_property.clone(),
                cache_enabled: config.cache_enabled,
                omarchy_launcher_present: which("omarchy-launch-or-focus-tui"),
                secret_service_available: which("secret-tool"),
            };
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&report)
                        .map_err(|error| AppError::Other(error.to_string()))?
                );
            } else {
                println!("ga4 {} on {}", report.version, report.platform);
                println!("OAuth client: {}", yes(report.oauth_client_present));
                println!("Keyring token: {}", yes(report.keyring_token_present));
                println!(
                    "Selected property: {}",
                    report.selected_property.as_deref().unwrap_or("none")
                );
                println!("Omarchy launcher: {}", yes(report.omarchy_launcher_present));
                println!("Secret Service: {}", yes(report.secret_service_available));
            }
            Ok(())
        }
    }
}

#[derive(Serialize)]
struct DoctorReport<'a> {
    version: &'a str,
    platform: &'a str,
    config_file: String,
    config_valid: bool,
    oauth_client_present: bool,
    keyring_token_present: bool,
    selected_property: Option<String>,
    cache_enabled: bool,
    omarchy_launcher_present: bool,
    secret_service_available: bool,
}

fn which(command: &str) -> bool {
    std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).any(|directory| directory.join(command).is_file()))
        .unwrap_or(false)
}
fn yes(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}
