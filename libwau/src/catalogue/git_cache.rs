//! Local mirror of a single file tracked on a remote git branch, kept as a
//! shallow, sparse clone under the cache dir instead of refetched over plain
//! HTTP on every run.
//!
//! Only `filename` is ever materialised in the worktree (`--filter=blob:none`
//! plus non-cone sparse-checkout), so the cost stays close to fetching that
//! one file directly rather than the whole branch. A failed update (network
//! down, remote unreachable, …) falls back to whatever's already checked out
//! — a stale copy beats a hard failure — and only a first-ever clone with
//! nothing cached yet propagates the error.

use std::path::{Path, PathBuf};

use crate::results::{Failure, InternalError};

/// Clones (or updates) `branch` of `repo_url` into `cache_dir`, sparsely
/// checking out only `filename`, and returns the path to that file.
pub(super) async fn resolve(
    repo_url: &str,
    branch: &str,
    cache_dir: &Path,
    filename: &str,
) -> Result<PathBuf, Failure> {
    let repo_dir = cache_dir.join("catalogue-data");
    let target = repo_dir.join(filename);
    let pattern = format!("/{filename}");

    if repo_dir.join(".git").exists() {
        if let Err(err) = update(&repo_dir, branch, &pattern).await {
            if target.is_file() {
                tracing::warn!("catalogue cache update failed, using stale copy: {err}");
            } else {
                return Err(err);
            }
        }
    } else {
        clone(repo_url, branch, &repo_dir, &pattern).await?;
    }

    if !target.is_file() {
        return Err(
            InternalError::new(format!("{filename} not found on {repo_url} ({branch})")).into(),
        );
    }
    Ok(target)
}

async fn clone(
    repo_url: &str,
    branch: &str,
    repo_dir: &Path,
    pattern: &str,
) -> Result<(), Failure> {
    let parent = repo_dir.parent().unwrap_or_else(|| Path::new("."));
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(InternalError::new)?;
    // Defensive: a previous attempt may have left a partial/corrupt directory.
    let _ = tokio::fs::remove_dir_all(repo_dir).await;

    let repo_dir_str = repo_dir.to_string_lossy().into_owned();
    run_git(
        parent,
        &[
            "clone",
            "--depth=1",
            "--filter=blob:none",
            "--no-checkout",
            "--branch",
            branch,
            repo_url,
            &repo_dir_str,
        ],
    )
    .await?;
    run_git(repo_dir, &["sparse-checkout", "init", "--no-cone"]).await?;
    run_git(repo_dir, &["sparse-checkout", "set", pattern]).await?;
    run_git(repo_dir, &["checkout", branch]).await
}

async fn update(repo_dir: &Path, branch: &str, pattern: &str) -> Result<(), Failure> {
    // Idempotent, and re-widens the checkout if `pattern` changed since the
    // last run (e.g. wau upgraded to a newer pinned catalogue version).
    run_git(repo_dir, &["sparse-checkout", "set", pattern]).await?;
    run_git(repo_dir, &["fetch", "--depth=1", "origin", branch]).await?;
    run_git(repo_dir, &["reset", "--hard", "FETCH_HEAD"]).await
}

async fn run_git(cwd: &Path, args: &[&str]) -> Result<(), Failure> {
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
    Ok(())
}
