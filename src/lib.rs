pub mod app;
pub mod auth;
pub mod cache;
pub mod cli;
pub mod config;
pub mod error;
pub mod export;
pub mod ga4;
pub mod tui;
pub mod ui;

pub const APP_NAME: &str = "ga4-omarchy";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
