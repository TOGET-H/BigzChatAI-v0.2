use super::*;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use tempfile::TempDir;

#[test]
fn macos_git_requirement_accepts_compatible_versions_and_rejects_old_or_unknown() {
    for version in [
        "git version 2.51.0",
        "git version 2.52.1 (Apple Git-200)",
        "git version 3.0.0",
    ] {
        assert!(compatible_git_version(version));
    }
    for version in [
        "git version 2.50.0 (Apple Git-160)",
        "git version 1.99.0",
        "git version 2.x.0",
        "unknown",
        "",
    ] {
        assert!(!compatible_git_version(version));
    }
}

// Replacing a directory with a file fails on both Unix and Windows. File
// readonly flags alone do not stop an atomic rename on Unix.
fn block_file_replacement(path: &Path) {
    fs::rename(path, path.with_extension("blocked-original")).unwrap();
    fs::create_dir(path).unwrap();
}
fn restore_blocked_file(path: &Path) {
    fs::remove_dir(path).unwrap();
    fs::rename(path.with_extension("blocked-original"), path).unwrap();
}

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}
struct Fixture {
    _temp: TempDir,
    root: PathBuf,
    peer: PathBuf,
    remote: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let remote = temp.path().join("remote.git");
        let root = temp.path().join("work");
        let peer = temp.path().join("peer");
        git(
            temp.path(),
            &[
                "init",
                "--bare",
                "--initial-branch=main",
                remote.to_str().unwrap(),
            ],
        );
        git(
            temp.path(),
            &["clone", remote.to_str().unwrap(), root.to_str().unwrap()],
        );
        for name in [&root] {
            git(name, &["config", "core.autocrlf", "false"]);
            git(name, &["config", "user.name", "Test Author"]);
            git(name, &["config", "user.email", "test@example.invalid"]);
        }
        for role in Role::all() {
            fs::create_dir(root.join(role.as_str())).unwrap();
            fs::write(
                root.join(role.as_str()).join("file.txt"),
                format!("{} baseline\n", role.as_str()),
            )
            .unwrap();
        }
        git(&root, &["add", "."]);
        git(&root, &["commit", "-m", "baseline"]);
        git(&root, &["push", "origin", "main"]);
        git(
            temp.path(),
            &["clone", remote.to_str().unwrap(), peer.to_str().unwrap()],
        );
        git(&peer, &["config", "core.autocrlf", "false"]);
        git(&peer, &["config", "user.name", "Peer Author"]);
        git(&peer, &["config", "user.email", "peer@example.invalid"]);
        Self {
            _temp: temp,
            root,
            peer,
            remote,
        }
    }
    fn project(&self) -> Project {
        Project {
            id: "fixture".into(),
            name: "Fixture".into(),
            root: self.root.to_string_lossy().into(),
            repository: "owner/repo".into(),
            remote_url: "https://github.com/owner/repo.git".into(),
            branch: "main".into(),
            role_paths: crate::model::default_role_paths(),
            monitor_enabled: true,
        }
    }
    fn ws(&self) -> GitWorkspace {
        GitWorkspace::new(self.project()).with_test_transport(self.remote.to_string_lossy().into())
    }
    fn write(&self, path: &str, text: &str) {
        let path = self.root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    fn input(&self, preview: &ChangePreview) -> PublishInput {
        PublishInput {
            project_id: "fixture".into(),
            role: preview.role,
            kind: "fix".into(),
            module: "商品页".into(),
            summary: "修复筛选".into(),
            fingerprint: preview.fingerprint.clone(),
        }
    }
    fn peer_commit(&self, text: &str) -> String {
        fs::write(self.peer.join("backend/file.txt"), text).unwrap();
        git(&self.peer, &["add", "."]);
        git(&self.peer, &["commit", "-m", "external update"]);
        git(&self.peer, &["push", "origin", "main"]);
        git(&self.peer, &["rev-parse", "HEAD"])
    }
}
#[test]
fn role_publish_preserves_other_roles_worktree_and_staging() {
    let f = Fixture::new();
    f.write("frontend/file.txt", "front change\n");
    f.write("backend/file.txt", "back staged\n");
    git(&f.root, &["add", "backend/file.txt"]);
    f.write("backend/file.txt", "back unstaged\n");
    let staged = git(&f.root, &["show", ":backend/file.txt"]);
    let ws = f.ws();
    let p = ws.preview(Role::Frontend).unwrap();
    assert_eq!(
        p.files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
        ["frontend/file.txt"]
    );
    assert_eq!(p.outside_count, 1);
    assert!(p.diff.contains("front change"));
    assert!(!p.diff.contains("back staged"));
    let r = ws.publish(&f.input(&p)).unwrap();
    assert!(r.pushed);
    assert_eq!(r.message, "fix(frontend): 商品页 - 修复筛选");
    assert_eq!(
        git(&f.remote, &["show", "main:backend/file.txt"]),
        "backend baseline"
    );
    assert_eq!(git(&f.root, &["show", ":backend/file.txt"]), staged);
    assert_eq!(
        fs::read_to_string(f.root.join("backend/file.txt")).unwrap(),
        "back unstaged\n"
    );
    assert_eq!(git(&f.root, &["diff", "--name-only"]), "backend/file.txt");
    assert_eq!(
        git(&f.root, &["diff", "--cached", "--name-only"]),
        "backend/file.txt"
    );
}
#[test]
fn role_publish_includes_new_and_deleted_paths() {
    let f = Fixture::new();
    f.write("frontend/new.txt", "new\n");
    fs::remove_file(f.root.join("frontend/file.txt")).unwrap();
    let ws = f.ws();
    let p = ws.preview(Role::Frontend).unwrap();
    assert_eq!(p.files.len(), 2);
    let r = ws.publish(&f.input(&p)).unwrap();
    assert!(r.pushed);
    assert_eq!(
        git(
            &f.remote,
            &["ls-tree", "-r", "--name-only", "main", "frontend"]
        ),
        "frontend/new.txt"
    );
    assert_eq!(git(&f.root, &["status", "--porcelain"]), "");
}
#[test]
fn rejects_cross_role_staged_rename() {
    let f = Fixture::new();
    git(&f.root, &["mv", "backend/file.txt", "frontend/moved.txt"]);
    assert_eq!(
        f.ws().preview(Role::Frontend).unwrap_err().code,
        "cross_role_rename"
    );
}
#[test]
fn rejects_stale_preview_after_file_or_head_change() {
    let f = Fixture::new();
    f.write("frontend/file.txt", "one\n");
    let ws = f.ws();
    let p = ws.preview(Role::Frontend).unwrap();
    f.write("frontend/file.txt", "two\n");
    assert_eq!(ws.publish(&f.input(&p)).unwrap_err().code, "stale_preview");
    let p = ws.preview(Role::Frontend).unwrap();
    git(
        &f.root,
        &["commit", "--allow-empty", "-m", "other local commit"],
    );
    assert_eq!(ws.publish(&f.input(&p)).unwrap_err().code, "stale_preview");
}
#[test]
fn refuses_sensitive_files_and_dependency_content() {
    for path in [
        "frontend/.env",
        "frontend/.env.production",
        "frontend/id_rsa",
        "frontend/key.pem",
        "frontend/node_modules/module.js",
    ] {
        let f = Fixture::new();
        f.write(path, "secret");
        assert_eq!(
            f.ws().preview(Role::Frontend).unwrap_err().code,
            "sensitive_path",
            "{path}"
        );
    }
    let f = Fixture::new();
    f.write("frontend/.env.example", "example");
    assert!(f.ws().preview(Role::Frontend).is_ok());
}
#[test]
fn refuses_binding_changes_and_non_github_discovery() {
    let f = Fixture::new();
    let ws = f.ws();
    git(&f.root, &["checkout", "-b", "other"]);
    assert_eq!(
        ws.preview(Role::Frontend).unwrap_err().code,
        "branch_mismatch"
    );
    git(&f.root, &["checkout", "main"]);
    git(
        &f.root,
        &["remote", "set-url", "origin", "https://evil.invalid/o/r"],
    );
    assert_eq!(
        ws.preview(Role::Frontend).unwrap_err().code,
        "remote_mismatch"
    );
    let f = Fixture::new();
    let input = AddProjectInput {
        root: f.root.to_string_lossy().into(),
        name: "Project".into(),
        branch: "main".into(),
        create_template: true,
    };
    assert_eq!(
        GitWorkspace::discover(&input).unwrap_err().code,
        "invalid_remote"
    );
    git(
        &f.root,
        &[
            "remote",
            "set-url",
            "origin",
            "git@github.com:owner/repo.git",
        ],
    );
    let p = GitWorkspace::discover(&input).unwrap();
    assert_eq!(p.repository, "owner/repo");
    assert!(f.root.join("aijimu.workspace.json").exists());
    fs::write(f.root.join("AGENTS.md"), "user instructions").unwrap();
    GitWorkspace::discover(&input).unwrap();
    assert_eq!(
        fs::read_to_string(f.root.join("AGENTS.md")).unwrap(),
        "user instructions"
    );
    fs::write(
        f.root.join("aijimu.workspace.json"),
        "{\"otherFormat\":true}",
    )
    .unwrap();
    assert_eq!(
        GitWorkspace::discover(&input).unwrap_err().code,
        "invalid_mapping"
    );
}
#[test]
fn refuses_unpublished_local_commits_and_remote_ahead() {
    let f = Fixture::new();
    git(&f.root, &["commit", "--allow-empty", "-m", "unpublished"]);
    f.write("frontend/file.txt", "front\n");
    let ws = f.ws();
    let p = ws.preview(Role::Frontend).unwrap();
    assert_eq!(ws.publish(&f.input(&p)).unwrap_err().code, "local_ahead");
    let f = Fixture::new();
    f.peer_commit("peer\n");
    f.write("frontend/file.txt", "front\n");
    let ws = f.ws();
    let p = ws.preview(Role::Frontend).unwrap();
    let head = p.head.clone();
    assert_eq!(ws.publish(&f.input(&p)).unwrap_err().code, "remote_ahead");
    assert_eq!(git(&f.root, &["rev-parse", "HEAD"]), head);
}
#[test]
fn push_failure_retains_receipt_and_retry_only_pushes_recorded_commit() {
    let f = Fixture::new();
    f.write("frontend/file.txt", "front\n");
    let remote = f.remote.clone();
    let offline = f.remote.with_extension("offline");
    let offline_hook = offline.clone();
    let ws = f
        .ws()
        .before_push(move || fs::rename(&remote, &offline_hook).unwrap());
    let p = ws.preview(Role::Frontend).unwrap();
    let r = ws.publish(&f.input(&p)).unwrap();
    assert!(!r.pushed);
    assert!(r.error.is_some());
    assert_eq!(git(&f.root, &["rev-parse", "HEAD"]), r.commit);
    fs::rename(offline, &f.remote).unwrap();
    let retry = f.ws().retry_push(&r).unwrap();
    assert!(retry.pushed);
    assert_eq!(git(&f.remote, &["rev-parse", "main"]), r.commit);
    let mut forged = r.clone();
    forged.message = "forged".into();
    assert_eq!(
        f.ws().retry_push(&forged).unwrap_err().code,
        "invalid_receipt"
    );
}
#[test]
fn remote_push_race_and_changed_retry_baseline_are_rejected() {
    let f = Fixture::new();
    f.write("frontend/file.txt", "front\n");
    let peer = f.peer.clone();
    let ws = f.ws().before_push(move || {
        fs::write(peer.join("backend/file.txt"), "racing peer\n").unwrap();
        git(&peer, &["add", "."]);
        git(&peer, &["commit", "-m", "race"]);
        git(&peer, &["push", "origin", "main"]);
    });
    let p = ws.preview(Role::Frontend).unwrap();
    let r = ws.publish(&f.input(&p)).unwrap();
    assert!(!r.pushed);
    assert_eq!(f.ws().retry_push(&r).unwrap_err().code, "remote_changed");
    git(&f.root, &["commit", "--allow-empty", "-m", "after receipt"]);
    assert_eq!(f.ws().retry_push(&r).unwrap_err().code, "head_changed");
}
#[test]
fn clean_pull_fast_forwards_and_dirty_or_diverged_pull_preserves_files() {
    let f = Fixture::new();
    let remote = f.peer_commit("new remote\n");
    assert_eq!(f.ws().pull().unwrap(), remote);
    assert_eq!(
        fs::read_to_string(f.root.join("backend/file.txt")).unwrap(),
        "new remote\n"
    );
    let f = Fixture::new();
    f.write("frontend/file.txt", "local\n");
    let head = git(&f.root, &["rev-parse", "HEAD"]);
    f.peer_commit("remote\n");
    assert_eq!(f.ws().pull().unwrap_err().code, "dirty_workspace");
    assert_eq!(git(&f.root, &["rev-parse", "HEAD"]), head);
    assert_eq!(
        fs::read_to_string(f.root.join("frontend/file.txt")).unwrap(),
        "local\n"
    );
    let f = Fixture::new();
    git(&f.root, &["commit", "--allow-empty", "-m", "local"]);
    let head = git(&f.root, &["rev-parse", "HEAD"]);
    f.peer_commit("remote\n");
    assert_eq!(f.ws().pull().unwrap_err().code, "diverged");
    assert_eq!(git(&f.root, &["rev-parse", "HEAD"]), head);
}
#[test]
fn snapshots_baseline_all_new_commits_and_history_rewrite() {
    let f = Fixture::new();
    let ws = f.ws();
    let baseline = ws.fetch_updates(None).unwrap();
    assert!(baseline.commits.is_empty());
    assert!(!baseline.rewritten);
    f.peer_commit("one\n");
    let final_head = f.peer_commit("two\n");
    let snapshot = ws.fetch_updates(Some(&baseline.head)).unwrap();
    assert_eq!(snapshot.head, final_head);
    assert_eq!(snapshot.commits.len(), 2);
    assert_eq!(snapshot.commits[0].title, "external update");
    assert_eq!(snapshot.commits[0].author, "Peer Author");
    assert!(!snapshot.commits[0].committed_at.is_empty());
    git(&f.peer, &["reset", "--hard", &baseline.head]);
    git(&f.peer, &["commit", "--allow-empty", "-m", "rewritten"]);
    git(&f.peer, &["push", "--force", "origin", "main"]);
    let snapshot = ws.fetch_updates(Some(&final_head)).unwrap();
    assert!(snapshot.rewritten);
    assert!(snapshot.commits.is_empty());
}

