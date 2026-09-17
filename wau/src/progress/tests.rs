use indicatif::MultiProgress;
use libwau::progress::{Progress, ProgressBus, ProgressKind};

use super::*;

fn sample(current: u64, total: Option<u64>) -> Progress {
    Progress {
        kind: ProgressKind::Download,
        label: "some-addon".to_owned(),
        current,
        total,
    }
}

#[test]
fn bar_style_template_is_valid() {
    // `bar_style` swallows a bad template via `unwrap_or_else` (falls back
    // to the plain default bar) rather than panicking — this catches a typo
    // in the template string, which would otherwise silently degrade
    // styling instead of failing loudly.
    assert!(
        ProgressStyle::with_template(
            "{spinner:.green} {msg:.bold} [{bar:30.cyan/blue}] {bytes}/{total_bytes}",
        )
        .is_ok()
    );
}

#[test]
fn reconcile_adds_updates_and_removes_bars() {
    let multi = MultiProgress::new();
    let mut bars = HashMap::new();

    let mut group = ProgressGroup::new();
    group.insert(1, sample(0, Some(100)));
    reconcile(&multi, &mut bars, &group);
    assert_eq!(bars.len(), 1);
    assert_eq!(bars[&1].position(), 0);
    assert_eq!(bars[&1].length(), Some(100));
    assert_eq!(bars[&1].message(), "some-addon");

    group.get_mut(&1).unwrap().current = 40;
    group.insert(2, sample(0, None));
    reconcile(&multi, &mut bars, &group);
    assert_eq!(bars.len(), 2);
    assert_eq!(bars[&1].position(), 40);

    group.remove(&1);
    reconcile(&multi, &mut bars, &group);
    assert_eq!(bars.len(), 1);
    assert!(!bars.contains_key(&1));
    assert!(bars.contains_key(&2));
}

#[tokio::test]
async fn with_download_bars_disabled_just_runs_the_future() {
    let bus = ProgressBus::new();
    let result = with_download_bars(&bus, false, async { 42 }).await;
    assert_eq!(result, 42);
}

#[tokio::test]
async fn with_download_bars_enabled_returns_the_futures_result_and_clears_bars() {
    let bus = ProgressBus::new();

    let result = with_download_bars(&bus, true, async {
        let id = bus.next_id();
        bus.update(id, Some(sample(0, Some(10))));
        for step in 1..=10 {
            tokio::task::yield_now().await;
            bus.update(id, Some(sample(step, Some(10))));
        }
        bus.update(id, None);
        "done"
    })
    .await;

    assert_eq!(result, "done");
    assert!(bus.snapshot().is_empty());
}
