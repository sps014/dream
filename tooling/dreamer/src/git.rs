//! Git selectors resolve once to immutable commits; cached checkouts are verified before reuse.
use crate::fetch::git_dir;
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct GitCheckout {
    pub path: PathBuf,
    pub commit: String,
}

fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git").arg("-C").arg(dir).args(args).output()?;
    if !output.status.success() {
        bail!(
            "git operation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

pub fn is_commit(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|c| c.is_ascii_hexdigit())
}

pub fn fetch_git_dependency(
    url: &str,
    selector: &str,
    pinned: Option<&str>,
) -> Result<GitCheckout> {
    fetch_at(&git_dir(), url, selector, pinned)
}

fn valid_checkout(path: &Path, url: &str, commit: &str) -> bool {
    path.is_dir()
        && git(path, &["rev-parse", "HEAD"]).is_ok_and(|head| head == commit)
        && git(path, &["remote", "get-url", "origin"]).is_ok_and(|origin| origin == url)
        && git(path, &["status", "--porcelain", "--untracked-files=all"])
            .is_ok_and(|status| status.is_empty())
}

fn fetch_at(root: &Path, url: &str, selector: &str, pinned: Option<&str>) -> Result<GitCheckout> {
    if url.is_empty() || url.starts_with('-') || selector.is_empty() || selector.starts_with('-') {
        bail!("invalid Git dependency URL or selector");
    }
    if pinned.is_some_and(|sha| !is_commit(sha)) {
        bail!("Git lock must contain an immutable commit SHA");
    }
    std::fs::create_dir_all(root)?;
    let source = crate::registry::checksum::sha256_of(url.as_bytes()).replace(':', "-");
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join(format!("{source}.lock")))?;
    lock.lock()?;
    if let Some(commit) = pinned {
        let path = root.join(format!("{source}-{commit}"));
        if valid_checkout(&path, url, commit) {
            return Ok(GitCheckout {
                path,
                commit: commit.to_string(),
            });
        }
    }
    let temporary = tempfile::tempdir_in(root)?;
    git(temporary.path(), &["init", "--quiet"])?;
    git(temporary.path(), &["remote", "add", "origin", url])?;
    git(
        temporary.path(),
        &[
            "fetch",
            "--quiet",
            "--depth=1",
            "origin",
            pinned.unwrap_or(selector),
        ],
    )
    .context("fetching exact Git dependency")?;
    let commit = git(temporary.path(), &["rev-parse", "FETCH_HEAD^{commit}"])?;
    if !is_commit(&commit) || pinned.is_some_and(|sha| sha != commit) {
        bail!("Git commit identity mismatch");
    }
    git(
        temporary.path(),
        &["checkout", "--quiet", "--detach", &commit],
    )?;
    let path = root.join(format!("{source}-{commit}"));
    if valid_checkout(&path, url, &commit) {
        return Ok(GitCheckout { path, commit });
    }
    crate::package_fs::remove_entry(&path)?;
    std::fs::rename(temporary.path(), &path)?;
    Ok(GitCheckout { path, commit })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn locks_keep_old_commits_when_a_branch_moves_and_repair_dirty_cache() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("origin");
        std::fs::create_dir(&repo).unwrap();
        git(&repo, &["init", "--quiet", "-b", "main"]).unwrap();
        git(&repo, &["config", "user.email", "test@example.invalid"]).unwrap();
        git(&repo, &["config", "user.name", "Test"]).unwrap();
        std::fs::write(repo.join("source"), "first").unwrap();
        git(&repo, &["add", "source"]).unwrap();
        git(&repo, &["commit", "--quiet", "-m", "first"]).unwrap();
        let cache = temp.path().join("cache");
        let url = repo.to_str().unwrap();
        let first = fetch_at(&cache, url, "main", None).unwrap();
        std::fs::write(repo.join("source"), "second").unwrap();
        git(&repo, &["commit", "--quiet", "-a", "-m", "second"]).unwrap();
        let locked = fetch_at(&cache, url, "main", Some(&first.commit)).unwrap();
        assert_eq!(
            std::fs::read_to_string(locked.path.join("source")).unwrap(),
            "first"
        );
        std::fs::write(locked.path.join("source"), "tampered").unwrap();
        let repaired = fetch_at(&cache, url, "main", Some(&first.commit)).unwrap();
        assert_eq!(
            std::fs::read_to_string(repaired.path.join("source")).unwrap(),
            "first"
        );
        let updated = fetch_at(&cache, url, "main", None).unwrap();
        assert_ne!(updated.commit, first.commit);
        assert_eq!(
            std::fs::read_to_string(updated.path.join("source")).unwrap(),
            "second"
        );
    }

    #[test]
    fn commits_are_full_immutable_identities() {
        assert!(is_commit(&"a".repeat(40)));
        assert!(is_commit(&"0".repeat(64)));
        for value in ["HEAD", "main", "v1.0", "abcdef", "../escape"] {
            assert!(!is_commit(value));
        }
    }
}
