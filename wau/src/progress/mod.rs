//! Terminal rendering for `libwau::progress`: a multi-bar `indicatif`
//! display for in-flight downloads, live for the duration of one command's
//! operation.

use std::{collections::HashMap, future::Future};

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use libwau::progress::{ProgressBus, ProgressGroup};

#[cfg(test)]
mod tests;

/// Runs `fut` to completion, rendering the download progress reported
/// through `bus` as a multi-bar terminal display while `enabled` (see
/// [`crate::style::color_enabled`]) — piped/redirected output passes `fut`
/// straight through instead, since there's no terminal to draw bars into
/// and nothing should pollute captured output.
pub async fn with_download_bars<T>(
    bus: &ProgressBus,
    enabled: bool,
    fut: impl Future<Output = T>,
) -> T {
    if !enabled {
        return fut.await;
    }

    let mut rx = bus.subscribe();
    let multi = MultiProgress::new();
    let mut bars: HashMap<u64, ProgressBar> = HashMap::new();

    tokio::pin!(fut);
    loop {
        tokio::select! {
            result = &mut fut => {
                for (_, bar) in bars.drain() {
                    bar.finish_and_clear();
                }
                return result;
            }
            Ok(()) = rx.changed() => {
                let group = rx.borrow_and_update().clone();
                reconcile(&multi, &mut bars, &group);
            }
        }
    }
}

/// Adds a bar for every id newly present in `group`, updates every id still
/// there, and clears+drops any bar for an id that's gone (that download
/// finished or failed — [`libwau::pkg_archives::download_pkg_archive`]
/// always clears its own entry before returning).
fn reconcile(multi: &MultiProgress, bars: &mut HashMap<u64, ProgressBar>, group: &ProgressGroup) {
    bars.retain(|id, bar| {
        if group.contains_key(id) {
            true
        } else {
            bar.finish_and_clear();
            multi.remove(bar);
            false
        }
    });

    for (id, progress) in group {
        let bar = bars.entry(*id).or_insert_with(|| {
            let bar = multi.add(ProgressBar::new(progress.total.unwrap_or(0)));
            bar.set_style(bar_style());
            bar
        });
        if let Some(total) = progress.total {
            bar.set_length(total);
        }
        bar.set_position(progress.current);
        bar.set_message(progress.label.clone());
    }
}

fn bar_style() -> ProgressStyle {
    ProgressStyle::with_template(
        "{spinner:.green} {msg:.bold} [{bar:30.cyan/blue}] {bytes}/{total_bytes}",
    )
    .unwrap_or_else(|_| ProgressStyle::default_bar())
    .progress_chars("#>-")
}
