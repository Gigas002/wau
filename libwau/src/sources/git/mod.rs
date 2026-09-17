//! Git-repository addon source: builds addons from a locally-authored
//! `addbuild.toml` (a PKGBUILD-like recipe) plus a build script — modeled on
//! how `paru`/`yay` handle AUR `-git` packages.
//!
//! Everything (clone/update, version computation, running the build script,
//! and zipping the result) happens inside [`GitResolver::resolve_one_impl`];
//! the returned `PkgCandidate::download_url` is a `file://` URI pointing at
//! the zip it just built, so the rest of the install/update pipeline
//! (`pkg_management`, `pkg_archives`) needs no changes at all — a
//! version-specific/immutable download URL is exactly what that pipeline
//! already expects from every source.

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::{
    http::HttpClient,
    model::{ChangelogFormat, Defn, Flavour, SourceMetadata, Strategy},
    pkg_archives::{ArchiveError, is_safe_folder_name},
    results::{AnyOutcome, InternalError, ManagerError},
    sources::{PkgCandidate, Resolver, percent_encode},
};

/// On-disk shape of `<addname>/addbuild.toml`.
#[derive(Debug, Deserialize)]
struct AddBuild {
    addname: String,
    url: String,
    #[serde(default)]
    makedepends: Vec<String>,
    #[serde(default)]
    flavors: Option<Vec<String>>,
    #[serde(default)]
    depends: Vec<String>,
    build: String,
}

pub struct GitResolver {
    /// `<config-dir>/addbuilds` — one subdirectory per addon, each holding
    /// an `addbuild.toml` and its build script.
    addbuilds_dir: PathBuf,
    /// `<cache-dir>/git` — git checkouts (`git-src/<addname>`) and built,
    /// version-named zips (`git-build/<addname>/<version>.zip`).
    cache_dir: PathBuf,
}

impl GitResolver {
    pub fn new(addbuilds_dir: PathBuf, cache_dir: PathBuf) -> Self {
        Self {
            addbuilds_dir,
            cache_dir,
        }
    }

    fn addbuild_dir(&self, addname: &str) -> PathBuf {
        self.addbuilds_dir.join(addname)
    }

    fn src_dir(&self, addname: &str) -> PathBuf {
        self.cache_dir.join("git-src").join(addname)
    }

    fn build_dir(&self, addname: &str) -> PathBuf {
        self.cache_dir.join("git-build").join(addname)
    }
}

#[async_trait::async_trait]
impl Resolver for GitResolver {
    fn metadata(&self) -> SourceMetadata {
        SourceMetadata {
            id: "git",
            name: "Git",
            strategies: &[Strategy::AnyFlavour, Strategy::VersionEq],
            changelog_format: ChangelogFormat::Raw,
            addon_toc_key: None,
        }
    }

    async fn resolve_one_impl(
        &self,
        _http: &HttpClient,
        flavour: Flavour,
        defn: &Defn,
    ) -> AnyOutcome<PkgCandidate> {
        let addname = defn.alias.as_str();
        if !is_safe_folder_name(addname) {
            return Err(ManagerError::PkgNonexistent.into());
        }

        let addbuild = load_addbuild(&self.addbuild_dir(addname), addname).await?;
        check_flavour(&addbuild, flavour, defn)?;
        check_makedepends(&addbuild.makedepends).await?;
        check_depends(&addbuild.depends)?;

        let src_dir = self.src_dir(addname);
        clone_or_update(&addbuild.url, &src_dir).await?;

        if let Some(pinned) = &defn.strategies.version_eq {
            let hash = parse_pinned_hash(pinned).ok_or_else(|| files_not_matching(defn))?;
            checkout_rev(&src_dir, hash)
                .await
                .map_err(|_| files_not_matching(defn))?;
        }

        let version = git_version_string(&src_dir, "HEAD").await?;
        let date_published = git_commit_date(&src_dir, "HEAD").await?;

        let build_dir = self.build_dir(addname);
        let zip_path = build_dir.join(format!("{version}.zip"));
        if !zip_path.is_file() {
            build_and_package(
                &self.addbuild_dir(addname),
                &addbuild.build,
                &src_dir,
                &build_dir,
                addname,
                &version,
                &zip_path,
            )
            .await?;
        }

        let log = git_log_text(&src_dir, "HEAD").await.unwrap_or_default();
        let changelog_url = format!("data:,{}", percent_encode(&log));

        let download_url = url::Url::from_file_path(&zip_path)
            .map_err(|_| {
                InternalError::new(format!("unrepresentable path: {}", zip_path.display()))
            })?
            .to_string();

        Ok(PkgCandidate {
            id: addname.to_owned(),
            slug: addname.to_owned(),
            name: addname.to_owned(),
            description: String::new(),
            url: addbuild.url,
            download_url,
            date_published,
            version,
            changelog_url,
            deps: addbuild.depends,
        })
    }
}