#[test]
fn rejects_linked_worktrees_nonroots_and_link_escape() {
    let f = Fixture::new();
    git(
        &f.root,
        &[
            "remote",
            "set-url",
            "origin",
            "https://github.com/owner/repo.git",
        ],
    );
    let input = AddProjectInput {
        root: f.root.join("frontend").to_string_lossy().into(),
        name: "Project".into(),
        branch: "main".into(),
        create_template: false,
    };
    assert!(GitWorkspace::discover(&input).is_err());
    let linked = f._temp.path().join("linked");
    git(
        &f.root,
        &["worktree", "add", "-b", "linked", linked.to_str().unwrap()],
    );
    let input = AddProjectInput {
        root: linked.to_string_lossy().into(),
        branch: "linked".into(),
        ..input
    };
    assert_eq!(
        GitWorkspace::discover(&input).unwrap_err().code,
        "unsupported_worktree"
    );
    let f = Fixture::new();
    let outside = f._temp.path().join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("secret.txt"), "private").unwrap();
    #[cfg(windows)]
    {
        let script = format!(
            "New-Item -ItemType Junction -Path '{}' -Target '{}' | Out-Null",
            f.root.join("frontend/escape").display(),
            outside.display()
        );
        assert!(
            Command::new("powershell")
                .args(["-NoProfile", "-Command", &script])
                .status()
                .unwrap()
                .success()
        );
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, f.root.join("frontend/escape")).unwrap();
    assert_eq!(
        f.ws().preview(Role::Frontend).unwrap_err().code,
        "link_escape"
    );
}
#[test]
fn unstaged_cross_role_move_is_rejected() {
    let f = Fixture::new();
    fs::rename(
        f.root.join("backend/file.txt"),
        f.root.join("frontend/moved.txt"),
    )
    .unwrap();
    assert_eq!(
        f.ws().preview(Role::Frontend).unwrap_err().code,
        "cross_role_rename"
    );
}
#[test]
fn stale_preview_after_mapping_change_is_rejected() {
    let f = Fixture::new();
    f.write("frontend/file.txt", "one\n");
    let ws = f.ws();
    let p = ws.preview(Role::Frontend).unwrap();
    let mut role_paths = crate::model::default_role_paths();
    role_paths.insert(Role::Frontend, vec!["apps/web".into()]);
    fs::write(
        f.root.join("aijimu.workspace.json"),
        serde_json::to_vec(&Mapping {
            version: 1,
            role_paths,
        })
        .unwrap(),
    )
    .unwrap();
    assert_eq!(ws.publish(&f.input(&p)).unwrap_err().code, "stale_preview");
}
#[test]
fn role_publish_preserves_executable_mode() {
    let f = Fixture::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            f.root.join("frontend/file.txt"),
            fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }
    git(
        &f.root,
        &["update-index", "--chmod=+x", "frontend/file.txt"],
    );
    git(&f.root, &["commit", "-m", "executable baseline"]);
    git(&f.root, &["push", "origin", "main"]);
    f.write("frontend/file.txt", "changed executable\n");
    let ws = f.ws();
    let p = ws.preview(Role::Frontend).unwrap();
    ws.publish(&f.input(&p)).unwrap();
    assert!(git(&f.remote, &["ls-tree", "main", "frontend/file.txt"]).starts_with("100755"));
}
#[test]
fn refuses_unfinished_merge_operation() {
    let f = Fixture::new();
    git(&f.root, &["checkout", "-b", "conflicting"]);
    f.write("frontend/file.txt", "branch\n");
    git(&f.root, &["add", "."]);
    git(&f.root, &["commit", "-m", "branch change"]);
    git(&f.root, &["checkout", "main"]);
    f.write("frontend/file.txt", "main\n");
    git(&f.root, &["add", "."]);
    git(&f.root, &["commit", "-m", "main change"]);
    assert!(
        !Command::new("git")
            .args(["merge", "conflicting"])
            .current_dir(&f.root)
            .output()
            .unwrap()
            .status
            .success()
    );
    assert_eq!(
        f.ws().preview(Role::Frontend).unwrap_err().code,
        "unfinished_operation"
    );
}
#[test]
fn drains_large_diff_and_refuses_oversized_outputs() {
    let f = Fixture::new();
    f.write(
        "frontend/big.txt",
        &format!("{}\n", "line with payload\n".repeat(20000)),
    );
    let preview = f.ws().preview(Role::Frontend).unwrap();
    assert!(preview.diff.len() > 300000);
    f.write("frontend/huge.txt", &"x".repeat(LIMIT + 1));
    assert_eq!(
        f.ws().preview(Role::Frontend).unwrap_err().code,
        "file_limit"
    );
    git(&f.root, &["add", "frontend/huge.txt"]);
    git(&f.root, &["commit", "-m", "large fixture blob"]);
    assert_eq!(
        run(&f.root, &["show", "HEAD:frontend/huge.txt"], None, None)
            .unwrap_err()
            .code,
        "output_limit"
    );
}
#[test]
fn inherited_git_environment_is_removed() {
    let f = Fixture::new();
    f.write("frontend/file.txt", "actual change\n");
    let bogus = f._temp.path().join("not-a-repository");
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "git::tests::git_environment_child",
            "--nocapture",
        ])
        .env("AIJIMU_TEST_ROOT", &f.root)
        .env("AIJIMU_TEST_REMOTE", &f.remote)
        .env("GIT_DIR", &bogus)
        .env("GIT_WORK_TREE", &bogus)
        .env("GIT_INDEX_FILE", &bogus)
        .env("GIT_OBJECT_DIRECTORY", &bogus)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
}
#[test]
fn git_environment_child() {
    let Ok(root) = std::env::var("AIJIMU_TEST_ROOT") else {
        return;
    };
    let project = Project {
        id: "child".into(),
        name: "Child".into(),
        root,
        repository: "owner/repo".into(),
        remote_url: "https://github.com/owner/repo.git".into(),
        branch: "main".into(),
        role_paths: default_role_paths(),
        monitor_enabled: true,
    };
    let ws = GitWorkspace::new(project)
        .with_test_transport(std::env::var("AIJIMU_TEST_REMOTE").unwrap());
    assert_eq!(
        ws.preview(Role::Frontend).unwrap().files[0].path,
        "frontend/file.txt"
    );
}

