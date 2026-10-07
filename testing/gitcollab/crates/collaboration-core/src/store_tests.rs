use crate::*;
use std::{fs, path::PathBuf};
use tempfile::TempDir;

fn project(id: &str, root: &str) -> Project {
    Project {
        id: id.into(),
        name: id.into(),
        root: root.into(),
        repository: "owner/repo".into(),
        remote_url: "https://github.com/owner/repo.git".into(),
        branch: "main".into(),
        role_paths: crate::model::default_role_paths(),
        monitor_enabled: true,
    }
}
fn commit(sha: &str, title: &str) -> RemoteCommit {
    RemoteCommit {
        sha: sha.into(),
        title: title.into(),
        author: "Alice".into(),
        committed_at: "2026-10-07T10:00:00+08:00".into(),
    }
}
fn remote(head: &str, commits: Vec<RemoteCommit>) -> RemoteSnapshot {
    RemoteSnapshot {
        head: head.into(),
        commits,
        rewritten: false,
    }
}
fn receipt(pushed: bool) -> PublishReceipt {
    PublishReceipt {
        push_requested: true,
        persistence_warning: None,
        project_id: "one".into(),
        role: Role::Frontend,
        commit: "own".into(),
        parent: "base".into(),
        message: "fix(frontend): 商品页 - 修复筛选".into(),
        url: "https://github.com/owner/repo/commit/own".into(),
        pushed,
        error: (!pushed).then(|| "网络失败".into()),
    }
}
fn setup() -> (TempDir, PathBuf, CollaborationStore) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    let mut store = CollaborationStore::open(path.clone()).unwrap();
    store
        .add_project(project("one", "C:/projects/one"))
        .unwrap();
    (dir, path, store)
}
fn baseline(store: &mut CollaborationStore) {
    assert!(
        store
            .apply_remote("one", remote("base", vec![]))
            .unwrap()
            .is_empty()
    );
}
fn json(store: &CollaborationStore) -> serde_json::Value {
    serde_json::to_value(store.snapshot()).unwrap()
}

