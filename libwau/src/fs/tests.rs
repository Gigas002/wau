use std::fs;

use super::*;

#[test]
fn trash_moves_file_out_of_place() {
    let dir = tempfile::tempdir().unwrap();
    set_test_trash_root(&dir.path().join("trash"));
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
    set_test_trash_root(&dir.path().join("trash"));
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
    set_test_trash_root(&dir.path().join("trash"));
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
    set_test_trash_root(&dir.path().join("trash"));
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
    set_test_trash_root(&dir.path().join("trash"));
    let child = dir.path().join("child");
    fs::create_dir(&child).unwrap();

    assert!(is_within(dir.path(), &child));
}

#[test]
fn is_within_false_for_the_root_itself() {
    let dir = tempfile::tempdir().unwrap();
    set_test_trash_root(&dir.path().join("trash"));
    assert!(!is_within(dir.path(), dir.path()));
}

#[test]
fn is_within_false_for_the_parent_of_root() {
    let dir = tempfile::tempdir().unwrap();
    set_test_trash_root(&dir.path().join("trash"));
    let child = dir.path().join("child");
    fs::create_dir(&child).unwrap();

    // `dir` is the parent of `child` — must not be "within" `child`.
    assert!(!is_within(&child, dir.path()));
}

#[test]
fn remove_delete_removes_a_directory_without_trashing_it() {
    let dir = tempfile::tempdir().unwrap();
    // No trash root set: trashing would panic, so this proves nothing is trashed.
    let target = dir.path().join("doomed_dir");
    fs::create_dir_all(target.join("nested")).unwrap();
    fs::write(target.join("nested").join("inner.txt"), b"contents").unwrap();
    let file = dir.path().join("doomed.txt");
    fs::write(&file, b"bye").unwrap();

    remove(&target, Removal::Delete).unwrap();
    remove(&file, Removal::Delete).unwrap();

    assert!(!target.exists());
    assert!(!file.exists());
}

#[cfg(unix)]
#[test]
fn remove_delete_removes_a_symlink_not_its_target() {
    let dir = tempfile::tempdir().unwrap();
    let real = dir.path().join("real");
    fs::create_dir(&real).unwrap();
    fs::write(real.join("keep.txt"), b"keep").unwrap();
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();

    remove(&link, Removal::Delete).unwrap();

    assert!(fs::symlink_metadata(&link).is_err());
    assert_eq!(fs::read(real.join("keep.txt")).unwrap(), b"keep");
}

#[test]
fn remove_trash_moves_into_the_trash() {
    let dir = tempfile::tempdir().unwrap();
    let trash_root = dir.path().join("trash");
    set_test_trash_root(&trash_root);
    let file = dir.path().join("doomed.txt");
    fs::write(&file, b"bye").unwrap();

    remove(&file, Removal::Trash).unwrap();

    assert!(!file.exists());
    assert_eq!(fs::read_dir(&trash_root).unwrap().count(), 1);
}
