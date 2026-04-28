//! Top-level command dispatch. `main` calls `run`; all logic lives here or in `libwau`.

use libwau::{
    lock::{self, Lock},
    manifest, ops, providers,
};

use crate::{
    output,
    settings::{
        CommandSettings, InfoSettings, ListSettings, RemoveSettings, SearchSettings, Settings,
        SyncSettings,
    },
};

#[cfg(test)]
mod tests;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    Libwau(#[from] libwau::Error),

    #[error("addon '{name}' not found in manifest or lock")]
    AddonNotFound { name: String },
}

/// Dispatches the resolved settings to the appropriate command handler.
pub async fn run(settings: Settings) -> Result<(), AppError> {
    let quiet = settings.quiet;
    let noconfirm = settings.noconfirm;
    match settings.command {
        CommandSettings::List(s) => list(s),
        CommandSettings::Sync(s) => sync(s, quiet, noconfirm).await,
        CommandSettings::Remove(s) => remove(s, quiet, noconfirm).await,
        CommandSettings::Search(s) => search(s),
        CommandSettings::Info(s) => info(s),
    }
}

fn list(s: ListSettings) -> Result<(), AppError> {
    tracing::debug!(tag = %s.tag, path = %s.addons_path.display(), "scanning addons");
    let addons = libwau::fs::scan(&s.addons_path)?;
    output::print_addon_list(&addons);
    Ok(())
}

async fn sync(s: SyncSettings, quiet: bool, noconfirm: bool) -> Result<(), AppError> {
    tracing::debug!(
        tag = %s.tag,
        manifest = %s.manifest_path.display(),
        lock = %s.lock_path.display(),
        "syncing"
    );

    let manifest = manifest::load(&s.manifest_path)?;

    let mut lock = match lock::load(&s.lock_path) {
        Ok(l) => l,
        Err(libwau::Error::LockNotFound { .. }) => {
            tracing::debug!("lock not found; starting fresh");
            Lock::new(s.tag.clone())
        }
        Err(e) => return Err(e.into()),
    };

    let ctx = providers::InstallContext {
        tag: s.tag.clone(),
        flavor: s.flavor.clone(),
        channel: s.channel.clone(),
        addons_path: s.addons_path.clone(),
        cache_dir: s.cache_dir.clone(),
    };

    let plan = libwau::resolve::plan(&manifest, &lock, &ctx.flavor, s.update);

    if plan.to_install.is_empty() {
        output::print_sync_summary(0, plan.skipped as u32, quiet);
        return Ok(());
    }

    output::print_sync_plan(&plan.to_install);

    if !noconfirm && !output::confirm("Proceed with installation?") {
        println!("Aborted.");
        return Ok(());
    }

    let mut installed = 0u32;
    for addon in plan.to_install {
        let provider = providers::for_provider(&addon.provider, &s.provider_config)?;
        ops::install(provider.as_ref(), addon, &ctx, &mut lock).await?;
        output::print_installed(&addon.name, quiet);
        installed += 1;
    }

    lock::save(&lock, &s.lock_path)?;
    output::print_sync_summary(installed, plan.skipped as u32, quiet);
    Ok(())
}

async fn remove(s: RemoveSettings, quiet: bool, noconfirm: bool) -> Result<(), AppError> {
    tracing::debug!(
        tag = %s.tag,
        lock = %s.lock_path.display(),
        addons = ?s.addons,
        "removing addons"
    );

    let mut lock = lock::load(&s.lock_path)?;

    let ctx = providers::InstallContext {
        tag: s.tag.clone(),
        flavor: s.flavor.clone(),
        channel: libwau::model::Channel::Stable,
        addons_path: s.addons_path.clone(),
        cache_dir: std::path::PathBuf::new(),
    };

    output::print_remove_plan(&s.addons);

    if !noconfirm && !output::confirm("Proceed with removal?") {
        println!("Aborted.");
        return Ok(());
    }

    for addon_name in &s.addons {
        ops::remove(addon_name, &ctx, &mut lock).await?;
        output::print_removed(addon_name, quiet);
    }

    lock::save(&lock, &s.lock_path)?;
    Ok(())
}

fn search(s: SearchSettings) -> Result<(), AppError> {
    tracing::debug!(query = %s.query, tag = %s.tag, "searching");

    let manifest = match manifest::load(&s.manifest_path) {
        Ok(m) => m,
        Err(libwau::Error::ManifestNotFound { .. }) => manifest::Manifest {
            schema: manifest::SUPPORTED_SCHEMA,
            addon: Vec::new(),
        },
        Err(e) => return Err(e.into()),
    };

    let installed = libwau::fs::scan(&s.addons_path).unwrap_or_default();
    let query = s.query.to_lowercase();

    let manifest_matches: Vec<&libwau::manifest::ManifestAddon> = manifest
        .addon
        .iter()
        .filter(|a| a.name.to_lowercase().contains(&query))
        .collect();

    // Show installed addons not already shown via manifest to avoid duplicates.
    let manifest_names: std::collections::HashSet<&str> =
        manifest_matches.iter().map(|a| a.name.as_str()).collect();

    let installed_matches: Vec<&libwau::fs::InstalledAddon> = installed
        .iter()
        .filter(|a| {
            let title_match = a.display_title().to_lowercase().contains(&query);
            let folder_match = a.folder.to_lowercase().contains(&query);
            (title_match || folder_match) && !manifest_names.contains(a.display_title())
        })
        .collect();

    output::print_search_results(&s.query, &manifest_matches, &installed_matches);
    Ok(())
}

fn info(s: InfoSettings) -> Result<(), AppError> {
    tracing::debug!(addon = %s.addon_name, tag = %s.tag, "info");

    let manifest = match manifest::load(&s.manifest_path) {
        Ok(m) => m,
        Err(libwau::Error::ManifestNotFound { .. }) => manifest::Manifest {
            schema: manifest::SUPPORTED_SCHEMA,
            addon: Vec::new(),
        },
        Err(e) => return Err(e.into()),
    };

    let lock = match lock::load(&s.lock_path) {
        Ok(l) => l,
        Err(libwau::Error::LockNotFound { .. }) => Lock::new(s.tag.clone()),
        Err(e) => return Err(e.into()),
    };

    let manifest_entry = manifest.addon.iter().find(|a| a.name == s.addon_name);
    let lock_entry = lock.addon.iter().find(|a| a.name == s.addon_name);

    if manifest_entry.is_none() && lock_entry.is_none() {
        return Err(AppError::AddonNotFound {
            name: s.addon_name.clone(),
        });
    }

    output::print_addon_info(&s.addon_name, manifest_entry, lock_entry);
    Ok(())
}
