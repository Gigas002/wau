use super::*;

fn progress(current: u64, total: u64) -> Progress {
    Progress {
        kind: ProgressKind::Generic,
        label: "test".to_owned(),
        current,
        total: Some(total),
    }
}

#[test]
fn update_and_snapshot_round_trip() {
    let bus = ProgressBus::new();
    let id = bus.next_id();
    bus.update(id, Some(progress(1, 10)));
    let snapshot = bus.snapshot();
    assert_eq!(snapshot.get(&id), Some(&progress(1, 10)));
}

#[test]
fn update_none_removes_entry() {
    let bus = ProgressBus::new();
    let id = bus.next_id();
    bus.update(id, Some(progress(1, 10)));
    bus.update(id, None);
    assert!(bus.snapshot().is_empty());
}

#[test]
fn ids_are_unique_and_increasing() {
    let bus = ProgressBus::new();
    let a = bus.next_id();
    let b = bus.next_id();
    assert_ne!(a, b);
}

#[tokio::test]
async fn subscriber_observes_updates() {
    let bus = ProgressBus::new();
    let mut rx = bus.subscribe();
    let id = bus.next_id();
    bus.update(id, Some(progress(1, 10)));

    rx.changed().await.unwrap();
    let group = rx.borrow_and_update();
    assert_eq!(group.get(&id), Some(&progress(1, 10)));
}

#[tokio::test]
async fn incrementing_tracker_clears_entry_once_batch_completes() {
    let bus = ProgressBus::new();
    let tracker = IncrementingTracker::new(&bus, 2, "Resolving").unwrap();

    tracker.track(async { 1 }).await;
    assert_eq!(bus.snapshot().len(), 1);

    tracker.track(async { 2 }).await;
    assert!(bus.snapshot().is_empty());
}

#[test]
fn incrementing_tracker_none_for_empty_batch() {
    let bus = ProgressBus::new();
    assert!(IncrementingTracker::new(&bus, 0, "Nothing").is_none());
}

#[tokio::test]
async fn incrementing_tracker_reports_current_count_mid_batch() {
    let bus = ProgressBus::new();
    let tracker = IncrementingTracker::new(&bus, 3, "Resolving").unwrap();

    tracker.track(async {}).await;
    let snapshot = bus.snapshot();
    let entry = snapshot.values().next().unwrap();
    assert_eq!(entry.current, 1);
    assert_eq!(entry.total, Some(3));
}