#[test]
fn times_out_without_waiting_for_shell_descendant_pipes() {
    let f = Fixture::new();
    let started = Instant::now();
    let error = run_with_timeout(
        &f.root,
        &["-c", "alias.wait=!sleep 10", "wait"],
        None,
        None,
        Duration::from_millis(80),
    )
    .unwrap_err();
    assert_eq!(error.code, "git_timeout");
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[cfg(any(windows, unix))]
#[test]
fn timeout_terminates_git_descendants() {
    let f = Fixture::new();
    let error = run_with_timeout(
        &f.root,
        &[
            "-c",
            "alias.wait=!sleep 0.4; echo leaked > timeout-marker",
            "wait",
        ],
        None,
        None,
        Duration::from_millis(80),
    )
    .unwrap_err();
    assert_eq!(error.code, "git_timeout");
    thread::sleep(Duration::from_millis(650));
    assert!(
        !f.root.join("timeout-marker").exists(),
        "git shell descendant survived timeout"
    );
}

#[test]
fn pull_preserves_ignored_local_files_that_remote_would_overwrite() {
    let f = Fixture::new();
    f.write(".git/info/exclude", "frontend/local-only.txt\n");
    f.write("frontend/local-only.txt", "precious local\n");
    fs::write(f.peer.join("frontend/local-only.txt"), "remote version\n").unwrap();
    git(&f.peer, &["add", "."]);
    git(&f.peer, &["commit", "-m", "remote new file"]);
    git(&f.peer, &["push", "origin", "main"]);
    let head = git(&f.root, &["rev-parse", "HEAD"]);
    assert!(f.ws().pull().is_err());
    assert_eq!(git(&f.root, &["rev-parse", "HEAD"]), head);
    assert_eq!(
        fs::read_to_string(f.root.join("frontend/local-only.txt")).unwrap(),
        "precious local\n"
    );
}
#[test]
fn rejects_shallow_repository_instead_of_incomplete_history() {
    let f = Fixture::new();
    let shallow = f._temp.path().join("shallow");
    let remote = format!("file:///{}", f.remote.to_string_lossy().replace('\\', "/"));
    git(
        f._temp.path(),
        &["clone", "--depth=1", &remote, shallow.to_str().unwrap()],
    );
    git(
        &shallow,
        &[
            "remote",
            "set-url",
            "origin",
            "https://github.com/owner/repo.git",
        ],
    );
    let input = AddProjectInput {
        root: shallow.to_string_lossy().into(),
        name: "Shallow".into(),
        branch: "main".into(),
        create_template: false,
    };
    assert_eq!(
        GitWorkspace::discover(&input).unwrap_err().code,
        "unsupported_shallow"
    );
}
#[test]
fn first_role_delivery_to_empty_remote_has_no_parent() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("empty");
    let remote = temp.path().join("empty.git");
    git(
        temp.path(),
        &[
            "init",
            "--bare",
            "--initial-branch=main",
            remote.to_str().unwrap(),
        ],
    );
    git(
        temp.path(),
        &["clone", remote.to_str().unwrap(), root.to_str().unwrap()],
    );
    git(&root, &["config", "user.name", "Test Author"]);
    git(&root, &["config", "user.email", "test@example.invalid"]);
    git(&root, &["config", "core.autocrlf", "false"]);
    fs::create_dir(root.join("frontend")).unwrap();
    fs::write(root.join("frontend/new.txt"), "first file\n").unwrap();
    let project = Project {
        id: "empty".into(),
        name: "Empty".into(),
        root: root.to_string_lossy().into(),
        repository: "owner/repo".into(),
        remote_url: "https://github.com/owner/repo.git".into(),
        branch: "main".into(),
        role_paths: default_role_paths(),
        monitor_enabled: true,
    };
    let ws = GitWorkspace::new(project).with_test_transport(remote.to_string_lossy().into());
    let p = ws.preview(Role::Frontend).unwrap();
    assert!(p.head.is_empty());
    let r = ws
        .publish(&PublishInput {
            project_id: "empty".into(),
            role: Role::Frontend,
            kind: "feat".into(),
            module: "初始页面".into(),
            summary: "创建页面".into(),
            fingerprint: p.fingerprint,
        })
        .unwrap();
    assert!(r.pushed);
    assert!(r.parent.is_empty());
    assert_eq!(
        git(&remote, &["show", "main:frontend/new.txt"]),
        "first file"
    );
}

#[test]
fn binding_change_after_local_commit_returns_failed_receipt() {
    let f = Fixture::new();
    f.write("frontend/file.txt", "front\n");
    let root = f.root.clone();
    let ws = f.ws().before_push(move || {
        git(
            &root,
            &[
                "remote",
                "set-url",
                "origin",
                "https://evil.invalid/other/repo",
            ],
        );
    });
    let preview = ws.preview(Role::Frontend).unwrap();
    let receipt = ws.publish(&f.input(&preview)).unwrap();
    assert!(!receipt.pushed);
    assert!(receipt.error.unwrap().contains("remote_mismatch"));
    assert_eq!(git(&f.root, &["rev-parse", "HEAD"]), receipt.commit);
}
#[test]
fn edited_unstaged_cross_role_move_requires_manual_resolution() {
    let f = Fixture::new();
    fs::rename(
        f.root.join("backend/file.txt"),
        f.root.join("frontend/moved.txt"),
    )
    .unwrap();
    f.write(
        "frontend/moved.txt",
        "backend baseline\nchanged after move\n",
    );
    assert_eq!(
        f.ws().preview(Role::Frontend).unwrap_err().code,
        "cross_role_rename"
    );
}

#[test]
fn refuses_head_binding_race_without_mutating_branch_or_index() {
    let f = Fixture::new();
    let baseline = git(&f.root, &["rev-parse", "HEAD"]);
    git(&f.root, &["branch", "other"]);
    f.write("frontend/file.txt", "front\n");
    let original_index = git(&f.root, &["show", ":frontend/file.txt"]);
    let root = f.root.clone();
    let ws = f.ws().before_cas(move || {
        git(&root, &["symbolic-ref", "HEAD", "refs/heads/other"]);
    });
    let preview = ws.preview(Role::Frontend).unwrap();
    assert_eq!(
        ws.publish(&f.input(&preview)).unwrap_err().code,
        "head_changed"
    );
    assert_eq!(git(&f.root, &["rev-parse", "main"]), baseline);
    assert_eq!(git(&f.remote, &["rev-parse", "main"]), baseline);
    assert_eq!(
        git(&f.root, &["show", ":frontend/file.txt"]),
        original_index
    );
    assert_eq!(
        fs::read_to_string(f.root.join("frontend/file.txt")).unwrap(),
        "front\n"
    );
}

