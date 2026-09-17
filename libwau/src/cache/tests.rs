use super::*;

fn write(path: &Path, contents: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

#[test]
fn usage_reports_zero_for_a_never_used_cache_dir() {
    let dir = tempfile::tempdir().unwrap();
    let categories = usage(dir.path());
    assert_eq!(categories.len(), 5);
    assert!(categories.iter().all(|c| c.bytes == 0));
}

#[test]
fn usage_sums_bytes_per_category_including_nested_git_dirs() {
    let dir = tempfile::tempdir().unwrap();
    write(&dir.path().join("http/aaa.body"), &[0u8; 100]);
    write(&dir.path().join("http/aaa.meta"), &[0u8; 20]);
    write(
        &dir.path().join("catalogue-data/data/catalogue.json"),
        &[0u8; 50],
    );
    write(&dir.path().join("staging/download-1-2"), &[0u8; 10]);
    write(&dir.path().join("git/git-src/Foo/.git/HEAD"), &[0u8; 5]);
    write(
        &dir.path().join("git/git-build/Foo/r1.abc.zip"),
        &[0u8; 200],
    );

    let categories = usage(dir.path());
    let by_name = |name: &str| categories.iter().find(|c| c.name == name).unwrap().bytes;
    assert_eq!(by_name("http"), 120);
    assert_eq!(by_name("catalogue-data"), 50);
    assert_eq!(by_name("staging"), 10);
    assert_eq!(by_name("git-src"), 5);
    assert_eq!(by_name("git-build"), 200);
}

#[test]
fn clean_removes_every_category_but_leaves_the_cache_dir_itself() {
    let dir = tempfile::tempdir().unwrap();
    write(&dir.path().join("http/aaa.body"), &[0u8; 100]);
    write(&dir.path().join("git/git-src/Foo/.git/HEAD"), &[0u8; 5]);

    clean(dir.path()).unwrap();

    assert!(dir.path().is_dir());
    for category in usage(dir.path()) {
        assert!(!category.path.exists(), "{} still exists", category.name);
        assert_eq!(category.bytes, 0);
    }
}

#[test]
fn clean_on_a_never_used_cache_dir_is_a_no_op() {
    let dir = tempfile::tempdir().unwrap();
    clean(dir.path()).unwrap();
}
