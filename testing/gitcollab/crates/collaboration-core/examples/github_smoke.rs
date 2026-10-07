//! Explicit, opt-in live GitHub acceptance. Never run by `cargo test`.
//! Uploads only fixed public test text on a new codex/gitcollab-smoke-* branch.
use collaboration_core::{AddProjectInput, CollaborationStore, GitWorkspace, PublishInput, Role};
use std::{collections::BTreeMap, fs, path::Path, process::Command};

const REMOTE: &str = "https://github.com/TOGET-H/BigzChatAI-v0.2.git";

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "never")
        .output()
        .expect("Git is required");
    // Do not print raw Git diagnostics: helpers may include credentials.
    assert!(output.status.success(), "Git acceptance step failed");
    String::from_utf8(output.stdout).unwrap().trim().into()
}
fn basic_header(token: &str) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let input = format!("x-access-token:{token}");
    let mut encoded = String::new();
    for chunk in input.as_bytes().chunks(3) {
        let n = ((chunk[0] as u32) << 16)
            | ((chunk.get(1).copied().unwrap_or(0) as u32) << 8)
            | chunk.get(2).copied().unwrap_or(0) as u32;
        encoded.push(TABLE[((n >> 18) & 63) as usize] as char);
        encoded.push(TABLE[((n >> 12) & 63) as usize] as char);
        encoded.push(if chunk.len() > 1 {
            TABLE[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        encoded.push(if chunk.len() > 2 {
            TABLE[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    format!("AUTHORIZATION: basic {encoded}")
}
fn configure_git(root: &Path) {
    git(
        root,
        &["config", "--local", "user.name", "Git Collab acceptance"],
    );
    git(
        root,
        &[
            "config",
            "--local",
            "user.email",
            "gitcollab-test@example.invalid",
        ],
    );
    git(root, &["config", "--local", "core.autocrlf", "false"]);
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        assert!(!token.is_empty(), "An empty job token is invalid");
        // Ephemeral clone-local credentials; never a URL or global Git setting.
        git(
            root,
            &[
                "config",
                "--local",
                "http.https://github.com/.extraheader",
                &basic_header(&token),
            ],
        );
    }
}
fn remote_head(root: &Path, branch: &str) -> String {
    git(
        root,
        &[
            "ls-remote",
            "--heads",
            "origin",
            &format!("refs/heads/{branch}"),
        ],
    )
    .split_whitespace()
    .next()
    .unwrap_or("")
    .into()
}
fn discover(root: &Path, branch: &str) -> collaboration_core::Project {
    GitWorkspace::discover(&AddProjectInput {
        root: root.to_str().unwrap().into(),
        name: "Cross-platform acceptance".into(),
        branch: branch.into(),
        create_template: false,
    })
    .unwrap()
}
fn input(id: &str, fingerprint: String, summary: &str) -> PublishInput {
    PublishInput {
        project_id: id.into(),
        role: Role::Frontend,
        kind: "test".into(),
        module: "跨平台验收".into(),
        summary: summary.into(),
        fingerprint,
    }
}
fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert!(
        args.len() == 3 && args[0] == "--live-github",
        "Usage: --live-github BASE_BRANCH codex/gitcollab-smoke-UNIQUE_ID"
    );
    let base = &args[1];
    let branch = &args[2];
    assert!(base.starts_with("codex/gitcollab-platform-"));
    assert!(branch.starts_with("codex/gitcollab-smoke-") && branch.len() <= 120);
    assert!(
        base.chars()
            .chain(branch.chars())
            .all(|c| c.is_ascii_alphanumeric() || "-_/".contains(c))
    );
    let data = std::env::var_os("GITCOLLAB_DATA_DIR").expect("GITCOLLAB_DATA_DIR is required");
    let report =
        std::env::var_os("GITCOLLAB_TEST_REPORT").expect("GITCOLLAB_TEST_REPORT is required");
    assert!(
        !Path::new(&report).exists(),
        "Use a new report path; do not overwrite evidence"
    );
    fs::create_dir_all(&data).unwrap();
    let temp = tempfile::tempdir_in(data).unwrap();
    let root = temp.path().join("work");
    let peer = temp.path().join("peer");
    git(
        temp.path(),
        &["clone", "--branch", base, REMOTE, root.to_str().unwrap()],
    );
    configure_git(&root);
    assert!(
        remote_head(&root, branch).is_empty(),
        "Test branch already exists; no overwrite"
    );
    git(&root, &["switch", "-c", branch]);
    assert!(
        !root.join("aijimu.workspace.json").exists(),
        "Preserve any existing mapping"
    );
    let paths: BTreeMap<_, _> = Role::all()
        .into_iter()
        .map(|r| (r, vec![format!("gitcollab-smoke/{}", r.as_str())]))
        .collect();
    let project = GitWorkspace::new(discover(&root, branch))
        .configure_role_paths(&paths)
        .unwrap();
    git(&root, &["add", "--", "aijimu.workspace.json"]);
    git(
        &root,
        &[
            "commit",
            "-m",
            "test(testing): 跨平台验收 - 初始化隔离身份映射",
        ],
    );
    let bootstrap = git(&root, &["rev-parse", "HEAD"]);
    git(
        &root,
        &["push", "origin", &format!("HEAD:refs/heads/{branch}")],
    );
    git(
        temp.path(),
        &["clone", "--branch", branch, REMOTE, peer.to_str().unwrap()],
    );
    configure_git(&peer);
    let peer_project = discover(&peer, branch);
    let ws = GitWorkspace::new(project.clone());
    let peer_ws = GitWorkspace::new(peer_project.clone());
    let state_path = temp.path().join("state.json");
    let mut store = CollaborationStore::open(state_path.clone()).unwrap();
    store.add_project(project.clone()).unwrap();
    store
        .apply_remote(&project.id, ws.fetch_updates(None).unwrap())
        .unwrap();
    let file = root.join("gitcollab-smoke/frontend/probe.txt");
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    let content = format!(
        "Git Collab public smoke fixture\nos={}\nphase=local\n",
        std::env::consts::OS
    );
    fs::write(&file, &content).unwrap();
    let local = ws
        .commit_local(&input(
            &project.id,
            ws.preview(Role::Frontend).unwrap().fingerprint,
            "验证仅本地提交",
        ))
        .unwrap();
    assert!(!local.pushed && !local.push_requested);
    assert_eq!(
        remote_head(&root, branch),
        bootstrap,
        "Local commit must not push"
    );
    store.record_delivery(local.clone());
    assert_eq!(
        ws.local_history(0, None).unwrap().commits[0].sha,
        local.commit
    );
    let outside = root.join("gitcollab-smoke/backend/keep.txt");
    fs::create_dir_all(outside.parent().unwrap()).unwrap();
    fs::write(&outside, b"public unrelated staged fixture\n").unwrap();
    git(&root, &["add", "--", "gitcollab-smoke/backend/keep.txt"]);
    let index = fs::read(root.join(".git/index")).unwrap();
    let preview = ws.preview_local_push().unwrap();
    assert_eq!(preview.commits.len(), 1);
    assert_eq!(preview.commits[0].sha, local.commit);
    let range = ws.push_local_commits(&preview.fingerprint).unwrap();
    assert!(range.pushed && range.persistence_warning.is_none());
    assert_eq!(remote_head(&root, branch), local.commit);
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
    assert_eq!(
        fs::read(&outside).unwrap(),
        b"public unrelated staged fixture\n"
    );
    store.reconcile_local_push(range).unwrap();
    drop(store);
    let mut store = CollaborationStore::open(state_path).unwrap();
    assert!(
        store.snapshot().projects[0]
            .last_receipt
            .as_ref()
            .unwrap()
            .pushed
    );
    assert_eq!(
        peer_ws
            .fetch_updates(Some(&bootstrap))
            .unwrap()
            .commits
            .len(),
        1
    );
    assert_eq!(peer_ws.pull().unwrap(), local.commit);
    assert_eq!(
        fs::read_to_string(peer.join("gitcollab-smoke/frontend/probe.txt")).unwrap(),
        content
    );
    fs::write(
        peer.join("gitcollab-smoke/frontend/probe.txt"),
        b"Git Collab public smoke fixture\nphase=peer\n",
    )
    .unwrap();
    let delivery = peer_ws
        .publish(&input(
            &peer_project.id,
            peer_ws.preview(Role::Frontend).unwrap().fingerprint,
            "验证角色提交推送",
        ))
        .unwrap();
    assert!(delivery.pushed && delivery.persistence_warning.is_none());
    assert_eq!(git(&root, &["rev-parse", "HEAD"]), local.commit);
    assert!(ws.pull().is_err(), "Dirty work must block pull");
    assert_eq!(
        fs::read(&outside).unwrap(),
        b"public unrelated staged fixture\n"
    );
    // Only our generated fixture is unstaged and removed, inside the TempDir.
    git(
        &root,
        &[
            "restore",
            "--staged",
            "--",
            "gitcollab-smoke/backend/keep.txt",
        ],
    );
    fs::remove_file(&outside).unwrap();
    let updates = ws.fetch_updates(Some(&local.commit)).unwrap();
    assert_eq!(updates.commits.len(), 1);
    store.apply_remote(&project.id, updates).unwrap();
    assert_eq!(ws.pull().unwrap(), delivery.commit);
    assert_eq!(
        fs::read(&file).unwrap(),
        b"Git Collab public smoke fixture\nphase=peer\n"
    );
    let evidence = serde_json::json!({"status":"passed", "os":std::env::consts::OS,
        "repository":"TOGET-H/BigzChatAI-v0.2", "branch":branch, "baseBranch":base,
        "bootstrap":bootstrap, "localCommit":local.commit, "peerCommit":delivery.commit,
        "checks":["history","local-only","range-push-original-sha","unrelated-index-preserved",
            "store-restart","peer-fetch-pull","role-publish","dirty-pull-refusal","clean-pull"]});
    use std::io::Write;
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(report)
        .unwrap()
        .write_all(serde_json::to_string_pretty(&evidence).unwrap().as_bytes())
        .unwrap();
    println!("{}", serde_json::to_string(&evidence).unwrap());
}