#[test]
fn defaults_and_project_settings_survive_restart_as_camel_case() {
    // Catches settings that exist only in memory or role/IPC spelling drift.
    let (_dir, path, mut store) = setup();
    assert_eq!(store.snapshot().selected_role, Role::Product);
    assert!(!store.snapshot().system_notifications);
    store.select(Some("one".into()), Role::Backend).unwrap();
    store.set_monitor("one", false).unwrap();
    store.set_notifications(true).unwrap();
    drop(store);
    let store = CollaborationStore::open(path).unwrap();
    let value = json(&store);
    assert_eq!(value["selectedProject"], "one");
    assert_eq!(value["selectedRole"], "backend");
    assert_eq!(value["systemNotifications"], true);
    assert_eq!(value["projects"][0]["project"]["monitorEnabled"], false);
    assert!(value.get("selected_project").is_none());
}
#[test]
fn first_snapshot_ignores_historic_commits_and_persists_baseline() {
    let (_dir, path, mut store) = setup();
    assert!(
        store
            .apply_remote(
                "one",
                remote(
                    "base",
                    vec![commit("old", "historical"), commit("base", "baseline")]
                )
            )
            .unwrap()
            .is_empty()
    );
    assert!(store.snapshot().messages.is_empty());
    drop(store);
    let store = CollaborationStore::open(path).unwrap();
    assert_eq!(store.snapshot().projects[0].cursor.as_deref(), Some("base"));
}
#[test]
fn multiple_remote_commits_have_metadata_links_and_optional_parsed_identity() {
    let (_dir, _path, mut store) = setup();
    baseline(&mut store);
    let messages = store
        .apply_remote(
            "one",
            remote(
                "new2",
                vec![
                    commit("new1", "feat(backend): API - 新增接口"),
                    commit("new2", "ordinary external commit"),
                ],
            ),
        )
        .unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].project_id, "one");
    assert_eq!(messages[0].repository, "owner/repo");
    assert_eq!(messages[0].branch, "main");
    assert_eq!(messages[0].sha, "new1");
    assert_eq!(messages[0].author, "Alice");
    assert_eq!(messages[0].committed_at, "2026-10-07T10:00:00+08:00");
    assert_eq!(messages[0].url, "https://github.com/owner/repo/commit/new1");
    let parsed = messages[0].parsed.as_ref().unwrap();
    assert_eq!(parsed.role, Role::Backend);
    assert_eq!(parsed.module, "API");
    assert!(!messages[0].read);
    assert!(!messages[0].rewritten);
    assert_eq!(messages[1].title, "ordinary external commit");
    assert!(messages[1].parsed.is_none());
    assert_eq!(store.snapshot().messages.len(), 2);
    assert!(!store.snapshot().system_notifications); // app list is independent of system notification opt-in
}
#[test]
fn duplicate_remote_sha_emits_once_including_across_restart_and_preserves_read() {
    let (_dir, path, mut store) = setup();
    baseline(&mut store);
    let messages = store
        .apply_remote(
            "one",
            remote("new", vec![commit("new", "plain"), commit("new", "plain")]),
        )
        .unwrap();
    assert_eq!(messages.len(), 1);
    store.mark_read(&[messages[0].id.clone()]).unwrap();
    drop(store);
    let mut store = CollaborationStore::open(path).unwrap();
    assert!(
        store
            .apply_remote("one", remote("new", vec![commit("new", "plain")]))
            .unwrap()
            .is_empty()
    );
    assert_eq!(store.snapshot().messages.len(), 1);
    assert!(store.snapshot().messages[0].read);
}
#[test]
fn own_success_is_visible_immediately_without_fabricated_metadata_or_cursor_advance() {
    let (_dir, path, mut store) = setup();
    baseline(&mut store);
    store.record_receipt(receipt(true)).unwrap();
    store.record_receipt(receipt(true)).unwrap();
    let snap = store.snapshot();
    assert_eq!(snap.messages.len(), 1);
    assert_eq!(snap.messages[0].sha, "own");
    assert_eq!(snap.messages[0].title, "fix(frontend): 商品页 - 修复筛选");
    assert_eq!(snap.messages[0].author, "");
    assert_eq!(snap.messages[0].committed_at, "");
    assert!(snap.messages[0].read);
    assert_eq!(snap.projects[0].cursor.as_deref(), Some("base"));
    assert!(snap.projects[0].pending.is_none());
    drop(store);
    let store = CollaborationStore::open(path).unwrap();
    assert_eq!(store.snapshot().messages.len(), 1);
}
#[test]
fn remote_hydrates_own_receipt_after_restart_without_notifying_or_resetting_read() {
    let (_dir, path, mut store) = setup();
    baseline(&mut store);
    store.record_receipt(receipt(true)).unwrap();
    let id = store.snapshot().messages[0].id.clone();
    drop(store);
    let mut store = CollaborationStore::open(path).unwrap();
    assert!(
        store
            .apply_remote(
                "one",
                remote(
                    "own",
                    vec![commit("own", "fix(frontend): 商品页 - 修复筛选")]
                )
            )
            .unwrap()
            .is_empty()
    );
    let snap = store.snapshot();
    assert_eq!(snap.messages.len(), 1);
    assert_eq!(snap.messages[0].id, id);
    assert_eq!(snap.messages[0].author, "Alice");
    assert_eq!(snap.messages[0].committed_at, "2026-10-07T10:00:00+08:00");
    assert!(snap.messages[0].read);
    store.record_receipt(receipt(true)).unwrap();
    assert_eq!(store.snapshot().messages[0].author, "Alice");
}
#[test]
fn first_baseline_hydrates_own_receipts_but_skips_other_history() {
    let (_dir, _path, mut store) = setup();
    store.record_receipt(receipt(true)).unwrap();
    assert!(store.snapshot().projects[0].cursor.is_none());
    assert!(
        store
            .apply_remote(
                "one",
                remote(
                    "latest",
                    vec![
                        commit("old", "historical"),
                        commit("own", "fix(frontend): 商品页 - 修复筛选"),
                        commit("latest", "historic latest")
                    ]
                )
            )
            .unwrap()
            .is_empty()
    );
    assert_eq!(store.snapshot().messages.len(), 1);
    assert_eq!(store.snapshot().messages[0].author, "Alice");
    assert_eq!(
        store.snapshot().projects[0].cursor.as_deref(),
        Some("latest")
    );
}
#[test]
fn own_success_does_not_skip_intervening_remote_commit() {
    let (_dir, _path, mut store) = setup();
    baseline(&mut store);
    store.record_receipt(receipt(true)).unwrap();
    let messages = store
        .apply_remote(
            "one",
            remote(
                "own",
                vec![
                    commit("other", "other teammate"),
                    commit("own", "fix(frontend): 商品页 - 修复筛选"),
                ],
            ),
        )
        .unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].sha, "other");
    assert_eq!(store.snapshot().messages.len(), 2);
}
#[test]
fn failed_pending_receipt_survives_restart_and_successful_retry_clears_it() {
    let (_dir, path, mut store) = setup();
    baseline(&mut store);
    store.record_receipt(receipt(false)).unwrap();
    assert!(store.snapshot().messages.is_empty());
    drop(store);
    let mut store = CollaborationStore::open(path).unwrap();
    assert_eq!(
        store.snapshot().projects[0]
            .pending
            .as_ref()
            .unwrap()
            .commit,
        "own"
    );
    assert_eq!(
        store.snapshot().projects[0].last_error.as_deref(),
        Some("网络失败")
    );
    store.record_receipt(receipt(true)).unwrap();
    assert!(store.snapshot().projects[0].pending.is_none());
    assert!(store.snapshot().projects[0].last_error.is_none());
    assert_eq!(store.snapshot().projects[0].cursor.as_deref(), Some("base"));
    assert_eq!(store.snapshot().messages.len(), 1);
}
#[test]
fn own_identity_is_repository_and_branch_scoped_and_survives_project_removal() {
    let (_dir, path, mut store) = setup();
    baseline(&mut store);
    store.record_receipt(receipt(true)).unwrap();
    store.remove_project("one").unwrap();
    drop(store);
    let mut store = CollaborationStore::open(path).unwrap();
    let mut same = project("new-id", "C:/projects/new");
    store.add_project(same.clone()).unwrap();
    store
        .apply_remote("new-id", remote("base", vec![]))
        .unwrap();
    assert!(
        store
            .apply_remote("new-id", remote("own", vec![commit("own", "own commit")]))
            .unwrap()
            .is_empty()
    );
    assert_eq!(store.snapshot().messages.len(), 1);
    same.id = "other-branch".into();
    same.root = "C:/projects/other".into();
    same.branch = "dev".into();
    store.add_project(same).unwrap();
    store
        .apply_remote("other-branch", remote("base", vec![]))
        .unwrap();
    assert_eq!(
        store
            .apply_remote(
                "other-branch",
                remote("own", vec![commit("own", "same SHA in dev")])
            )
            .unwrap()
            .len(),
        1
    );
    let mut other_repo = project("other-repo", "C:/projects/another");
    other_repo.repository = "owner/another".into();
    store.add_project(other_repo).unwrap();
    store
        .apply_remote("other-repo", remote("base", vec![]))
        .unwrap();
    assert_eq!(
        store
            .apply_remote(
                "other-repo",
                remote("own", vec![commit("own", "same SHA elsewhere")])
            )
            .unwrap()
            .len(),
        1
    );
}
#[test]
fn recording_error_preserves_cursor_messages_and_pending_and_success_clears_error() {
    let (_dir, path, mut store) = setup();
    baseline(&mut store);
    store.record_receipt(receipt(false)).unwrap();
    store.record_error("one", "权限丢失".into()).unwrap();
    assert_eq!(store.snapshot().projects[0].cursor.as_deref(), Some("base"));
    assert_eq!(
        store.snapshot().projects[0]
            .pending
            .as_ref()
            .unwrap()
            .commit,
        "own"
    );
    assert!(store.snapshot().messages.is_empty());
    drop(store);
    let mut store = CollaborationStore::open(path).unwrap();
    assert_eq!(
        store.snapshot().projects[0].last_error.as_deref(),
        Some("权限丢失")
    );
    store.apply_remote("one", remote("base", vec![])).unwrap();
    assert!(store.snapshot().projects[0].last_error.is_none());
    assert!(store.snapshot().projects[0].pending.is_some());
}
#[test]
fn rewrite_creates_one_distinct_warning_instead_of_normal_history() {
    let (_dir, path, mut store) = setup();
    baseline(&mut store);
    store
        .apply_remote("one", remote("new", vec![commit("new", "normal update")]))
        .unwrap();
    let snap = RemoteSnapshot {
        head: "rewrite".into(),
        commits: vec![
            commit("rewrite", "rewritten history"),
            commit("old", "old history"),
        ],
        rewritten: true,
    };
    let messages = store.apply_remote("one", snap.clone()).unwrap();
    assert_eq!(messages.len(), 1);
    assert!(messages[0].rewritten);
    assert_eq!(messages[0].title, "远端历史已变化");
    assert_eq!(messages[0].sha, "rewrite");
    assert!(messages[0].parsed.is_none());
    assert_eq!(store.snapshot().messages.len(), 2);
    assert_eq!(
        store.snapshot().projects[0].cursor.as_deref(),
        Some("rewrite")
    );
    drop(store);
    let mut store = CollaborationStore::open(path).unwrap();
    assert!(store.apply_remote("one", snap).unwrap().is_empty());
}
#[test]
fn remove_project_keeps_history_and_clears_selection() {
    let (_dir, path, mut store) = setup();
    baseline(&mut store);
    store
        .apply_remote("one", remote("new", vec![commit("new", "update")]))
        .unwrap();
    store.select(Some("one".into()), Role::Skills).unwrap();
    store.remove_project("one").unwrap();
    drop(store);
    let store = CollaborationStore::open(path).unwrap();
    assert!(store.snapshot().projects.is_empty());
    assert_eq!(store.snapshot().messages.len(), 1);
    assert!(store.snapshot().selected_project.is_none());
}
#[test]
fn duplicate_project_ids_and_normalized_roots_are_rejected_without_mutation() {
    let (_dir, _path, mut store) = setup();
    let before = json(&store);
    assert!(
        store
            .add_project(project("one", "C:/projects/different"))
            .is_err()
    );
    assert!(
        store
            .add_project(project("two", "C:/projects/one/"))
            .is_err()
    );
    #[cfg(windows)]
    assert!(
        store
            .add_project(project("two", "c:/PROJECTS/one"))
            .is_err()
    );
    assert_eq!(json(&store), before);
}
#[test]
fn duplicate_existing_root_alias_is_rejected_on_each_platform() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("repository");
    fs::create_dir(&root).unwrap();
    let mut store = CollaborationStore::open(dir.path().join("state.json")).unwrap();
    store
        .add_project(project("one", root.to_str().unwrap()))
        .unwrap();
    let before = json(&store);
    assert!(
        store
            .add_project(project("two", root.join(".").to_str().unwrap()))
            .is_err()
    );
    assert_eq!(json(&store), before);
}
#[cfg(unix)]
#[test]
fn unix_backslash_in_root_is_distinct_from_a_directory_separator() {
    let dir = tempfile::tempdir().unwrap();
    let literal = dir.path().join("repository\\name");
    let nested = dir.path().join("repository/name");
    fs::create_dir(&literal).unwrap();
    fs::create_dir_all(&nested).unwrap();
    let mut store = CollaborationStore::open(dir.path().join("state.json")).unwrap();
    store
        .add_project(project("one", literal.to_str().unwrap()))
        .unwrap();
    store
        .add_project(project("two", nested.to_str().unwrap()))
        .unwrap();
    assert_eq!(store.snapshot().projects.len(), 2);
}
#[test]
fn missing_selection_clears_project_but_keeps_chosen_role() {
    let (_dir, path, mut store) = setup();
    store.select(Some("missing".into()), Role::Testing).unwrap();
    drop(store);
    let store = CollaborationStore::open(path).unwrap();
    assert!(store.snapshot().selected_project.is_none());
    assert_eq!(store.snapshot().selected_role, Role::Testing);
}
#[test]
fn malformed_json_and_unknown_schema_are_actionable_and_preserve_original_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    for original in ["{broken JSON", r#"{"schemaVersion":999}"#] {
        fs::write(&path, original).unwrap();
        let err = CollaborationStore::open(path.clone())
            .err()
            .expect("invalid state must fail");
        assert!(err.message.contains("state.json"));
        assert!(err.message.contains("备份"));
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
    }
}
#[test]
fn second_store_open_is_locked_until_owner_is_dropped() {
    let (_dir, path, store) = setup();
    let err = CollaborationStore::open(path.clone())
        .err()
        .expect("concurrent open must fail");
    assert_eq!(err.code, "store_locked");
    drop(store);
    assert!(CollaborationStore::open(path).is_ok());
}
#[test]
fn failed_atomic_replace_keeps_cursor_messages_and_settings_unchanged() {
    // A directory at the JSON target makes actual tempfile replacement fail.
    let (_dir, path, mut store) = setup();
    baseline(&mut store);
    let before = json(&store);
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(
        store
            .apply_remote("one", remote("new", vec![commit("new", "must retry")]))
            .is_err()
    );
    assert_eq!(json(&store), before);
    assert!(store.set_notifications(true).is_err());
    assert_eq!(json(&store), before);
    assert!(store.record_receipt(receipt(true)).is_err());
    assert_eq!(json(&store), before);
    fs::remove_dir(&path).unwrap();
    assert_eq!(
        store
            .apply_remote("one", remote("new", vec![commit("new", "must retry")]))
            .unwrap()
            .len(),
        1
    );
    assert_eq!(store.snapshot().projects[0].cursor.as_deref(), Some("new"));
}
#[test]
fn unknown_project_operations_do_not_modify_existing_state() {
    let (_dir, _path, mut store) = setup();
    let before = json(&store);
    assert!(store.remove_project("missing").is_err());
    assert!(store.set_monitor("missing", false).is_err());
    assert!(store.record_error("missing", "x".into()).is_err());
    assert!(
        store
            .apply_remote("missing", remote("new", vec![]))
            .is_err()
    );
    let mut missing = receipt(false);
    missing.project_id = "missing".into();
    assert!(store.record_receipt(missing).is_err());
    assert_eq!(json(&store), before);
}