fn files_not_matching(defn: &Defn) -> crate::results::Failure {
    ManagerError::PkgFilesNotMatching {
        strategies: defn.strategies.clone(),
    }
    .into()
}

// ============================================================================
// addbuild.toml
// ============================================================================

async fn load_addbuild(addbuild_dir: &Path, addname: &str) -> AnyOutcome<AddBuild> {
    let path = addbuild_dir.join("addbuild.toml");
    let contents = match tokio::fs::read_to_string(&path).await {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(ManagerError::PkgNonexistent.into());
        }
        Err(e) => return Err(InternalError::new(e).into()),
    };
    let addbuild: AddBuild = toml::from_str(&contents).map_err(InternalError::new)?;
    if addbuild.addname != addname {
        return Err(InternalError::new(format!(
            "{}: addname ({:?}) does not match its directory name ({addname:?})",
            path.display(),
            addbuild.addname
        ))
        .into());
    }
    Ok(addbuild)
}

fn check_flavour(addbuild: &AddBuild, flavour: Flavour, defn: &Defn) -> AnyOutcome<()> {
    let Some(flavors) = &addbuild.flavors else {
        return Ok(());
    };
    if defn.strategies.any_flavour {
        return Ok(());
    }

    let mut parsed = Vec::with_capacity(flavors.len());
    for f in flavors {
        match Flavour::parse(f) {
            Some(fl) => parsed.push(fl),
            None => {
                return Err(
                    InternalError::new(format!("addbuild.toml: unknown flavour {f:?}")).into(),
                );
            }
        }
    }

    if parsed.contains(&flavour) {
        Ok(())
    } else {
        Err(files_not_matching(defn))
    }
}

async fn check_makedepends(deps: &[String]) -> AnyOutcome<()> {
    let mut missing = Vec::new();
    for dep in deps {
        let ok = tokio::process::Command::new("sh")
            .args(["-c", "command -v \"$1\" >/dev/null 2>&1", "sh", dep])
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok {
            missing.push(dep.clone());
        }
    }
    if missing.is_empty() {
        Ok(())
    } else {
        Err(InternalError::new(format!(
            "missing build dependencies: {}",
            missing.join(", ")
        ))
        .into())
    }
}

/// Validates that every `depends` entry parses as a `source:alias` addon URI
/// `wau` itself accepts (the same format `install`/`remove` take on the
/// command line, e.g. `wowi:1234`) — checked eagerly, before any network
/// activity, so a typo surfaces immediately rather than after a clone/build.
/// The entries themselves are carried through verbatim in
/// `PkgCandidate::deps`; `pkg_management::resolve_deps` is what actually
/// resolves/installs them alongside the git package, one level deep, same as
/// every other source's same-source dependency ids.
fn check_depends(depends: &[String]) -> AnyOutcome<()> {
    let malformed: Vec<&str> = depends
        .iter()
        .filter(|d| Defn::from_uri(d, &[], true).is_err())
        .map(String::as_str)
        .collect();
    if malformed.is_empty() {
        Ok(())
    } else {
        Err(InternalError::new(format!(
            "addbuild.toml: depends entries must be a valid `source:alias` addon URI: {}",
            malformed.join(", ")
        ))
        .into())
    }
}

