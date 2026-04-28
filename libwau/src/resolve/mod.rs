//! Manifest filtering layer: determines which addons from a manifest need to
//! be installed or updated given the current lock state and install context.

use crate::{
    lock::Lock,
    manifest::{Manifest, ManifestAddon},
    model::Flavor,
};

#[cfg(test)]
mod tests;

/// The outcome of [`plan`]: the list of addons to install and the count of
/// those skipped because they are already locked and `update` is false.
pub struct ResolutionPlan<'a> {
    /// Addons that should be passed to the install pipeline.
    pub to_install: Vec<&'a ManifestAddon>,
    /// Addons that matched the flavor filter but were already locked and
    /// skipped because `update` was false.
    pub skipped: usize,
}

/// Determines which addons from `manifest` need to be installed or updated.
///
/// Rules:
/// - Addons whose `flavors` list does not include `flavor` are silently ignored
///   (not applicable to this install, not counted as skipped).
/// - Addons already present in the lock are skipped when `update` is false;
///   they are counted in [`ResolutionPlan::skipped`].
/// - All remaining addons are returned in [`ResolutionPlan::to_install`] in
///   manifest order.
pub fn plan<'a>(
    manifest: &'a Manifest,
    lock: &Lock,
    flavor: &Flavor,
    update: bool,
) -> ResolutionPlan<'a> {
    let mut to_install = Vec::new();
    let mut skipped = 0;

    for addon in &manifest.addon {
        if let Some(flavors) = &addon.flavors
            && !flavors.contains(flavor)
        {
            continue;
        }

        let already_locked = lock
            .addon
            .iter()
            .any(|a| a.name == addon.name && a.flavor == *flavor);

        if already_locked && !update {
            skipped += 1;
            continue;
        }

        to_install.push(addon);
    }

    ResolutionPlan {
        to_install,
        skipped,
    }
}
