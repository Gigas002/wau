//! Top-level command dispatch. `main` calls `run`; all logic lives here or in `libwau`.

use libwau::{
    lock::{self, Lock},
    manifest, ops, providers,
};

use crate::{
    cli::{Cli, Command},
    output,
    settings::{ListSettings, RemoveSettings, SettingsError, SyncSettings},
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
pub async fn run(cli: &Cli) -> Result<(), AppError> {
    match &cli.command {
        Command::List(_) => list(cli),
        Command::Sync(_) => sync(cli).await,
        Command::Remove(_) => remove(cli).await,
    }
}

fn list(cli: &Cli) -> Result<(), AppError> {
    let settings = ListSettings::for_list(cli)?;
    tracing::debug!(tag = %settings.tag, path = %settings.addons_path.display(), "scanning addons");

    let addons = libwau::fs::scan(&settings.addons_path)?;
    output::print_addon_list(&addons);
    Ok(())
}

async fn sync(cli: &Cli) -> Result<(), AppError> {
    let settings = SyncSettings::for_sync(cli)?;
    tracing::debug!(
        tag = %settings.tag,
        manifest = %settings.manifest_path.display(),
        lock = %settings.lock_path.display(),
        "syncing"
    );

    let manifest = manifest::load(&settings.manifest_path)?;

    let mut lock = match lock::load(&settings.lock_path) {
        Ok(l) => l,
        Err(libwau::Error::LockNotFound { .. }) => {
            tracing::debug!("lock not found; starting fresh");
            Lock::new(settings.tag.clone())
        }
        Err(e) => return Err(e.into()),
    };

    let ctx = providers::InstallContext {
        tag: settings.tag.clone(),
        flavor: settings.flavor.clone(),
        channel: settings.channel.clone(),
        addons_path: settings.addons_path.clone(),
        cache_dir: settings.cache_dir.clone(),
    };

    let mut installed = 0u32;
    let mut skipped = 0u32;

    for addon in &manifest.addon {
        // Filter: skip entries whose flavor list does not include this context's flavor.
        if let Some(flavors) = &addon.flavors
            && !flavors.contains(&ctx.flavor)
        {
            continue;
        }

        // Skip if already in lock and --update is not set.
        let already_locked = lock
            .addon
            .iter()
            .any(|a| a.name == addon.name && a.flavor == ctx.flavor);
        if already_locked && !settings.update {
            skipped += 1;
            continue;
        }

        let provider = providers::for_provider(&addon.provider)?;
        ops::install(provider.as_ref(), addon, &ctx, &mut lock).await?;
        output::print_installed(&addon.name);
        installed += 1;
    }

    lock::save(&lock, &settings.lock_path)?;
    output::print_sync_summary(installed, skipped);
    Ok(())
}

async fn remove(cli: &Cli) -> Result<(), AppError> {
    let settings = RemoveSettings::for_remove(cli)?;
    tracing::debug!(
        tag = %settings.tag,
        lock = %settings.lock_path.display(),
        addons = ?settings.addons,
        "removing addons"
    );

    let mut lock = lock::load(&settings.lock_path)?;

    let ctx = providers::InstallContext {
        tag: settings.tag.clone(),
        flavor: settings.flavor.clone(),
        channel: libwau::model::Channel::Stable,
        addons_path: settings.addons_path.clone(),
        cache_dir: std::path::PathBuf::new(),
    };

    for addon_name in &settings.addons {
        ops::remove(addon_name, &ctx, &mut lock).await?;
        output::print_removed(addon_name);
    }

    lock::save(&lock, &settings.lock_path)?;
    Ok(())
}