/// Extracts the commit hash out of a previously-generated `r<count>.<hash>`
/// version string, for `Strategy::VersionEq` pinning.
fn parse_pinned_hash(pinned: &str) -> Option<&str> {
    let rest = pinned.strip_prefix('r')?;
    let (_count, hash) = rest.split_once('.')?;
    (!hash.is_empty()).then_some(hash)
}

// ============================================================================
// git plumbing
// ============================================================================

async fn run_git(cwd: &Path, args: &[&str]) -> AnyOutcome<()> {
    run_git_capture(cwd, args).await.map(|_| ())
}

async fn run_git_capture(cwd: &Path, args: &[&str]) -> AnyOutcome<String> {
    let output = tokio::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .await
        .map_err(InternalError::new)?;
    if !output.status.success() {
        return Err(InternalError::new(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ))
        .into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Clones `url` into `repo_dir` if it isn't already a checkout of that same
/// remote, else fetches and hard-resets to the remote's default branch tip.
/// Always a full clone/fetch (never `--depth`-shallow) — `rev-list --count`
/// must be monotonically increasing across runs, which shallow history would
/// break.
///
/// If `addbuild.toml`'s `url` is edited after a checkout already exists
/// (e.g. fixing a wrong URL), a plain `git fetch` would silently keep
/// pulling from the *old* remote — `origin` is never repointed by a bare
/// fetch. So the existing `origin` URL is checked first; a mismatch (or a
/// checkout too broken to even ask) forces a fresh clone instead of an
/// update.
async fn clone_or_update(url: &str, repo_dir: &Path) -> AnyOutcome<()> {
    let is_same_remote = repo_dir.join(".git").exists()
        && run_git_capture(repo_dir, &["remote", "get-url", "origin"])
            .await
            .is_ok_and(|current| current.trim() == url);

    if is_same_remote {
        run_git(repo_dir, &["fetch", "--quiet", "origin"]).await?;
        let head_ref = run_git_capture(
            repo_dir,
            &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
        )
        .await?;
        let head_ref = head_ref.trim();
        run_git(repo_dir, &["reset", "--quiet", "--hard", head_ref]).await?;
        run_git(repo_dir, &["clean", "--quiet", "-fdx"]).await?;
    } else {
        if let Some(parent) = repo_dir.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(InternalError::new)?;
        }
        // Defensive: a previous attempt (or a stale checkout of a since-changed
        // `url`) may have left a directory here.
        let _ = tokio::fs::remove_dir_all(repo_dir).await;
        let repo_dir_str = repo_dir.to_string_lossy().into_owned();
        run_git(
            repo_dir.parent().unwrap_or(Path::new(".")),
            &["clone", "--quiet", url, &repo_dir_str],
        )
        .await?;
    }
    Ok(())
}

async fn checkout_rev(repo_dir: &Path, rev: &str) -> AnyOutcome<()> {
    run_git(repo_dir, &["checkout", "--quiet", "--detach", rev]).await
}

async fn git_version_string(repo_dir: &Path, rev: &str) -> AnyOutcome<String> {
    let count = run_git_capture(repo_dir, &["rev-list", "--count", rev]).await?;
    let hash = run_git_capture(repo_dir, &["rev-parse", "--short", rev]).await?;
    Ok(format!("r{}.{}", count.trim(), hash.trim()))
}

async fn git_commit_date(repo_dir: &Path, rev: &str) -> AnyOutcome<DateTime<Utc>> {
    let s = run_git_capture(repo_dir, &["show", "-s", "--format=%cI", rev]).await?;
    DateTime::parse_from_rfc3339(s.trim())
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| InternalError::new(e).into())
}