fn utf16_with_bom(value: &str) -> Vec<u8> {
    [
        vec![0xff, 0xfe],
        value.encode_utf16().flat_map(u16::to_le_bytes).collect(),
    ]
    .concat()
}
#[test]
fn working_tree_encoding_publishes_canonical_blob_and_native_checkout() {
    let f = Fixture::new();
    f.write(
        ".gitattributes",
        "frontend/encoded.txt text eol=lf working-tree-encoding=UTF-16\n",
    );
    fs::write(
        f.root.join("frontend/encoded.txt"),
        utf16_with_bom("编码基线\n"),
    )
    .unwrap();
    git(&f.root, &["add", ".gitattributes", "frontend/encoded.txt"]);
    git(&f.root, &["commit", "-m", "encoding baseline"]);
    // Establish a native, Git-consistent round trip before publishing via the app.
    fs::remove_file(f.root.join("frontend/encoded.txt")).unwrap();
    git(
        &f.root,
        &["checkout-index", "--force", "--", "frontend/encoded.txt"],
    );
    assert_eq!(git(&f.root, &["status", "--porcelain"]), "");
    git(&f.root, &["push", "origin", "main"]);
    let working_bytes = utf16_with_bom("编码更新\n");
    fs::write(f.root.join("frontend/encoded.txt"), &working_bytes).unwrap();
    let ws = f.ws();
    let preview = ws.preview(Role::Frontend).unwrap();
    let receipt = ws.publish(&f.input(&preview)).unwrap();
    assert!(receipt.pushed);
    assert_eq!(git(&f.root, &["status", "--porcelain"]), "");
    assert_eq!(
        git(&f.remote, &["show", "main:frontend/encoded.txt"]),
        "编码更新"
    );
    fs::remove_file(f.root.join("frontend/encoded.txt")).unwrap();
    git(
        &f.root,
        &["checkout-index", "--force", "--", "frontend/encoded.txt"],
    );
    let restored = fs::read(f.root.join("frontend/encoded.txt")).unwrap();
    let little_endian = match &restored[..2] {
        [0xff, 0xfe] => true,
        [0xfe, 0xff] => false,
        _ => panic!("native checkout did not restore UTF-16 BOM"),
    };
    let units: Vec<_> = restored[2..]
        .chunks_exact(2)
        .map(|pair| {
            if little_endian {
                u16::from_le_bytes([pair[0], pair[1]])
            } else {
                u16::from_be_bytes([pair[0], pair[1]])
            }
        })
        .collect();
    assert_eq!(
        String::from_utf16(&units).unwrap().replace("\r\n", "\n"),
        "编码更新\n"
    );
    assert_eq!(git(&f.root, &["status", "--porcelain"]), "");
}
#[test]
fn builtin_text_eol_and_ident_are_canonicalized() {
    let f = Fixture::new();
    f.write(".gitattributes", "frontend/file.txt text eol=crlf ident\n");
    f.write("frontend/file.txt", "baseline\r\n$Id$\r\n");
    git(&f.root, &["add", ".gitattributes", "frontend/file.txt"]);
    git(&f.root, &["commit", "-m", "builtin attributes baseline"]);
    git(&f.root, &["push", "origin", "main"]);
    f.write(
        "frontend/file.txt",
        "updated\r\n$Id: expanded working value $\r\n",
    );
    let ws = f.ws();
    let preview = ws.preview(Role::Frontend).unwrap();
    assert!(ws.publish(&f.input(&preview)).unwrap().pushed);
    assert_eq!(
        git(&f.remote, &["show", "main:frontend/file.txt"]),
        "updated\n$Id$"
    );
    assert_eq!(git(&f.root, &["status", "--porcelain"]), "");
}
#[test]
fn active_external_filter_is_rejected_without_execution() {
    let f = Fixture::new();
    f.write(".gitattributes", "frontend/file.txt filter=unsafe\n");
    git(&f.root, &["add", ".gitattributes"]);
    git(&f.root, &["commit", "-m", "filter attributes"]);
    git(&f.root, &["push", "origin", "main"]);
    git(
        &f.root,
        &[
            "config",
            "filter.unsafe.clean",
            "echo executed > filter-marker; cat",
        ],
    );
    f.write("frontend/file.txt", "modified\n");
    let head = git(&f.root, &["rev-parse", "HEAD"]);
    let result = f.ws().preview(Role::Frontend);
    assert!(
        !f.root.join("filter-marker").exists(),
        "external clean filter executed"
    );
    assert_eq!(result.unwrap_err().code, "external_filter");
    assert_eq!(git(&f.root, &["rev-parse", "HEAD"]), head);
}
#[test]
fn cat_file_corruption_is_error_not_rewritten_snapshot() {
    let f = Fixture::new();
    let previous = git(&f.root, &["rev-parse", "HEAD"]);
    f.peer_commit("normal remote update\n");
    let object = f
        .root
        .join(".git/objects")
        .join(&previous[..2])
        .join(&previous[2..]);
    let ws = f.ws().before_history(move || corrupt_loose_object(&object));
    assert_eq!(
        ws.fetch_updates(Some(&previous)).unwrap_err().code,
        "git_failed"
    );
}
#[test]
fn merge_base_execution_failure_is_error_not_rewritten_snapshot() {
    let f = Fixture::new();
    let baseline = git(&f.root, &["rev-parse", "HEAD"]);
    let tree = git(&f.root, &["rev-parse", "HEAD^{tree}"]);
    let side = git(
        &f.root,
        &["commit-tree", &tree, "-p", &baseline, "-m", "side history"],
    );
    f.peer_commit("normal remote update\n");
    let object = f
        .root
        .join(".git/objects")
        .join(&baseline[..2])
        .join(&baseline[2..]);
    let ws = f.ws().before_history(move || corrupt_loose_object(&object));
    assert_eq!(
        ws.fetch_updates(Some(&side)).unwrap_err().code,
        "git_failed"
    );
}
#[test]
fn cross_role_move_over_existing_destination_is_rejected_for_both_roles() {
    let f = Fixture::new();
    let head = git(&f.root, &["rev-parse", "HEAD"]);
    let index = fs::read(f.root.join(".git/index")).unwrap();
    fs::copy(
        f.root.join("backend/file.txt"),
        f.root.join("frontend/file.txt"),
    )
    .unwrap();
    fs::remove_file(f.root.join("backend/file.txt")).unwrap();
    for role in [Role::Frontend, Role::Backend] {
        assert_eq!(f.ws().preview(role).unwrap_err().code, "cross_role_rename");
    }
    assert_eq!(git(&f.root, &["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(f.root.join(".git/index")).unwrap(), index);
}

fn corrupt_loose_object(path: &Path) {
    let mut permissions = fs::metadata(path).unwrap().permissions();
    #[cfg(windows)]
    permissions.set_readonly(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        permissions.set_mode(0o600);
    }
    fs::set_permissions(path, permissions).unwrap();
    fs::write(path, b"corrupt loose object").unwrap();
}

#[test]
fn ambiguous_unset_filter_is_rejected_without_execution() {
    assert_ambiguous_filter_refused("unset");
}
#[test]
fn ambiguous_unspecified_filter_is_rejected_without_execution() {
    assert_ambiguous_filter_refused("unspecified");
}
fn assert_ambiguous_filter_refused(driver: &str) {
    let f = Fixture::new();
    f.write("frontend/file.txt", "base\n");
    f.write(
        ".gitattributes",
        &format!("frontend/file.txt filter={driver}\n"),
    );
    git(&f.root, &["add", ".gitattributes", "frontend/file.txt"]);
    git(&f.root, &["commit", "-m", "ambiguous filter baseline"]);
    git(&f.root, &["push", "origin", "main"]);
    // Equal lengths force native status to check content rather than only file size.
    f.write("frontend/file.txt", "newx\n");
    let ws = f.ws();
    let preview = ws.preview(Role::Frontend).unwrap();
    let head = git(&f.root, &["rev-parse", "HEAD"]);
    let index = fs::read(f.root.join(".git/index")).unwrap();
    git(
        &f.root,
        &[
            "config",
            &format!("filter.{driver}.clean"),
            "echo executed > filter-marker; cat",
        ],
    );
    let result = ws.preview(Role::Frontend);
    assert!(
        !f.root.join("filter-marker").exists(),
        "ambiguous {driver} clean filter executed during preview"
    );
    assert_eq!(result.unwrap_err().code, "external_filter");
    let result = ws.publish(&f.input(&preview));
    assert!(
        !f.root.join("filter-marker").exists(),
        "ambiguous {driver} clean filter executed during publish"
    );
    assert_eq!(result.unwrap_err().code, "external_filter");
    assert_eq!(git(&f.root, &["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(f.root.join(".git/index")).unwrap(), index);
}
#[test]
fn disabled_and_absent_filters_preserve_normal_publish() {
    for attributes in ["frontend/file.txt -filter\n", ""] {
        let f = Fixture::new();
        f.write("frontend/file.txt", "base\n");
        f.write(".gitattributes", attributes);
        git(&f.root, &["add", ".gitattributes", "frontend/file.txt"]);
        git(&f.root, &["commit", "-m", "inert filter baseline"]);
        git(&f.root, &["push", "origin", "main"]);
        git(
            &f.root,
            &[
                "config",
                "filter.unsafe.clean",
                "echo executed > filter-marker; cat",
            ],
        );
        f.write("frontend/file.txt", "newx\n");
        let ws = f.ws();
        let preview = ws.preview(Role::Frontend).unwrap();
        assert!(ws.publish(&f.input(&preview)).unwrap().pushed);
        assert!(!f.root.join("filter-marker").exists());
        assert_eq!(git(&f.root, &["status", "--porcelain"]), "");
        assert_eq!(git(&f.remote, &["show", "main:frontend/file.txt"]), "newx");
    }
}

#[test]
fn final_review_pull_refuses_clean_filter_before_status() {
    let f = Fixture::new();
    f.write(".gitattributes", "frontend/file.txt filter=marker\n");
    git(&f.root, &["add", ".gitattributes"]);
    git(&f.root, &["commit", "-m", "attributes"]);
    git(&f.root, &["push", "origin", "main"]);
    f.write("frontend/file.txt", "FRONTEND baseline\n");
    git(
        &f.root,
        &[
            "config",
            "filter.marker.clean",
            "echo ran > filter-marker; cat",
        ],
    );
    let head = git(&f.root, &["rev-parse", "HEAD"]);
    let bytes = fs::read(f.root.join("frontend/file.txt")).unwrap();
    let result = f.ws().pull();
    assert!(
        !f.root.join("filter-marker").exists(),
        "pull executed clean helper"
    );
    assert_eq!(result.unwrap_err().code, "external_filter");
    assert_eq!(git(&f.root, &["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(f.root.join("frontend/file.txt")).unwrap(), bytes);
}
#[test]
fn final_review_pull_refuses_incoming_filter_before_checkout() {
    for driver in ["smudge", "process"] {
        let f = Fixture::new();
        fs::write(
            f.peer.join(".gitattributes"),
            "frontend/file.txt filter=marker\n",
        )
        .unwrap();
        fs::write(f.peer.join("frontend/file.txt"), "incoming content\n").unwrap();
        git(&f.peer, &["add", "."]);
        git(&f.peer, &["commit", "-m", "incoming filter"]);
        git(&f.peer, &["push", "origin", "main"]);
        git(
            &f.root,
            &[
                "config",
                &format!("filter.marker.{driver}"),
                "echo ran > filter-marker; cat",
            ],
        );
        git(&f.root, &["config", "filter.marker.required", "true"]);
        let head = git(&f.root, &["rev-parse", "HEAD"]);
        let bytes = fs::read(f.root.join("frontend/file.txt")).unwrap();
        let result = f.ws().pull();
        assert!(
            !f.root.join("filter-marker").exists(),
            "pull executed incoming {driver} helper"
        );
        assert_eq!(result.unwrap_err().code, "external_filter");
        assert_eq!(git(&f.root, &["rev-parse", "HEAD"]), head);
        assert_eq!(fs::read(f.root.join("frontend/file.txt")).unwrap(), bytes);
        assert!(!f.root.join(".gitattributes").exists());
    }
}
#[test]
fn final_review_abandoned_custom_lock_does_not_strand_repository() {
    let f = Fixture::new();
    fs::write(
        f.root.join(".git/aijimu-operation.lock"),
        b"abandoned by exited process",
    )
    .unwrap();
    f.write("frontend/file.txt", "delivery\n");
    let ws = f.ws();
    let receipt = ws
        .publish(&f.input(&ws.preview(Role::Frontend).unwrap()))
        .unwrap();
    assert!(receipt.pushed);
    assert_eq!(git(&f.root, &["status", "--porcelain"]), "");
    fs::write(f.root.join(".git/index.lock"), b"another Git owner").unwrap();
    f.write("frontend/file.txt", "another delivery\n");
    assert_eq!(
        ws.publish(&f.input(&ws.preview(Role::Frontend).unwrap()))
            .unwrap_err()
            .code,
        "workspace_busy"
    );
    assert_eq!(
        fs::read(f.root.join(".git/index.lock")).unwrap(),
        b"another Git owner"
    );
}
#[test]
fn final_review_receipt_rewrite_failure_still_returns_known_delivery() {
    for pushed in [false, true] {
        let f = Fixture::new();
        if !pushed {
            git(
                &f.remote,
                &["config", "receive.denyCurrentBranch", "refuse"],
            );
            git(&f.remote, &["config", "core.bare", "false"]);
        }
        f.write("frontend/file.txt", "delivery\n");
        let root = f.root.clone();
        let ws = f.ws().before_push(move || {
            let head = git(&root, &["rev-parse", "HEAD"]);
            let path = root.join(format!(".git/aijimu/{head}.json"));
            block_file_replacement(&path);
        });
        let result = ws.publish(&f.input(&ws.preview(Role::Frontend).unwrap()));
        let head = git(&f.root, &["rev-parse", "HEAD"]);
        let path = f.root.join(format!(".git/aijimu/{head}.json"));
        restore_blocked_file(&path);
        let receipt =
            result.expect("post-commit persistence failure must retain the delivery outcome");
        assert_eq!(receipt.commit, head);
        assert_eq!(receipt.pushed, pushed);
        assert!(
            serde_json::to_value(&receipt).unwrap()["persistenceWarning"]
                .as_str()
                .is_some()
        );
        let recovered = f.ws().recover_receipt().unwrap().unwrap();
        assert_eq!(recovered.commit, head);
        assert_eq!(recovered.pushed, pushed);
        if !pushed {
            git(&f.remote, &["config", "core.bare", "true"]);
            assert!(f.ws().retry_push(&recovered).unwrap().pushed);
        }
        assert_eq!(git(&f.root, &["rev-list", "--count", "HEAD"]), "2");
    }
}

#[test]
fn final_review_recovers_after_store_save_failure_without_duplicate_commit() {
    for pushed in [false, true] {
        let f = Fixture::new();
        let path = f._temp.path().join("state.json");
        let mut store = CollaborationStore::open(path.clone()).unwrap();
        store.add_project(f.project()).unwrap();
        if !pushed {
            git(
                &f.remote,
                &["config", "receive.denyCurrentBranch", "refuse"],
            );
            git(&f.remote, &["config", "core.bare", "false"]);
        }
        f.write("frontend/file.txt", "recover this delivery\n");
        let ws = f.ws();
        let result = ws
            .publish(&f.input(&ws.preview(Role::Frontend).unwrap()))
            .unwrap();
        let head = result.commit.clone();
        let count = git(&f.root, &["rev-list", "--count", "HEAD"]);
        block_file_replacement(&path);
        let returned = store.record_delivery(result);
        assert_eq!(returned.commit, head);
        assert_eq!(returned.pushed, pushed);
        assert!(returned.persistence_warning.is_some());
        assert!(store.snapshot().projects[0].pending.is_none());
        restore_blocked_file(&path);
        drop(store);
        let mut reopened = CollaborationStore::open(path).unwrap();
        let recovered = ws
            .recover_receipt()
            .unwrap()
            .expect("bound HEAD proof is recoverable");
        assert_eq!(recovered.commit, head);
        assert_eq!(recovered.pushed, pushed);
        reopened.record_receipt(recovered.clone()).unwrap();
        if pushed {
            assert!(reopened.snapshot().projects[0].pending.is_none());
            assert_eq!(reopened.snapshot().messages[0].sha, head);
            assert!(reopened.snapshot().messages[0].own);
        } else {
            assert_eq!(
                reopened.snapshot().projects[0]
                    .pending
                    .as_ref()
                    .unwrap()
                    .commit,
                head
            );
            git(&f.remote, &["config", "core.bare", "true"]);
            let retried = ws.retry_push(&recovered).unwrap();
            assert!(retried.pushed);
            assert_eq!(retried.commit, head);
        }
        assert_eq!(git(&f.root, &["rev-list", "--count", "HEAD"]), count);
        assert_eq!(git(&f.root, &["status", "--porcelain"]), "");
    }
}
#[test]
fn final_review_recovery_ignores_pre_cas_orphan_and_rejects_mismatched_proof() {
    let f = Fixture::new();
    f.write("frontend/file.txt", "orphan candidate\n");
    let root = f.root.clone();
    let ws = f.ws().before_cas(move || {
        git(&root, &["symbolic-ref", "HEAD", "refs/heads/other"]);
    });
    let preview = ws.preview(Role::Frontend).unwrap();
    assert!(ws.publish(&f.input(&preview)).is_err());
    git(&f.root, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    assert!(f.ws().recover_receipt().unwrap().is_none());
    let ws = f.ws();
    let receipt = ws
        .publish(&f.input(&ws.preview(Role::Frontend).unwrap()))
        .unwrap();
    let path = f.root.join(format!(".git/aijimu/{}.json", receipt.commit));
    let mut wrong = receipt;
    wrong.parent = "f".repeat(40);
    fs::write(&path, serde_json::to_vec(&wrong).unwrap()).unwrap();
    assert_eq!(ws.recover_receipt().unwrap_err().code, "invalid_receipt");
}

#[test]
fn final_review_publish_does_not_remove_a_later_standard_git_lock() {
    let f = Fixture::new();
    f.write("frontend/file.txt", "delivery\n");
    let root = f.root.clone();
    let ws = f.ws().before_push(move || {
        fs::write(root.join(".git/index.lock"), "other Git operation").unwrap();
    });
    assert!(
        ws.publish(&f.input(&ws.preview(Role::Frontend).unwrap()))
            .unwrap()
            .pushed
    );
    assert_eq!(
        fs::read(f.root.join(".git/index.lock")).unwrap(),
        b"other Git operation"
    );
}
#[test]
fn final_review_custom_lock_blocks_live_owner_and_reopens_after_release() {
    let f = Fixture::new();
    let ws = f.ws();
    let owner = ws.operation().unwrap();
    assert_eq!(f.ws().pull().unwrap_err().code, "workspace_busy");
    drop(owner);
    assert_eq!(f.ws().pull().unwrap(), git(&f.root, &["rev-parse", "HEAD"]));
}

#[test]
fn local_commit_accumulates_without_touching_remote() {
    let f = Fixture::new();
    let remote_before = git(&f.remote, &["rev-parse", "refs/heads/main"]);
    f.write("backend/file.txt", "staged backend\n");
    git(&f.root, &["add", "backend/file.txt"]);
    f.write("backend/file.txt", "unstaged backend\n");
    let staged = git(&f.root, &["show", ":backend/file.txt"]);
    f.write("frontend/file.txt", "local one\n");
    let r = f
        .ws()
        .commit_local(&f.input(&f.ws().preview(Role::Frontend).unwrap()))
        .unwrap();
    assert!(!r.pushed && !r.push_requested && r.error.is_none());
    assert_eq!(
        git(&f.remote, &["rev-parse", "refs/heads/main"]),
        remote_before
    );
    fs::rename(&f.remote, f._temp.path().join("unavailable.git")).unwrap();
    f.write("frontend/file.txt", "local two\n");
    let r2 = f
        .ws()
        .commit_local(&f.input(&f.ws().preview(Role::Frontend).unwrap()))
        .unwrap();
    assert_eq!(r2.parent, r.commit);
    assert_eq!(git(&f.root, &["show", ":backend/file.txt"]), staged);
    assert_eq!(
        fs::read(f.root.join("backend/file.txt")).unwrap(),
        b"unstaged backend\n"
    );
    let recovered = f.ws().recover_receipt().unwrap().unwrap();
    assert!(!recovered.push_requested && recovered.error.is_none());
}

#[test]
fn local_history_pages_preserve_metadata_and_refuse_changed_head() {
    let f = Fixture::new();
    git(&f.root, &["config", "user.name", "中文作者"]);
    for i in 0..51 {
        git(
            &f.root,
            &[
                "commit",
                "--allow-empty",
                "-m",
                &format!("fix(frontend): 页面 - 第{i}次"),
                "-m",
                "原始正文\n第二行",
            ],
        );
    }
    fs::rename(&f.remote, f._temp.path().join("offline.git")).unwrap();
    let p = f.ws().local_history(0, None).unwrap();
    assert_eq!(p.commits.len(), 50);
    assert_eq!(p.next_offset, Some(50));
    assert_eq!(p.commits[0].author, "中文作者");
    assert_eq!(p.commits[0].body, "原始正文\n第二行\n");
    assert_eq!(p.commits[0].parsed.as_ref().unwrap().role, Role::Frontend);
    let last = f.ws().local_history(50, Some(&p.head)).unwrap();
    assert_eq!(last.commits.len(), 2);
    assert!(last.next_offset.is_none());
    assert!(last.commits[1].parents.is_empty());
    assert!(last.commits[1].parsed.is_none());
    assert!(f.ws().local_history(50, None).is_err());
    git(&f.root, &["commit", "--allow-empty", "-m", "new head"]);
    assert_eq!(
        f.ws().local_history(50, Some(&p.head)).unwrap_err().code,
        "head_changed"
    );
}

fn ordinary_commits(f: &Fixture) -> Vec<String> {
    (0..2)
        .map(|i| {
            f.write("frontend/file.txt", &format!("ordinary {i}\n"));
            git(&f.root, &["add", "frontend/file.txt"]);
            git(
                &f.root,
                &[
                    "commit",
                    "-m",
                    &format!("ordinary title {i}"),
                    "-m",
                    "body kept",
                ],
            );
            git(&f.root, &["rev-parse", "HEAD"])
        })
        .collect()
}

#[test]
fn local_push_complete_range_preserves_shas_and_dirty_index() {
    let f = Fixture::new();
    let shas = ordinary_commits(&f);
    f.write("backend/file.txt", "staged\n");
    git(&f.root, &["add", "backend/file.txt"]);
    f.write("backend/file.txt", "unstaged\n");
    let index = fs::read(f.root.join(".git/index")).unwrap();
    let p = f.ws().preview_local_push().unwrap();
    assert_eq!(
        p.commits.iter().map(|c| c.sha.clone()).collect::<Vec<_>>(),
        shas
    );
    assert_eq!(p.commits[0].body, "body kept\n");
    let r = f.ws().push_local_commits(&p.fingerprint).unwrap();
    assert!(r.pushed && r.error.is_none());
    assert_eq!(git(&f.remote, &["rev-parse", "main"]), shas[1]);
    assert_eq!(fs::read(f.root.join(".git/index")).unwrap(), index);
    assert_eq!(
        fs::read(f.root.join("backend/file.txt")).unwrap(),
        b"unstaged\n"
    );
    let recovered = f.ws().recover_local_push().unwrap().unwrap();
    assert!(recovered.pushed);
    // A repeated request uses its positive proof and never re-executes push.
    let ws = f.ws().before_push(|| panic!("duplicate push"));
    assert!(ws.push_local_commits(&p.fingerprint).unwrap().pushed);
}

#[test]
fn local_push_refuses_stale_head_origin_and_remote() {
    for change in ["head", "origin", "remote"] {
        let f = Fixture::new();
        ordinary_commits(&f);
        let p = f.ws().preview_local_push().unwrap();
        match change {
            "head" => {
                git(&f.root, &["commit", "--allow-empty", "-m", "late"]);
            }
            "origin" => {
                git(
                    &f.root,
                    &[
                        "remote",
                        "set-url",
                        "origin",
                        "https://github.com/other/repo.git",
                    ],
                );
            }
            _ => {
                f.peer_commit("remote advances\n");
            }
        }
        let head = git(&f.root, &["rev-parse", "HEAD"]);
        let index = fs::read(f.root.join(".git/index")).unwrap();
        assert!(
            f.ws().push_local_commits(&p.fingerprint).is_err(),
            "{change}"
        );
        assert_eq!(git(&f.root, &["rev-parse", "HEAD"]), head);
        assert_eq!(fs::read(f.root.join(".git/index")).unwrap(), index);
    }
}

#[test]
fn local_push_refuses_ahead_diverged_and_sensitive_history() {
    let f = Fixture::new();
    f.peer_commit("remote update\n");
    assert_eq!(
        f.ws().preview_local_push().unwrap_err().code,
        "remote_ahead"
    );
    ordinary_commits(&f);
    assert_eq!(f.ws().preview_local_push().unwrap_err().code, "diverged");
    let f = Fixture::new();
    f.write("frontend/.env", "secret\n");
    git(&f.root, &["add", "."]);
    git(&f.root, &["commit", "-m", "secret added"]);
    fs::remove_file(f.root.join("frontend/.env")).unwrap();
    git(&f.root, &["add", "."]);
    git(&f.root, &["commit", "-m", "secret removed"]);
    assert_eq!(
        f.ws().preview_local_push().unwrap_err().code,
        "sensitive_path"
    );
}

#[test]
fn local_push_failure_recovers_same_range_and_sha() {
    let f = Fixture::new();
    let shas = ordinary_commits(&f);
    let p = f.ws().preview_local_push().unwrap();
    git(
        &f.remote,
        &["config", "receive.denyCurrentBranch", "refuse"],
    );
    git(&f.remote, &["config", "core.bare", "false"]);
    let r = f.ws().push_local_commits(&p.fingerprint).unwrap();
    assert!(!r.pushed && r.error.is_some());
    let r = f.ws().recover_local_push().unwrap().unwrap();
    assert_eq!(r.head, shas[1]);
    assert!(!r.pushed);
    assert!(f.ws().configure_role_paths(&default_role_paths()).is_err());
    f.write("frontend/file.txt", "blocked\n");
    assert!(
        f.ws()
            .commit_local(&f.input(&f.ws().preview(Role::Frontend).unwrap()))
            .is_err()
    );
    git(&f.remote, &["config", "core.bare", "true"]);
    assert!(f.ws().push_local_commits(&p.fingerprint).unwrap().pushed);
    assert_eq!(git(&f.root, &["rev-list", "--count", "HEAD"]), "3");
}

#[test]
fn role_paths_custom_multiple_offline_commit_reopen_and_default() {
    let f = Fixture::new();
    let head = git(&f.root, &["rev-parse", "HEAD"]);
    let index = fs::read(f.root.join(".git/index")).unwrap();
    let mut paths = default_role_paths();
    paths.insert(
        Role::Frontend,
        vec!["apps/web".into(), "packages/ui".into()],
    );
    fs::rename(&f.remote, f._temp.path().join("offline.git")).unwrap();
    let project = f.ws().configure_role_paths(&paths).unwrap();
    assert_eq!(git(&f.root, &["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(f.root.join(".git/index")).unwrap(), index);
    assert_eq!(mapping(&f.root).unwrap(), paths);
    assert_eq!(
        f.ws().configure_role_paths(&paths).unwrap().role_paths,
        paths
    );
    let ws = GitWorkspace::new(project).with_test_transport(f.remote.to_string_lossy().into());
    f.write("apps/web/a.txt", "web\n");
    f.write("packages/ui/b.txt", "ui\n");
    let p = ws.preview(Role::Frontend).unwrap();
    assert_eq!(p.files.len(), 2);
    let r = ws.commit_local(&f.input(&p)).unwrap();
    assert!(!r.push_requested);
    assert_eq!(git(&f.root, &["show", "HEAD:packages/ui/b.txt"]), "ui");
    ws.configure_role_paths(&default_role_paths()).unwrap();
    assert_eq!(mapping(&f.root).unwrap(), default_role_paths());
}

#[test]
fn role_paths_reject_invalid_external_changes_and_unresolved_delivery() {
    let f = Fixture::new();
    f.ws().configure_role_paths(&default_role_paths()).unwrap();
    let original = fs::read(f.root.join("aijimu.workspace.json")).unwrap();
    for bad in [
        "../outside",
        "backend/sub",
        "/absolute",
        ".git/objects",
        "frontend/file.txt",
    ] {
        let mut paths = default_role_paths();
        paths.insert(Role::Frontend, vec![bad.into()]);
        assert!(f.ws().configure_role_paths(&paths).is_err(), "{bad}");
        assert_eq!(
            fs::read(f.root.join("aijimu.workspace.json")).unwrap(),
            original
        );
    }
    let mut paths = default_role_paths();
    paths.insert(Role::Frontend, vec!["web".into()]);
    f.ws().configure_role_paths(&paths).unwrap();
    assert_eq!(
        f.ws()
            .configure_role_paths(&default_role_paths())
            .unwrap_err()
            .code,
        "mapping_changed"
    );
    let f = Fixture::new();
    git(
        &f.remote,
        &["config", "receive.denyCurrentBranch", "refuse"],
    );
    git(&f.remote, &["config", "core.bare", "false"]);
    f.write("frontend/file.txt", "pending\n");
    let r = f
        .ws()
        .publish(&f.input(&f.ws().preview(Role::Frontend).unwrap()))
        .unwrap();
    assert!(!r.pushed);
    assert!(f.ws().configure_role_paths(&paths).is_err());
}

#[test]
fn local_push_allows_unchanged_sensitive_paths_already_on_remote() {
    let f = Fixture::new();
    f.write("backend/.env", "already remote\n");
    git(&f.root, &["add", "."]);
    git(&f.root, &["commit", "-m", "existing remote content"]);
    git(&f.root, &["push", "origin", "main"]);
    ordinary_commits(&f);
    assert_eq!(f.ws().preview_local_push().unwrap().commits.len(), 2);
}

#[test]
fn local_history_empty_and_push_to_empty_remote_keep_full_ancestry() {
    let f = Fixture::new();
    git(&f.remote, &["update-ref", "-d", "refs/heads/main"]);
    let shas = ordinary_commits(&f);
    let p = f.ws().preview_local_push().unwrap();
    assert!(p.remote_head.is_none());
    assert_eq!(p.commits.len(), 3);
    assert!(f.ws().push_local_commits(&p.fingerprint).unwrap().pushed);
    assert_eq!(git(&f.remote, &["rev-parse", "main"]), shas[1]);
    let f = Fixture::new();
    git(&f.root, &["update-ref", "-d", "refs/heads/main"]);
    // Remove remote refs to represent a genuinely unborn repository.
    git(&f.root, &["update-ref", "-d", "refs/remotes/origin/main"]);
    let p = f.ws().local_history(0, None).unwrap();
    assert!(p.head.is_empty() && p.commits.is_empty() && p.next_offset.is_none());
}

#[test]
fn local_history_merge_parents_and_mid_read_head_change() {
    let f = Fixture::new();
    git(&f.root, &["checkout", "-b", "side"]);
    git(&f.root, &["commit", "--allow-empty", "-m", "side"]);
    let side = git(&f.root, &["rev-parse", "HEAD"]);
    git(&f.root, &["checkout", "main"]);
    git(&f.root, &["commit", "--allow-empty", "-m", "main"]);
    let main = git(&f.root, &["rev-parse", "HEAD"]);
    git(&f.root, &["merge", "--no-ff", "-m", "merge", "side"]);
    assert_eq!(
        f.ws().local_history(0, None).unwrap().commits[0].parents,
        vec![main, side]
    );
    let root = f.root.clone();
    let ws = f.ws().before_history(move || {
        git(&root, &["commit", "--allow-empty", "-m", "race"]);
    });
    assert_eq!(ws.local_history(0, None).unwrap_err().code, "head_changed");
}

#[test]
fn role_paths_link_rejection_keeps_config_and_external_files() {
    let f = Fixture::new();
    f.ws().configure_role_paths(&default_role_paths()).unwrap();
    let original = fs::read(f.root.join("aijimu.workspace.json")).unwrap();
    let outside = f._temp.path().join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("keep.txt"), "keep").unwrap();
    #[cfg(windows)]
    assert!(
        Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!(
                    "New-Item -ItemType Junction -Path '{}' -Target '{}' | Out-Null",
                    f.root.join("linked").display(),
                    outside.display()
                )
            ])
            .status()
            .unwrap()
            .success()
    );
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, f.root.join("linked")).unwrap();
    let mut paths = default_role_paths();
    paths.insert(Role::Frontend, vec!["linked/sub".into()]);
    assert_eq!(
        f.ws().configure_role_paths(&paths).unwrap_err().code,
        "link_escape"
    );
    assert_eq!(
        fs::read(f.root.join("aijimu.workspace.json")).unwrap(),
        original
    );
    assert_eq!(fs::read(outside.join("keep.txt")).unwrap(), b"keep");
}

#[cfg(windows)]
#[test]
fn role_paths_atomic_write_failure_keeps_original_and_allows_retry() {
    let f = Fixture::new();
    f.ws().configure_role_paths(&default_role_paths()).unwrap();
    let path = f.root.join("aijimu.workspace.json");
    let original = fs::read(&path).unwrap();
    let mut paths = default_role_paths();
    paths.insert(Role::Frontend, vec!["web".into()]);
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&path, permissions).unwrap();
    assert!(f.ws().configure_role_paths(&paths).is_err());
    assert_eq!(fs::read(&path).unwrap(), original);
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_readonly(false);
    fs::set_permissions(&path, permissions).unwrap();
    f.ws().configure_role_paths(&paths).unwrap();
    f.ws().configure_role_paths(&paths).unwrap();
    assert_eq!(mapping(&f.root).unwrap(), paths);
}

#[test]
fn local_push_recovery_rejects_tampered_range_and_ignores_orphan() {
    let f = Fixture::new();
    ordinary_commits(&f);
    let p = f.ws().preview_local_push().unwrap();
    f.ws().push_local_commits(&p.fingerprint).unwrap();
    let path = f
        .root
        .join(format!(".git/gitcollab/local-push/{}.json", p.head));
    let bytes = fs::read(&path).unwrap();
    let mut proof: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    proof["receipt"]["commits"][0]["body"] = "tampered".into();
    fs::write(&path, serde_json::to_vec(&proof).unwrap()).unwrap();
    assert_eq!(
        f.ws().recover_local_push().unwrap_err().code,
        "invalid_receipt"
    );
    fs::write(&path, bytes).unwrap();
    git(&f.root, &["commit", "--allow-empty", "-m", "next"]);
    assert!(f.ws().recover_local_push().unwrap().is_none());
}

#[test]
fn local_receipt_offline_recovery_after_store_failure_allows_next_commit() {
    let f = Fixture::new();
    let path = f._temp.path().join("state.json");
    let mut store = CollaborationStore::open(path.clone()).unwrap();
    store.add_project(f.project()).unwrap();
    fs::rename(&f.remote, f._temp.path().join("offline.git")).unwrap();
    f.write("frontend/file.txt", "one\n");
    let r = f
        .ws()
        .commit_local(&f.input(&f.ws().preview(Role::Frontend).unwrap()))
        .unwrap();
    let before = store.snapshot().revision;
    let saved_state = fs::read(&path).unwrap();
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    let returned = store.record_delivery(r.clone());
    assert!(!returned.push_requested && returned.persistence_warning.is_some());
    assert_eq!(store.snapshot().revision, before);
    fs::remove_dir(&path).unwrap();
    fs::write(&path, saved_state).unwrap();
    drop(store);
    let mut store = CollaborationStore::open(path).unwrap();
    let recovered = f.ws().recover_local_receipt().unwrap().unwrap();
    assert_eq!(recovered.commit, r.commit);
    store.reconcile_receipt(recovered).unwrap();
    assert!(store.snapshot().projects[0].pending.is_none());
    f.write("frontend/file.txt", "two\n");
    let r2 = f
        .ws()
        .commit_local(&f.input(&f.ws().preview(Role::Frontend).unwrap()))
        .unwrap();
    assert_eq!(r2.parent, r.commit);
    assert_eq!(git(&f.root, &["rev-list", "--count", "HEAD"]), "3");
}

#[cfg(windows)]
#[test]
fn local_push_proof_rewrite_failure_and_store_failure_preserve_known_outcome() {
    for pushed in [false, true] {
        let f = Fixture::new();
        ordinary_commits(&f);
        let p = f.ws().preview_local_push().unwrap();
        if !pushed {
            git(
                &f.remote,
                &["config", "receive.denyCurrentBranch", "refuse"],
            );
            git(&f.remote, &["config", "core.bare", "false"]);
        }
        let proof_path = f
            .root
            .join(format!(".git/gitcollab/local-push/{}.json", p.head));
        let hook_path = proof_path.clone();
        let ws = f.ws().after_local_proof(move || {
            let mut permissions = fs::metadata(&hook_path).unwrap().permissions();
            permissions.set_readonly(true);
            fs::set_permissions(&hook_path, permissions).unwrap();
        });
        let r = ws.push_local_commits(&p.fingerprint).unwrap();
        assert_eq!(r.pushed, pushed);
        assert!(r.persistence_warning.is_some());
        let mut permissions = fs::metadata(&proof_path).unwrap().permissions();
        permissions.set_readonly(false);
        fs::set_permissions(&proof_path, permissions).unwrap();
        let path = f._temp.path().join("state.json");
        let mut store = CollaborationStore::open(path.clone()).unwrap();
        store.add_project(f.project()).unwrap();
        let before = store.snapshot().revision;
        let saved_state = fs::read(&path).unwrap();
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        let r = store.record_local_push(r);
        assert_eq!(r.pushed, pushed);
        assert!(r.persistence_warning.is_some());
        assert_eq!(store.snapshot().revision, before);
        fs::remove_dir(&path).unwrap();
        fs::write(&path, saved_state).unwrap();
        drop(store);
        let mut store = CollaborationStore::open(path).unwrap();
        let r = f.ws().recover_local_push().unwrap().unwrap();
        assert_eq!(r.pushed, pushed);
        assert_eq!(r.head, p.head);
        store.reconcile_local_push(r).unwrap();
        if !pushed {
            git(&f.remote, &["config", "core.bare", "true"]);
            assert!(f.ws().push_local_commits(&p.fingerprint).unwrap().pushed);
        }
        assert_eq!(git(&f.root, &["rev-list", "--count", "HEAD"]), "3");
    }
}

#[test]
fn local_push_preflight_races_refuse_before_transport() {
    for change in ["head", "origin", "remote"] {
        let f = Fixture::new();
        ordinary_commits(&f);
        let p = f.ws().preview_local_push().unwrap();
        let root = f.root.clone();
        let peer = f.peer.clone();
        let ws = f.ws().after_local_proof(move || match change {
            "head" => {
                git(&root, &["commit", "--allow-empty", "-m", "late"]);
            }
            "origin" => {
                git(
                    &root,
                    &[
                        "remote",
                        "set-url",
                        "origin",
                        "https://github.com/other/repo.git",
                    ],
                );
            }
            _ => {
                git(&peer, &["commit", "--allow-empty", "-m", "remote late"]);
                git(&peer, &["push", "origin", "main"]);
            }
        });
        assert!(ws.push_local_commits(&p.fingerprint).is_err(), "{change}");
        assert_ne!(git(&f.remote, &["rev-parse", "main"]), p.head);
    }
}

#[test]
fn double_clone_real_git_success_is_durable_despite_dedup() {
    let f = Fixture::new();
    let mut two = f.project();
    two.id = "two".into();
    two.root = f.peer.to_string_lossy().into();
    let ws2 = GitWorkspace::new(two.clone()).with_test_transport(f.remote.to_string_lossy().into());
    let path = f._temp.path().join("state.json");
    let mut store = CollaborationStore::open(path.clone()).unwrap();
    store.add_project(f.project()).unwrap();
    store.add_project(two).unwrap();
    let base = f.ws().fetch_updates(None).unwrap();
    store.apply_remote("fixture", base.clone()).unwrap();
    store.apply_remote("two", base.clone()).unwrap();
    git(
        &f.remote,
        &["config", "receive.denyCurrentBranch", "refuse"],
    );
    git(&f.remote, &["config", "core.bare", "false"]);
    f.write("frontend/file.txt", "delivery\n");
    let r = f
        .ws()
        .publish(&f.input(&f.ws().preview(Role::Frontend).unwrap()))
        .unwrap();
    assert!(!r.pushed);
    store.reconcile_receipt(r.clone()).unwrap();
    git(&f.remote, &["config", "core.bare", "true"]);
    git(&f.root, &["push", "origin", "main"]);
    store
        .apply_remote("two", ws2.fetch_updates(Some(&base.head)).unwrap())
        .unwrap();
    assert_eq!(store.snapshot().messages.len(), 1);
    let id = store.snapshot().messages[0].id.clone();
    store.mark_read(&[id]).unwrap();
    let recovered = f.ws().recover_receipt().unwrap().unwrap();
    assert!(store.reconcile_receipt(recovered).unwrap().pushed);
    drop(store);
    let store = CollaborationStore::open(path).unwrap();
    assert!(
        store.snapshot().projects[0]
            .last_receipt
            .as_ref()
            .unwrap()
            .pushed
    );
    assert!(store.snapshot().projects[0].pending.is_none());
    assert_eq!(store.snapshot().messages.len(), 1);
    assert!(store.snapshot().messages[0].read);
}

#[test]
fn local_push_oversized_history_is_refused_without_partial_preview() {
    let f = Fixture::new();
    let tree = git(&f.root, &["rev-parse", "HEAD^{tree}"]);
    let head = git(&f.root, &["rev-parse", "HEAD"]);
    let mut body = b"large body\n\n".to_vec();
    body.extend(vec![b'x'; LIMIT]);
    let sha = text(
        run(
            &f.root,
            &["commit-tree", &tree, "-p", &head],
            None,
            Some(body),
        )
        .unwrap(),
    )
    .unwrap();
    git(&f.root, &["update-ref", "refs/heads/main", sha.trim()]);
    assert_eq!(
        f.ws().preview_local_push().unwrap_err().code,
        "output_limit"
    );
    assert_eq!(git(&f.remote, &["rev-parse", "main"]), head);
}

#[test]
fn role_paths_after_success_keep_durable_proof_and_invalidate_previews() {
    let f = Fixture::new();
    ordinary_commits(&f);
    let p = f.ws().preview_local_push().unwrap();
    assert!(f.ws().push_local_commits(&p.fingerprint).unwrap().pushed);
    let mut paths = default_role_paths();
    paths.insert(Role::Frontend, vec!["web".into()]);
    let updated = f.ws().configure_role_paths(&paths).unwrap();
    let ws = GitWorkspace::new(updated).with_test_transport(f.remote.to_string_lossy().into());
    assert!(ws.recover_local_push().unwrap().unwrap().pushed);
    assert!(ws.recover_local_receipt().unwrap().is_none());
    assert_eq!(
        f.ws().preview_local_push().unwrap_err().code,
        "mapping_changed"
    );
}

#[test]
fn review_offline_role_recovery_uses_successful_covering_range() {
    let f = Fixture::new();
    f.write("frontend/file.txt", "local role\n");
    let role = f
        .ws()
        .commit_local(&f.input(&f.ws().preview(Role::Frontend).unwrap()))
        .unwrap();
    let preview = f.ws().preview_local_push().unwrap();
    assert!(
        f.ws()
            .push_local_commits(&preview.fingerprint)
            .unwrap()
            .pushed
    );
    fs::rename(&f.remote, f._temp.path().join("offline.git")).unwrap();
    let recovered = f.ws().recover_local_receipt().unwrap().unwrap();
    assert_eq!(recovered.commit, role.commit);
    assert!(!recovered.push_requested);
    assert!(
        recovered.pushed,
        "successful range must upgrade the local role proof offline"
    );
    assert!(recovered.error.is_none());
}

fn reconcile_offline_before_commit(
    ws: &GitWorkspace,
    store: &mut CollaborationStore,
) -> Result<()> {
    // Native integration contract: obtain both proofs under its project lock,
    // durably reconcile both, and only then allow the caller to advance HEAD.
    let role = ws.recover_local_receipt()?;
    let range = ws.recover_local_push_offline()?;
    if let Some(receipt) = role {
        store.reconcile_receipt(receipt)?;
    }
    if let Some(receipt) = range {
        store.reconcile_local_push(receipt)?;
    }
    Ok(())
}

fn assert_offline_range_survives_failed_store_and_next_commit(ordinary: bool) {
    let f = Fixture::new();
    let ws = f.ws();
    let path = f._temp.path().join("state.json");
    let mut store = CollaborationStore::open(path.clone()).unwrap();
    store.add_project(f.project()).unwrap();
    if ordinary {
        ordinary_commits(&f);
    } else {
        f.write("frontend/file.txt", "role A\n");
        let role = ws
            .commit_local(&f.input(&ws.preview(Role::Frontend).unwrap()))
            .unwrap();
        store.reconcile_receipt(role).unwrap();
    }
    let preview = ws.preview_local_push().unwrap();
    let successful = ws.push_local_commits(&preview.fingerprint).unwrap();
    assert!(successful.pushed);
    let saved_state = fs::read(&path).unwrap();
    let revision = store.snapshot().revision;
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    let result = store.record_local_push(successful.clone());
    assert!(result.pushed && result.persistence_warning.is_some());
    assert_eq!(store.snapshot().revision, revision);
    assert!(store.snapshot().projects[0].last_local_push.is_none());
    fs::rename(&f.remote, f._temp.path().join("unavailable.git")).unwrap();
    f.write("frontend/file.txt", "next local\n");
    let next_input = f.input(&ws.preview(Role::Frontend).unwrap());
    let result = reconcile_offline_before_commit(&ws, &mut store)
        .and_then(|()| ws.commit_local(&next_input));
    assert_eq!(result.unwrap_err().code, "store_io");
    assert_eq!(git(&f.root, &["rev-parse", "HEAD"]), successful.head);
    assert_eq!(store.snapshot().revision, revision);
    fs::remove_dir(&path).unwrap();
    fs::write(&path, saved_state).unwrap();
    drop(store);
    let mut store = CollaborationStore::open(path.clone()).unwrap();
    reconcile_offline_before_commit(&ws, &mut store).unwrap();
    let state = &store.snapshot().projects[0];
    assert_eq!(state.last_local_push.as_ref(), Some(&successful));
    if !ordinary {
        let role = state.last_receipt.as_ref().unwrap();
        assert!(role.pushed && !role.push_requested && role.error.is_none());
    }
    let next = ws.commit_local(&next_input).unwrap();
    assert_eq!(next.parent, successful.head);
    store.reconcile_receipt(next).unwrap();
    assert!(ws.recover_local_push_offline().unwrap().is_none());
    drop(store);
    let store = CollaborationStore::open(path).unwrap();
    assert_eq!(
        store.snapshot().projects[0].last_local_push.as_ref(),
        Some(&successful)
    );
    assert_eq!(store.snapshot().messages.len(), successful.commits.len());
    for commit in &successful.commits {
        assert!(store.snapshot().messages.iter().any(|m| m.sha == commit.sha
            && m.title == commit.title
            && m.author == commit.author
            && m.committed_at == commit.committed_at));
    }
}

#[test]
fn review_offline_successful_role_range_recovery_before_next_commit() {
    assert_offline_range_survives_failed_store_and_next_commit(false);
}

#[test]
fn review_offline_successful_ordinary_range_recovery_before_next_commit() {
    assert_offline_range_survives_failed_store_and_next_commit(true);
}

#[test]
fn review_offline_range_recovery_validates_pending_objects_and_current_head() {
    let f = Fixture::new();
    assert!(f.ws().recover_local_push_offline().unwrap().is_none());
    ordinary_commits(&f);
    let preview = f.ws().preview_local_push().unwrap();
    git(
        &f.remote,
        &["config", "receive.denyCurrentBranch", "refuse"],
    );
    git(&f.remote, &["config", "core.bare", "false"]);
    assert!(
        !f.ws()
            .push_local_commits(&preview.fingerprint)
            .unwrap()
            .pushed
    );
    assert_eq!(
        f.ws().recover_local_push_offline().unwrap_err().code,
        "pending_delivery"
    );
    git(&f.remote, &["config", "core.bare", "true"]);
    assert!(
        f.ws()
            .push_local_commits(&preview.fingerprint)
            .unwrap()
            .pushed
    );
    fs::rename(&f.remote, f._temp.path().join("unavailable.git")).unwrap();
    assert!(f.ws().recover_local_push_offline().unwrap().unwrap().pushed);
    let path = f
        .root
        .join(format!(".git/gitcollab/local-push/{}.json", preview.head));
    let bytes = fs::read(&path).unwrap();
    let mut proof: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    proof["receipt"]["commits"][0]["body"] = "tampered".into();
    fs::write(&path, serde_json::to_vec(&proof).unwrap()).unwrap();
    assert_eq!(
        f.ws().recover_local_push_offline().unwrap_err().code,
        "invalid_receipt"
    );
    fs::write(&path, bytes).unwrap();
    git(&f.root, &["commit", "--allow-empty", "-m", "later head"]);
    assert!(f.ws().recover_local_push_offline().unwrap().is_none());
}

#[test]
fn review_offline_requested_success_after_store_failure_before_next_commit() {
    let f = Fixture::new();
    let ws = f.ws();
    let path = f._temp.path().join("state.json");
    let mut store = CollaborationStore::open(path.clone()).unwrap();
    store.add_project(f.project()).unwrap();
    f.write("frontend/file.txt", "requested delivery\n");
    let successful = ws
        .publish(&f.input(&ws.preview(Role::Frontend).unwrap()))
        .unwrap();
    assert!(successful.pushed && successful.push_requested);
    assert!(!ws.local_proof_path(&successful.commit).unwrap().exists());
    let saved_state = fs::read(&path).unwrap();
    let revision = store.snapshot().revision;
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(
        store
            .record_delivery(successful.clone())
            .persistence_warning
            .is_some()
    );
    fs::rename(&f.remote, f._temp.path().join("unavailable.git")).unwrap();
    f.write("frontend/file.txt", "next local\n");
    let next_input = f.input(&ws.preview(Role::Frontend).unwrap());
    let result = reconcile_offline_before_commit(&ws, &mut store)
        .and_then(|()| ws.commit_local(&next_input));
    assert_eq!(result.unwrap_err().code, "store_io");
    assert_eq!(git(&f.root, &["rev-parse", "HEAD"]), successful.commit);
    assert_eq!(store.snapshot().revision, revision);
    fs::remove_dir(&path).unwrap();
    fs::write(&path, saved_state).unwrap();
    drop(store);
    let mut store = CollaborationStore::open(path.clone()).unwrap();
    let recovered = ws.recover_local_receipt().unwrap().unwrap();
    assert_eq!(recovered, successful);
    reconcile_offline_before_commit(&ws, &mut store).unwrap();
    assert_eq!(
        store.snapshot().projects[0].last_receipt.as_ref(),
        Some(&successful)
    );
    let next = ws.commit_local(&next_input).unwrap();
    assert_eq!(next.parent, successful.commit);
    store.reconcile_receipt(next).unwrap();
    drop(store);
    let store = CollaborationStore::open(path).unwrap();
    assert_eq!(store.snapshot().messages.len(), 1);
    assert_eq!(store.snapshot().messages[0].sha, successful.commit);
}

#[test]
fn review_offline_covering_range_resolves_requested_role_pending() {
    let f = Fixture::new();
    git(
        &f.remote,
        &["config", "receive.denyCurrentBranch", "refuse"],
    );
    git(&f.remote, &["config", "core.bare", "false"]);
    f.write("frontend/file.txt", "pending requested role\n");
    let ws = f.ws();
    let role = ws
        .publish(&f.input(&ws.preview(Role::Frontend).unwrap()))
        .unwrap();
    assert!(role.push_requested && !role.pushed);
    assert_eq!(
        ws.recover_local_receipt().unwrap_err().code,
        "pending_delivery"
    );
    git(&f.remote, &["config", "core.bare", "true"]);
    let preview = ws.preview_local_push().unwrap();
    assert!(ws.push_local_commits(&preview.fingerprint).unwrap().pushed);
    fs::rename(&f.remote, f._temp.path().join("unavailable.git")).unwrap();
    let recovered = ws.recover_local_receipt().unwrap().unwrap();
    assert!(recovered.push_requested && recovered.pushed && recovered.error.is_none());
    assert_eq!(recovered.commit, role.commit);
}

#[test]
fn review_offline_head_change_cannot_apply_previous_success_to_new_role() {
    let f = Fixture::new();
    f.write("frontend/file.txt", "role A\n");
    let role_a = f
        .ws()
        .commit_local(&f.input(&f.ws().preview(Role::Frontend).unwrap()))
        .unwrap();
    let preview = f.ws().preview_local_push().unwrap();
    assert!(
        f.ws()
            .push_local_commits(&preview.fingerprint)
            .unwrap()
            .pushed
    );
    fs::rename(&f.remote, f._temp.path().join("unavailable.git")).unwrap();
    // Prepare an actual later role object/proof without moving HEAD yet. The
    // external Git writer will advance HEAD during offline recovery's read gap.
    let tree = git(&f.root, &["rev-parse", "HEAD^{tree}"]);
    let body = format!(
        "{}\n\nAI-Jimu-Role: frontend\nAI-Jimu-Delivery: {}\n",
        role_a.message,
        uuid::Uuid::new_v4()
    );
    let sha = text(
        run(
            &f.root,
            &["commit-tree", &tree, "-p", &role_a.commit],
            None,
            Some(body.into_bytes()),
        )
        .unwrap(),
    )
    .unwrap()
    .trim()
    .to_owned();
    let mut role_b = role_a.clone();
    role_b.parent = role_a.commit.clone();
    role_b.commit = sha.clone();
    role_b.url = format!("https://github.com/owner/repo/commit/{sha}");
    f.ws().save_receipt(&role_b).unwrap();
    let root = f.root.clone();
    let next = sha.clone();
    let old = role_a.commit;
    let ws = f.ws().before_history(move || {
        git(&root, &["update-ref", "refs/heads/main", &next, &old]);
    });
    let recovered = ws.recover_local_receipt().unwrap().unwrap();
    assert_eq!(recovered.commit, sha);
    assert_eq!(
        recovered, role_b,
        "the earlier HEAD's success cannot upgrade this new role SHA"
    );
    assert!(f.ws().recover_local_push_offline().unwrap().is_none());
}

#[test]
fn final_review_same_head_new_fingerprint_preserves_success_offline() {
    let f = Fixture::new();
    let g = git(&f.root, &["rev-parse", "HEAD"]);
    git(&f.root, &["commit", "--allow-empty", "-m", "baseline B"]);
    let b = git(&f.root, &["rev-parse", "HEAD"]);
    git(&f.root, &["push", "origin", "main"]);
    let shas = ordinary_commits(&f);
    let ws = f.ws();
    let original = ws.preview_local_push().unwrap();
    assert_eq!(original.remote_head.as_ref(), Some(&b));
    let successful = ws.push_local_commits(&original.fingerprint).unwrap();
    assert!(successful.pushed);
    assert_eq!(successful.commits.len(), 2);
    let proof_path = f
        .root
        .join(format!(".git/gitcollab/local-push/{}.json", original.head));
    let proof_bytes = fs::read(&proof_path).unwrap();
    git(&f.remote, &["update-ref", "refs/heads/main", &g]);
    let fresh = ws.preview_local_push().unwrap();
    assert_eq!(fresh.head, original.head);
    assert_ne!(fresh.fingerprint, original.fingerprint);
    assert_eq!(fresh.remote_head.as_ref(), Some(&g));
    assert_eq!(
        fresh
            .commits
            .iter()
            .map(|c| c.sha.clone())
            .collect::<Vec<_>>(),
        vec![b.clone(), shas[0].clone(), shas[1].clone()]
    );
    // A valid old proof must never turn arbitrary or now-stale input into success.
    assert_eq!(
        ws.push_local_commits("invalid-fingerprint")
            .unwrap_err()
            .code,
        "stale_preview"
    );
    git(&f.remote, &["update-ref", "refs/heads/main", &b]);
    assert_eq!(
        ws.push_local_commits(&fresh.fingerprint).unwrap_err().code,
        "stale_preview"
    );
    assert_eq!(fs::read(&proof_path).unwrap(), proof_bytes);
    git(&f.remote, &["update-ref", "refs/heads/main", &g]);
    let marker = f.remote.join("hook-entered");
    let hook = f.remote.join("hooks/pre-receive");
    fs::write(
        &hook,
        "#!/bin/sh\nprintf attempted > hook-entered\nexit 1\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let mut returned = ws.push_local_commits(&fresh.fingerprint).unwrap();
    assert!(
        !marker.exists(),
        "known success must skip the second push and rejection hook"
    );
    assert_eq!(git(&f.remote, &["rev-parse", "main"]), g);
    assert!(
        returned
            .persistence_warning
            .as_deref()
            .unwrap()
            .contains("本次未重复上传")
    );
    returned.persistence_warning = successful.persistence_warning.clone();
    assert_eq!(
        returned, successful,
        "return original complete historical metadata"
    );
    assert_eq!(
        fs::read(&proof_path).unwrap(),
        proof_bytes,
        "successful proof is immutable"
    );
    // Identical successful retries remain offline no-ops too.
    let no_push = f.ws().before_push(|| panic!("duplicate successful push"));
    assert_eq!(
        no_push.push_local_commits(&original.fingerprint).unwrap(),
        successful
    );
    fs::rename(&f.remote, f._temp.path().join("unavailable.git")).unwrap();
    assert_eq!(
        ws.recover_local_push_offline().unwrap().unwrap(),
        successful
    );
    let path = f._temp.path().join("state.json");
    let mut store = CollaborationStore::open(path.clone()).unwrap();
    store.add_project(f.project()).unwrap();
    reconcile_offline_before_commit(&ws, &mut store).unwrap();
    assert_eq!(
        store.snapshot().projects[0].last_local_push.as_ref(),
        Some(&successful)
    );
    f.write("frontend/file.txt", "next offline commit\n");
    let next = ws
        .commit_local(&f.input(&ws.preview(Role::Frontend).unwrap()))
        .unwrap();
    assert_eq!(next.parent, successful.head);
    let next_head = next.commit.clone();
    store.reconcile_receipt(next).unwrap();
    drop(store);
    let reopened = CollaborationStore::open(path).unwrap();
    assert_eq!(
        reopened.snapshot().projects[0].last_local_push.as_ref(),
        Some(&successful)
    );
    assert_eq!(
        reopened.snapshot().projects[0]
            .last_receipt
            .as_ref()
            .unwrap()
            .commit,
        next_head
    );
    assert!(ws.recover_local_push_offline().unwrap().is_none());
    assert_eq!(fs::read(&proof_path).unwrap(), proof_bytes);
}