#[test]
fn failed_push_ownership_survives_restart_when_remote_actually_received_commit() {
    let (_dir, path, mut store) = setup();
    baseline(&mut store);
    store.record_receipt(receipt(false)).unwrap();
    drop(store);
    let mut store = CollaborationStore::open(path).unwrap();
    let messages = store
        .apply_remote(
            "one",
            remote(
                "own",
                vec![commit("own", "fix(frontend): 商品页 - 修复筛选")],
            ),
        )
        .unwrap();
    assert!(
        messages.is_empty(),
        "a lost push acknowledgement must not turn our commit into a teammate notification"
    );
    assert_eq!(store.snapshot().messages.len(), 1);
    assert_eq!(store.snapshot().messages[0].author, "Alice");
    assert!(store.snapshot().messages[0].read);
    assert!(
        store.snapshot().projects[0].pending.is_none(),
        "positive remote proof must resolve the matching retry receipt"
    );
}
#[test]
fn older_successful_receipt_does_not_clear_a_different_failed_retry() {
    let (_dir, _path, mut store) = setup();
    store.record_receipt(receipt(true)).unwrap();
    let mut failed = receipt(false);
    failed.commit = "own-next".into();
    store.record_receipt(failed).unwrap();
    store.record_receipt(receipt(true)).unwrap();
    assert_eq!(
        store.snapshot().projects[0]
            .pending
            .as_ref()
            .map(|r| r.commit.as_str()),
        Some("own-next")
    );
    assert_eq!(
        store.snapshot().projects[0].last_error.as_deref(),
        Some("网络失败")
    );
    assert_eq!(store.snapshot().messages.len(), 1);
}