async fn git_log_text(repo_dir: &Path, rev: &str) -> AnyOutcome<String> {
    run_git_capture(repo_dir, &["log", "--oneline", "-n", "10", rev]).await
}

// ============================================================================
// build + package
// ============================================================================

/// Runs the addon's build script (directly — its own shebang decides the
/// interpreter, it must be `chmod +x`) with `pkgdir` freshly emptied, then
/// zips `pkgdir`'s contents to `zip_path`.
#[allow(clippy::too_many_arguments)]
async fn build_and_package(
    addbuild_dir: &Path,
    build_script: &str,
    src_dir: &Path,
    build_dir: &Path,
    pkgname: &str,
    pkgver: &str,
    zip_path: &Path,
) -> AnyOutcome<()> {
    let script_path = addbuild_dir.join(build_script);
    ensure_executable(&script_path).await?;

    let pkgdir = build_dir.join("pkgdir");
    let _ = tokio::fs::remove_dir_all(&pkgdir).await;
    tokio::fs::create_dir_all(&pkgdir)
        .await
        .map_err(InternalError::new)?;

    let output = tokio::process::Command::new(&script_path)
        .current_dir(src_dir)
        .env("srcdir", src_dir)
        .env("pkgdir", &pkgdir)
        .env("startdir", addbuild_dir)
        .env("pkgname", pkgname)
        .env("pkgver", pkgver)
        .output()
        .await
        .map_err(InternalError::new)?;
    if !output.status.success() {
        return Err(InternalError::new(format!(
            "build script {} failed: {}",
            script_path.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ))
        .into());
    }

    zip_dir(&pkgdir, zip_path).map_err(InternalError::new)?;
    Ok(())
}

#[cfg(unix)]
async fn ensure_executable(script_path: &Path) -> AnyOutcome<()> {
    use std::os::unix::fs::PermissionsExt;
    let meta = tokio::fs::metadata(script_path).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            InternalError::new(format!("build script not found: {}", script_path.display()))
        } else {
            InternalError::new(e)
        }
    })?;
    if meta.permissions().mode() & 0o111 == 0 {
        return Err(InternalError::new(format!(
            "build script is not executable: {} (run `chmod +x` on it)",
            script_path.display()
        ))
        .into());
    }
    Ok(())
}

#[cfg(not(unix))]
async fn ensure_executable(script_path: &Path) -> AnyOutcome<()> {
    if !tokio::fs::metadata(script_path)
        .await
        .map(|m| m.is_file())
        .unwrap_or(false)
    {
        return Err(InternalError::new(format!(
            "build script not found: {}",
            script_path.display()
        ))
        .into());
    }
    Ok(())
}

fn zip_dir(pkgdir: &Path, dest_zip: &Path) -> Result<(), ArchiveError> {
    if let Some(parent) = dest_zip.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = std::fs::File::create(dest_zip)?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    add_dir_recursive(&mut zip, pkgdir, Path::new(""), options)?;
    zip.finish()?;
    Ok(())
}

fn add_dir_recursive(
    zip: &mut zip::ZipWriter<std::fs::File>,
    base: &Path,
    rel: &Path,
    options: zip::write::SimpleFileOptions,
) -> Result<(), ArchiveError> {
    for entry in std::fs::read_dir(base.join(rel))? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let rel_path = rel.join(entry.file_name());
        let rel_str = rel_path.to_string_lossy().replace('\\', "/");

        if file_type.is_dir() {
            zip.add_directory(rel_str, options)?;
            add_dir_recursive(zip, base, &rel_path, options)?;
        } else if file_type.is_file() {
            zip.start_file(rel_str, options)?;
            let mut f = std::fs::File::open(entry.path())?;
            std::io::copy(&mut f, zip)?;
        }
    }
    Ok(())
}
