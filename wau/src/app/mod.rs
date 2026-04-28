//! Top-level command dispatch. `main` calls `run`; all logic lives here or in `libwau`.

use crate::{
    cli::{Cli, Command},
    output,
    settings::{Settings, SettingsError},
};

#[cfg(test)]
mod tests;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    Settings(#[from] SettingsError),

    #[error("{0}")]
    Libwau(#[from] libwau::Error),
}

/// Dispatches the parsed CLI command and returns an exit code (0 = success).
pub fn run(cli: &Cli) -> Result<(), AppError> {
    match &cli.command {
        Command::List(_) => list(cli),
    }
}

fn list(cli: &Cli) -> Result<(), AppError> {
    let settings = Settings::for_list(cli)?;
    tracing::debug!(tag = %settings.tag, path = %settings.addons_path.display(), "scanning addons");

    let addons = libwau::fs::scan(&settings.addons_path)?;
    output::print_addon_list(&addons);
    Ok(())
}