#[test]
fn first_baseline_acknowledges_failed_own_commit_and_skips_other_history() {
    let (_dir, path, mut store) = setup();
    store.record_receipt(receipt(false)).unwrap();
    drop(store);
    let mut store = CollaborationStore::open(path.clone()).unwrap();
    let notifications = store
        .apply_remote(
            "one",
            remote(
                "latest",
                vec![
                    commit("old", "old teammate history"),
                    commit("own", "fix(frontend): 商品页 - 修复筛选"),
                    commit("latest", "later teammate history"),
                ],
            ),
        )
        .unwrap();
    assert!(notifications.is_empty());
    let snapshot = store.snapshot();
    assert_eq!(snapshot.messages.len(), 1);
    assert_eq!(snapshot.messages[0].sha, "own");
    assert_eq!(snapshot.messages[0].author, "Alice");
    assert_eq!(
        snapshot.messages[0].committed_at,
        "2026-10-07T10:00:00+08:00"
    );
    assert!(snapshot.messages[0].read);
    assert_eq!(
        snapshot.messages[0].parsed.as_ref().unwrap().role,
        Role::Frontend
    );
    assert!(snapshot.projects[0].pending.is_none());
    assert_eq!(snapshot.projects[0].cursor.as_deref(), Some("latest"));
    drop(store);
    let store = CollaborationStore::open(path).unwrap();
    assert_eq!(store.snapshot().messages[0].author, "Alice");
    assert!(store.snapshot().projects[0].pending.is_none());
}
#[test]
fn head_only_acknowledgement_materializes_receipt_then_hydrates_same_record() {
    let (_dir, path, mut store) = setup();
    store.record_receipt(receipt(false)).unwrap();
    drop(store);
    let mut store = CollaborationStore::open(path.clone()).unwrap();
    assert!(
        store
            .apply_remote("one", remote("own", vec![]))
            .unwrap()
            .is_empty()
    );
    let snapshot = store.snapshot();
    assert_eq!(snapshot.messages.len(), 1);
    let id = snapshot.messages[0].id.clone();
    assert_eq!(snapshot.messages[0].sha, "own");
    assert_eq!(
        snapshot.messages[0].title,
        "fix(frontend): 商品页 - 修复筛选"
    );
    assert_eq!(
        snapshot.messages[0].url,
        "https://github.com/owner/repo/commit/own"
    );
    assert_eq!(snapshot.messages[0].author, "");
    assert_eq!(snapshot.messages[0].committed_at, "");
    assert!(snapshot.messages[0].read);
    assert!(snapshot.projects[0].pending.is_none());
    assert_eq!(snapshot.projects[0].cursor.as_deref(), Some("own"));
    drop(store);
    let mut store = CollaborationStore::open(path).unwrap();
    assert!(
        store
            .apply_remote(
                "one",
                remote(
                    "own",
                    vec![commit("own", "fix(frontend): 商品页 - 修复筛选")]
                )
            )
            .unwrap()
            .is_empty()
    );
    let snapshot = store.snapshot();
    assert_eq!(snapshot.messages.len(), 1);
    assert_eq!(snapshot.messages[0].id, id);
    assert_eq!(snapshot.messages[0].author, "Alice");
    assert!(snapshot.messages[0].read);
}
#[test]
fn post_baseline_commit_list_resolves_pending_even_when_head_has_advanced() {
    let (_dir, _path, mut store) = setup();
    baseline(&mut store);
    store.record_receipt(receipt(false)).unwrap();
    let notifications = store
        .apply_remote(
            "one",
            remote(
                "latest",
                vec![
                    commit("own", "fix(frontend): 商品页 - 修复筛选"),
                    commit("latest", "teammate update"),
                ],
            ),
        )
        .unwrap();
    assert_eq!(notifications.len(), 1);
    assert_eq!(notifications[0].sha, "latest");
    assert_eq!(store.snapshot().messages.len(), 2);
    assert!(store.snapshot().projects[0].pending.is_none());
    assert!(store.snapshot().projects[0].last_error.is_none());
    assert_eq!(
        store.snapshot().projects[0].cursor.as_deref(),
        Some("latest")
    );
}
#[test]
fn first_baseline_recovers_older_owned_commit_without_clearing_newer_pending() {
    let (_dir, path, mut store) = setup();
    store.record_receipt(receipt(false)).unwrap();
    let mut newer = receipt(false);
    newer.commit = "own-next".into();
    newer.error = Some("newer push failed".into());
    store.record_receipt(newer).unwrap();
    store
        .record_error("one", "stale monitoring error".into())
        .unwrap();
    drop(store);
    let mut store = CollaborationStore::open(path).unwrap();
    assert!(
        store
            .apply_remote(
                "one",
                remote(
                    "own",
                    vec![commit("own", "fix(frontend): 商品页 - 修复筛选")]
                )
            )
            .unwrap()
            .is_empty()
    );
    let snapshot = store.snapshot();
    assert_eq!(snapshot.messages.len(), 1);
    assert_eq!(snapshot.messages[0].sha, "own");
    assert_eq!(snapshot.messages[0].author, "Alice");
    assert!(snapshot.messages[0].read);
    let id = snapshot.messages[0].id.clone();
    let pending = snapshot.projects[0].pending.as_ref().unwrap();
    assert_eq!(pending.commit, "own-next");
    assert_eq!(pending.error.as_deref(), Some("newer push failed"));
    assert!(snapshot.projects[0].last_error.is_none());
    assert!(
        store
            .apply_remote(
                "one",
                remote(
                    "own",
                    vec![commit("own", "fix(frontend): 商品页 - 修复筛选")]
                )
            )
            .unwrap()
            .is_empty()
    );
    assert_eq!(store.snapshot().messages[0].id, id);
    assert!(store.snapshot().messages[0].read);
    assert_eq!(
        store.snapshot().projects[0]
            .pending
            .as_ref()
            .unwrap()
            .commit,
        "own-next"
    );
}
#[test]
fn failed_save_does_not_acknowledge_pending_or_advance_first_baseline() {
    let (_dir, path, mut store) = setup();
    store.record_receipt(receipt(false)).unwrap();
    let before = json(&store);
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(store.apply_remote("one", remote("own", vec![])).is_err());
    assert_eq!(json(&store), before);
    fs::remove_dir(&path).unwrap();
    assert!(
        store
            .apply_remote("one", remote("own", vec![]))
            .unwrap()
            .is_empty()
    );
    assert_eq!(store.snapshot().messages.len(), 1);
    assert!(store.snapshot().projects[0].pending.is_none());
    assert_eq!(store.snapshot().projects[0].cursor.as_deref(), Some("own"));
}

