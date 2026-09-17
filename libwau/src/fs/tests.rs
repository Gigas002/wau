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

#[test]
fn copy_dir_recursive_preserves_nested_structure() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(src.join("nested")).unwrap();
    fs::write(src.join("top.txt"), b"top").unwrap();
    fs::write(src.join("nested").join("inner.txt"), b"inner").unwrap();

    let dest = dir.path().join("dest");
    copy_dir_recursive(&src, &dest).unwrap();

    assert_eq!(fs::read(dest.join("top.txt")).unwrap(), b"top");
    assert_eq!(
        fs::read(dest.join("nested").join("inner.txt")).unwrap(),
        b"inner"
    );
    // Source is untouched: copy, not move.
    assert!(src.join("top.txt").exists());
}

#[test]
fn is_within_true_for_a_descendant() {
    let dir = tempfile::tempdir().unwrap();
    let child = dir.path().join("child");
    fs::create_dir(&child).unwrap();

    assert!(is_within(dir.path(), &child));
}

#[test]
fn is_within_false_for_the_root_itself() {
    let dir = tempfile::tempdir().unwrap();
    assert!(!is_within(dir.path(), dir.path()));
}

#[test]
fn is_within_false_for_the_parent_of_root() {
    let dir = tempfile::tempdir().unwrap();
    let child = dir.path().join("child");
    fs::create_dir(&child).unwrap();

    // `dir` is the parent of `child` — must not be "within" `child`.
    assert!(!is_within(&child, dir.path()));
}
