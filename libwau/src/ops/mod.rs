//! Install/remove orchestration (library-level).
//!
//! All filesystem operations are delegated to [`crate::fs`]; this module owns
//! only the staging lifecycle, provider calls, and lock bookkeeping.
//!
//! Install flow (§5.1):
//!  1. Provider resolves manifest row → `ResolvedArtifact`.
//!  2. Zip is downloaded to a staging directory under `cache_dir`.
//!  3. Zip is extracted; top-level dirs with at least one `.toc` are identified.
//!  4. Each addon dir is copied into `addons_path` (existing dir replaced).
//!  5. Lock is updated with the resolved artifact + installed directories.
//!
//! Remove flow:
//!  1. Lock entry is found by addon name + flavor.
//!  2. Each recorded directory is removed from `addons_path`.
//!  3. Lock entry is removed.

use std::{
    fs,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

use chrono::Utc;

use crate::{
    Result,
    lock::{Lock, LockedAddon},
    manifest::ManifestAddon,
    providers::{InstallContext, Provider},
};

#[cfg(test)]
mod tests;

static STAGING_COUNTER: AtomicU64 = AtomicU64::new(0);

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Installs an addon from the given manifest row into the install context.
///
/// Downloads the artifact, extracts the zip, copies addon directories into
/// `ctx.addons_path`, and records the result in `lock`.
pub fn install(
    provider: &dyn Provider,
    addon: &ManifestAddon,
    ctx: &InstallContext,
    lock: &mut Lock,
) -> Result<()> {
    let artifact = provider.resolve(addon, ctx)?;
    tracing::debug!(name = %addon.name, version = %artifact.version, "resolved artifact");

    let staging_base = ctx.cache_dir.join("staging");
    fs::create_dir_all(&staging_base)?;

    let n = STAGING_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let staging_dir = staging_base.join(format!("{}_{pid}_{n:06}", addon.name));
    fs::create_dir_all(&staging_dir)?;

    let result = install_inner(provider, addon, ctx, lock, &artifact, &staging_dir);

    // Always clean up staging, even on failure.
    if let Err(e) = fs::remove_dir_all(&staging_dir) {
        tracing::debug!(error = %e, path = %staging_dir.display(), "failed to clean staging dir");
    }

    result
}

/// Removes an installed addon by looking up its recorded directories in the lock.
///
/// Each directory listed in `lock` for `addon_name` + `ctx.flavor` is removed
/// from `ctx.addons_path`. The lock entry is then deleted.
pub fn remove(addon_name: &str, ctx: &InstallContext, lock: &mut Lock) -> Result<()> {
    let pos = lock
        .addon
        .iter()
        .position(|a| a.name == addon_name && a.flavor == ctx.flavor)
        .ok_or_else(|| crate::Error::AddonNotInLock {
            name: addon_name.to_owned(),
        })?;

    let entry = lock.addon.remove(pos);
    tracing::debug!(name = %addon_name, dirs = ?entry.installed_dirs, "removing addon dirs");

    crate::fs::remove_addon_dirs(&entry.installed_dirs, &ctx.addons_path)?;

    lock.generated_at = Utc::now();
    Ok(())
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn install_inner(
    provider: &dyn Provider,
    addon: &ManifestAddon,
    ctx: &InstallContext,
    lock: &mut Lock,
    artifact: &crate::providers::ResolvedArtifact,
    staging_dir: &Path,
) -> Result<()> {
    let zip_path = staging_dir.join("addon.zip");
    provider.download(artifact, &zip_path)?;
    tracing::debug!(path = %zip_path.display(), "downloaded");

    let extract_dir = staging_dir.join("extracted");
    fs::create_dir_all(&extract_dir)?;
    let addon_dirs = crate::fs::extract_addon_zip(&zip_path, &extract_dir)?;

    if addon_dirs.is_empty() {
        return Err(crate::Error::NoInstallableDirs {
            name: addon.name.clone(),
        });
    }

    let installed_dirs = crate::fs::install_addon_dirs(&addon_dirs, &ctx.addons_path)?;

    let flavor = ctx.flavor.clone();
    let channel = ctx.channel.clone();

    // Replace existing lock entry for this addon+flavor (covers updates).
    lock.addon
        .retain(|a| !(a.name == addon.name && a.flavor == flavor));

    lock.addon.push(LockedAddon {
        name: addon.name.clone(),
        provider: addon.provider.clone(),
        flavor,
        channel,
        project_id: addon.project_id,
        resolved_version: artifact.version.clone(),
        resolved_id: artifact.id.clone(),
        download_url: artifact.url.clone(),
        sha256: artifact.sha256.clone(),
        installed_dirs,
        installed_at: Utc::now(),
    });
    lock.generated_at = Utc::now();

    Ok(())
}