#[test]
fn final_review_snapshots_have_durable_monotonic_mutation_revision() {
    let (_dir, path, mut store) = setup();
    let first = json(&store)["revision"]
        .as_u64()
        .expect("snapshot has authoritative revision");
    store.set_notifications(true).unwrap();
    let second = json(&store)["revision"].as_u64().unwrap();
    assert!(second > first);
    drop(store);
    let mut store = CollaborationStore::open(path).unwrap();
    assert_eq!(json(&store)["revision"], second);
    store.mark_read(&[]).unwrap();
    assert!(json(&store)["revision"].as_u64().unwrap() > second);
}

#[test]
fn final_review_known_success_is_not_downgraded_by_older_repository_proof() {
    let (_dir, _path, mut store) = setup();
    store.record_receipt(receipt(true)).unwrap();
    // A failed final repository rewrite can leave a pre-push proof on disk,
    // while the app Store already has the acknowledged successful outcome.
    let returned = store.record_delivery(receipt(false));
    assert!(returned.pushed);
    assert!(returned.error.is_none());
    assert!(store.snapshot().projects[0].pending.is_none());
    assert!(store.snapshot().messages[0].own);
}

#[test]
fn local_receipt_is_durable_without_pending_and_old_default_is_push_requested() {
    let (_dir, path, mut store) = setup();
    let mut r = receipt(false);
    let mut old = serde_json::to_value(&r).unwrap();
    old.as_object_mut().unwrap().remove("pushRequested");
    assert!(
        serde_json::from_value::<PublishReceipt>(old)
            .unwrap()
            .push_requested
    );
    r.push_requested = false;
    r.error = None;
    store.record_receipt(r.clone()).unwrap();
    assert!(store.snapshot().projects[0].pending.is_none());
    assert!(store.snapshot().messages.is_empty());
    r.commit = "second".into();
    store.record_receipt(r.clone()).unwrap();
    drop(store);
    let store = CollaborationStore::open(path).unwrap();
    assert_eq!(store.snapshot().projects[0].last_receipt.as_ref(), Some(&r));
}

