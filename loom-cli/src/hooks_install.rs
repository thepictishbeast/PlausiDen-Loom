//! `loom hooks install` subcommand — install a pre-commit hook into
//! a target git repository that runs `loom validate --input <cms>`
//! before every commit.
//!
//! Extracted from `main.rs` as part of the bloat-reduction work
//! (Loom issue #3). Body kept byte-for-byte identical to the
//! pre-extraction implementation; visibility on `cmd_hooks_install`
//! was widened from module-private to `pub` so the entrypoint can
//! call it through the `mod hooks_install;` boundary.

/// `loom hooks install` writes this script as
/// `<target>/.git/hooks/pre-commit`. The script is intentionally
/// shell-portable (POSIX sh, no bash-isms) so it runs anywhere
/// git is installed. It looks up the loom binary via PATH so
/// authors can rebuild loom independently of the hook script
/// itself; if loom isn't on PATH, it warns + lets the commit
/// through (better than blocking commits because the dev forgot
/// to update PATH).
const PRE_COMMIT_HOOK_BODY: &str = r#"#!/bin/sh
# Installed by `loom hooks install`. Validates cms/*.json before
# every commit so broken schemas / URL validity never reach main.
#
# To skip (one commit only): git commit --no-verify
# To uninstall: rm .git/hooks/pre-commit
set -e
REPO_ROOT="$(git rev-parse --show-toplevel)"
if [ ! -d "$REPO_ROOT/cms" ]; then
  exit 0
fi
if ! command -v loom >/dev/null 2>&1; then
  echo "loom hooks: loom binary not on PATH; skipping cms/ validation"
  echo "  install: cargo install --path /path/to/PlausiDen-Loom/loom-cli"
  exit 0
fi
exec loom validate --input "$REPO_ROOT/cms"
"#;

pub fn cmd_hooks_install(target: &std::path::Path, force: bool) -> Result<bool, std::io::Error> {
    let git_dir = target.join(".git");
    if !git_dir.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!(".git not found at {} — not a git repo?", git_dir.display()),
        ));
    }
    let hooks_dir = git_dir.join("hooks");
    std::fs::create_dir_all(&hooks_dir)?;
    let hook_path = hooks_dir.join("pre-commit");
    if hook_path.exists() {
        // Two cases: it's our own hook (already current — idempotent
        // success) OR someone else's. Compare contents to decide.
        let existing = std::fs::read_to_string(&hook_path).unwrap_or_default();
        if existing == PRE_COMMIT_HOOK_BODY {
            // Already current. Nothing to do.
            println!(
                "  ok     pre-commit hook already current at {}",
                hook_path.display()
            );
            return Ok(false);
        }
        if !force {
            return Ok(true); // signal Conflict to caller
        }
    }
    std::fs::write(&hook_path, PRE_COMMIT_HOOK_BODY)?;
    set_executable(&hook_path)?;
    println!(
        "  ok     pre-commit hook installed at {}",
        hook_path.display()
    );
    Ok(false)
}

#[cfg(unix)]
fn set_executable(path: &std::path::Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms)
}

#[cfg(not(unix))]
fn set_executable(_path: &std::path::Path) -> std::io::Result<()> {
    // Non-Unix: git on Windows runs hooks via msys/git-bash which
    // honors execute via shebang; no chmod needed.
    Ok(())
}

#[cfg(test)]
mod cmd_hooks_install_tests {
    use super::*;

    fn unique(label: &str) -> std::path::PathBuf {
        let pid = std::process::id();
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        std::env::temp_dir().join(format!("loom-hooks-{label}-{pid}-{n}"))
    }

    fn fake_repo(label: &str) -> std::path::PathBuf {
        let dir = unique(label);
        std::fs::create_dir_all(dir.join(".git/hooks")).expect("mkdir");
        dir
    }

    #[test]
    fn errs_on_non_git_dir() {
        let dir = unique("not-git");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let r = cmd_hooks_install(&dir, false);
        assert!(r.is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn writes_hook_when_absent() {
        let repo = fake_repo("fresh");
        let conflict = cmd_hooks_install(&repo, false).expect("ok");
        assert!(!conflict);
        let hook = repo.join(".git/hooks/pre-commit");
        assert!(hook.exists());
        let body = std::fs::read_to_string(&hook).expect("read");
        assert!(body.contains("loom validate --input"));
        assert!(body.starts_with("#!/bin/sh"));
        let _ = std::fs::remove_dir_all(&repo);
    }

    #[test]
    #[cfg(unix)]
    fn hook_is_executable() {
        use std::os::unix::fs::PermissionsExt as _;
        let repo = fake_repo("perms");
        cmd_hooks_install(&repo, false).expect("ok");
        let mode = std::fs::metadata(repo.join(".git/hooks/pre-commit"))
            .expect("stat")
            .permissions()
            .mode();
        // Owner exec bit set.
        assert!(mode & 0o100 != 0, "hook not user-executable: {mode:o}");
        let _ = std::fs::remove_dir_all(&repo);
    }

    #[test]
    fn refuses_overwrite_without_force() {
        let repo = fake_repo("conflict");
        let hook_path = repo.join(".git/hooks/pre-commit");
        std::fs::write(&hook_path, "# someone else's hook\n").expect("write");
        let conflict = cmd_hooks_install(&repo, false).expect("ok");
        assert!(conflict);
        // Body unchanged.
        let body = std::fs::read_to_string(&hook_path).expect("read");
        assert!(body.contains("someone else's hook"));
        let _ = std::fs::remove_dir_all(&repo);
    }

    #[test]
    fn force_overwrites() {
        let repo = fake_repo("force");
        let hook_path = repo.join(".git/hooks/pre-commit");
        std::fs::write(&hook_path, "# old\n").expect("write");
        cmd_hooks_install(&repo, true).expect("ok");
        let body = std::fs::read_to_string(&hook_path).expect("read");
        assert!(body.contains("loom validate"));
        let _ = std::fs::remove_dir_all(&repo);
    }

    #[test]
    fn rerun_with_current_body_is_idempotent() {
        let repo = fake_repo("idempotent");
        // First install.
        cmd_hooks_install(&repo, false).expect("first ok");
        // Second invocation: body is already current → Ok(false), no error.
        let conflict = cmd_hooks_install(&repo, false).expect("second ok");
        assert!(!conflict);
        let _ = std::fs::remove_dir_all(&repo);
    }

    #[test]
    fn hook_skips_when_no_cms_dir() {
        let repo = fake_repo("no-cms");
        cmd_hooks_install(&repo, false).expect("ok");
        let body = std::fs::read_to_string(repo.join(".git/hooks/pre-commit")).expect("read");
        // The hook checks for cms/ existence and exits 0 if absent.
        assert!(body.contains("if [ ! -d \"$REPO_ROOT/cms\" ]"));
        assert!(body.contains("exit 0"));
        let _ = std::fs::remove_dir_all(&repo);
    }
}
