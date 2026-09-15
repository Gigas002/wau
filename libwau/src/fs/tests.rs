use std::fs;

use super::*;

#[test]
fn trash_moves_file_out_of_place() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("doomed.txt");
    fs::write(&file, b"bye").unwrap();

    let dest = trash(&file).unwrap();

    assert!(!file.exists());
    assert!(dest.exists());
    assert_eq!(fs::read(&dest).unwrap(), b"bye");
}

#[test]
fn trash_moves_directory_out_of_place() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("doomed_dir");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("inner.txt"), b"contents").unwrap();

    let dest = trash(&target).unwrap();

    assert!(!target.exists());
    assert!(dest.is_dir());
    assert_eq!(fs::read(dest.join("inner.txt")).unwrap(), b"contents");
}

#[test]
fn trash_destinations_are_unique_per_call() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.txt");
    let b = dir.path().join("b.txt");
    fs::write(&a, b"a").unwrap();
    fs::write(&b, b"b").unwrap();

    let dest_a = trash(&a).unwrap();
    let dest_b = trash(&b).unwrap();

    assert_ne!(dest_a.parent(), dest_b.parent());
}