#[test]
fn double_clone_dedup_keeps_per_project_success_after_reopen() {
    let (_dir, path, mut store) = setup();
    store
        .add_project(project("two", "C:/projects/two"))
        .unwrap();
    store.record_receipt(receipt(false)).unwrap();
    store
        .apply_remote(
            "two",
            remote("own", vec![commit("own", &receipt(true).message)]),
        )
        .unwrap();
    let id = store.snapshot().messages[0].id.clone();
    store.mark_read(&[id]).unwrap();
    let r = store.reconcile_receipt(receipt(false)).unwrap();
    assert!(r.pushed);
    store.apply_remote("one", remote("own", vec![])).unwrap();
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

fn local_push_receipt(pushed: bool) -> LocalPushReceipt {
    LocalPushReceipt {
        project_id: "one".into(),
        head: "ordinary".into(),
        remote_head: Some("base".into()),
        url: "https://github.com/owner/repo/commit/ordinary".into(),
        commits: vec![LocalCommit {
            sha: "ordinary".into(),
            parents: vec!["base".into()],
            title: "ordinary title".into(),
            body: "body\n".into(),
            author: "Author".into(),
            committed_at: "2026-10-07T10:00:00+08:00".into(),
            parsed: None,
        }],
        pushed,
        error: (!pushed).then(|| "network failed".into()),
        persistence_warning: None,
    }
}

#[test]
fn local_push_store_preserves_success_and_failed_save_revision() {
    let (_dir, path, mut store) = setup();
    let failed = local_push_receipt(false);
    store.reconcile_local_push(failed.clone()).unwrap();
    assert!(
        !store.snapshot().projects[0]
            .last_local_push
            .as_ref()
            .unwrap()
            .pushed
    );
    store
        .apply_remote("one", remote("ordinary", vec![]))
        .unwrap();
    assert!(store.reconcile_local_push(failed.clone()).unwrap().pushed);
    let before = json(&store);
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    let known = store.record_local_push(local_push_receipt(true));
    assert!(known.pushed && known.persistence_warning.is_some());
    assert_eq!(json(&store), before);
    fs::remove_dir(&path).unwrap();
    store.reconcile_local_push(failed).unwrap();
    drop(store);
    let store = CollaborationStore::open(path).unwrap();
    assert!(
        store.snapshot().projects[0]
            .last_local_push
            .as_ref()
            .unwrap()
            .pushed
    );
    assert_eq!(store.snapshot().messages.len(), 1);
}

#[test]
fn role_paths_store_update_preserves_history_and_retries_failed_save() {
    let (_dir, path, mut store) = setup();
    baseline(&mut store);
    store.record_receipt(receipt(true)).unwrap();
    store
        .reconcile_local_push(local_push_receipt(true))
        .unwrap();
    store.set_monitor("one", false).unwrap();
    let before = store.snapshot();
    let mut p = before.projects[0].project.clone();
    p.role_paths.insert(
        Role::Frontend,
        vec!["apps/web".into(), "packages/ui".into()],
    );
    p.monitor_enabled = true;
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(store.update_project_role_paths(p.clone()).is_err());
    assert_eq!(store.snapshot().revision, before.revision);
    fs::remove_dir(&path).unwrap();
    store.update_project_role_paths(p.clone()).unwrap();
    store.update_project_role_paths(p).unwrap();
    let after = store.snapshot();
    assert!(!after.projects[0].project.monitor_enabled);
    assert_eq!(after.projects[0].cursor, before.projects[0].cursor);
    assert_eq!(
        after.projects[0].last_receipt,
        before.projects[0].last_receipt
    );
    assert_eq!(
        after.projects[0].last_local_push,
        before.projects[0].last_local_push
    );
    assert_eq!(
        serde_json::to_value(after.messages).unwrap(),
        serde_json::to_value(before.messages).unwrap()
    );
}

#[test]
fn local_push_head_only_remote_ack_materializes_full_range_once() {
    let (_dir, path, mut store) = setup();
    let mut r = local_push_receipt(false);
    let mut first = r.commits[0].clone();
    first.sha = "first".into();
    r.commits.insert(0, first);
    store.reconcile_local_push(r).unwrap();
    store
        .apply_remote("one", remote("ordinary", vec![]))
        .unwrap();
    assert!(
        store.snapshot().projects[0]
            .last_local_push
            .as_ref()
            .unwrap()
            .pushed
    );
    assert_eq!(store.snapshot().messages.len(), 2);
    store
        .apply_remote("one", remote("ordinary", vec![]))
        .unwrap();
    drop(store);
    let store = CollaborationStore::open(path).unwrap();
    assert_eq!(store.snapshot().messages.len(), 2);
}

#[test]
fn local_push_known_store_success_survives_failed_reconcile_save() {
    let (_dir, path, mut store) = setup();
    store
        .reconcile_local_push(local_push_receipt(true))
        .unwrap();
    store.reconcile_receipt(receipt(true)).unwrap();
    let before = json(&store);
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    let r = store.record_local_push(local_push_receipt(false));
    assert!(r.pushed && r.error.is_none() && r.persistence_warning.is_some());
    let r = store.record_delivery(receipt(false));
    assert!(r.pushed && r.error.is_none() && r.persistence_warning.is_some());
    assert_eq!(json(&store), before);
}

#[test]
fn role_paths_store_update_preserves_pending_and_rejects_binding_change() {
    let (_dir, _path, mut store) = setup();
    store.record_receipt(receipt(false)).unwrap();
    let before = store.snapshot();
    let mut p = before.projects[0].project.clone();
    p.role_paths.insert(Role::Frontend, vec!["web".into()]);
    store.update_project_role_paths(p.clone()).unwrap();
    assert_eq!(
        store.snapshot().projects[0].pending,
        before.projects[0].pending
    );
    p.remote_url = "https://github.com/other/repo.git".into();
    let before = json(&store);
    assert!(store.update_project_role_paths(p).is_err());
    assert_eq!(json(&store), before);
}

#[test]
fn review_head_only_range_ack_upgrades_covered_role_and_pending() {
    for push_requested in [false, true] {
        let (_dir, path, mut store) = setup();
        let mut role = receipt(false);
        role.push_requested = push_requested;
        if !push_requested {
            role.error = None;
        }
        store.reconcile_receipt(role.clone()).unwrap();
        let mut range = local_push_receipt(false);
        let mut first = range.commits[0].clone();
        first.sha = role.commit.clone();
        first.title = role.message.clone();
        first.parsed = parse_commit(&role.message);
        range.commits.insert(0, first);
        store.reconcile_local_push(range).unwrap();
        store
            .apply_remote("one", remote("ordinary", vec![]))
            .unwrap();
        let state = &store.snapshot().projects[0];
        assert!(state.last_local_push.as_ref().unwrap().pushed);
        assert!(
            state.last_receipt.as_ref().unwrap().pushed,
            "covered role must share range success"
        );
        assert_eq!(
            state.last_receipt.as_ref().unwrap().push_requested,
            push_requested
        );
        assert!(state.pending.is_none());
        assert_eq!(store.snapshot().messages.len(), 2);
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
    }
}

#[test]
fn review_head_only_range_ack_preserves_unrelated_pending() {
    let (_dir, path, mut store) = setup();
    let unrelated = receipt(false);
    store.reconcile_receipt(unrelated.clone()).unwrap();
    store
        .reconcile_local_push(local_push_receipt(false))
        .unwrap();
    store
        .apply_remote("one", remote("ordinary", vec![]))
        .unwrap();
    let state = &store.snapshot().projects[0];
    assert!(state.last_local_push.as_ref().unwrap().pushed);
    assert_eq!(state.pending.as_ref(), Some(&unrelated));
    assert_eq!(state.last_receipt.as_ref(), Some(&unrelated));
    drop(store);
    let store = CollaborationStore::open(path).unwrap();
    assert_eq!(
        store.snapshot().projects[0].pending.as_ref(),
        Some(&unrelated)
    );
}

#[test]
fn review_head_only_range_ack_hydrates_existing_role_message() {
    let (_dir, _path, mut store) = setup();
    let role = receipt(true);
    let mut range = local_push_receipt(false);
    let mut first = range.commits[0].clone();
    first.sha = role.commit.clone();
    first.title = role.message.clone();
    first.parsed = parse_commit(&role.message);
    range.commits.insert(0, first.clone());
    store.reconcile_local_push(range).unwrap();
    // The role retry is acknowledged before the complete range head is known.
    store.reconcile_receipt(role.clone()).unwrap();
    let original = store.snapshot().messages[0].clone();
    assert!(original.author.is_empty());
    store
        .apply_remote("one", remote("ordinary", vec![]))
        .unwrap();
    let snapshot = store.snapshot();
    let message = snapshot
        .messages
        .iter()
        .find(|m| m.sha == role.commit)
        .unwrap();
    assert_eq!(message.author, first.author);
    assert_eq!(message.committed_at, first.committed_at);
    assert_eq!(message.id, original.id);
    assert_eq!(message.read, original.read);
}
