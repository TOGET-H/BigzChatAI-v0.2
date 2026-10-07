use collaboration_core::{ProjectState, PublishReceipt, Result, SyncError, UpdateMessage};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct NotificationBatch {
    pub title: String,
    pub body: String,
}

pub(crate) fn notification_batch(
    enabled: bool,
    messages: &[UpdateMessage],
) -> Option<NotificationBatch> {
    let count = messages.iter().filter(|message| !message.read).count();
    (enabled && count > 0).then(|| NotificationBatch {
        title: "Git 协作台更新".into(),
        body: format!("收到 {count} 条项目更新，请打开Git 协作台查看。"),
    })
}

#[derive(Default)]
pub(crate) struct ProjectLocks(Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>);
impl ProjectLocks {
    pub(crate) fn for_project(&self, id: &str) -> Arc<tokio::sync::Mutex<()>> {
        // Retain locks for removed IDs: queued operations must share the old lock
        // and re-read the store instead of resurrecting removed state.
        self.0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .entry(id.into())
            .or_default()
            .clone()
    }
}

pub(crate) fn commit_url(repository: &str, sha: &str) -> Result<String> {
    let parts: Vec<_> = repository.split('/').collect();
    if parts.len() != 2
        || parts.iter().any(|part| {
            part.is_empty()
                || *part == "."
                || *part == ".."
                || part.starts_with('-')
                || !part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "_- .".contains(c) && c != ' ')
        })
        || ![40, 64].contains(&sha.len())
        || !sha.bytes().all(|c| c.is_ascii_hexdigit())
    {
        return Err(SyncError {
            code: "invalid_commit".into(),
            message: "提交标识或 GitHub 仓库无效，请重新选择消息中的提交链接".into(),
        });
    }
    Ok(format!("https://github.com/{repository}/commit/{sha}"))
}

pub(crate) fn fetch_cursor(
    state: &ProjectState,
    receipt: Option<&PublishReceipt>,
) -> Option<String> {
    state.cursor.clone().or_else(|| {
        receipt
            .or(state.pending.as_ref())
            .filter(|receipt| !receipt.parent.is_empty())
            .map(|receipt| receipt.parent.clone())
    })
}

pub(crate) fn role_folder_relative(
    root: &std::path::Path,
    chosen: &std::path::Path,
) -> Result<String> {
    let invalid = || SyncError {
        code: "invalid_mapping".into(),
        message: "请选择仓库内的相对文件夹，不能选择根目录、.git 或仓库外目录".into(),
    };
    let root = root.canonicalize().map_err(|_| invalid())?;
    let chosen = chosen.canonicalize().map_err(|_| invalid())?;
    if !chosen.is_dir() {
        return Err(invalid());
    }
    let relative = chosen.strip_prefix(root).map_err(|_| invalid())?;
    if relative.as_os_str().is_empty()
        || relative.components().any(|part| {
            part.as_os_str()
                .to_string_lossy()
                .eq_ignore_ascii_case(".git")
        })
    {
        return Err(invalid());
    }
    relative
        .to_str()
        .map(|p| p.replace('\\', "/"))
        .ok_or_else(invalid)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(id: &str, read: bool) -> UpdateMessage {
        UpdateMessage {
            own: false,
            id: id.into(),
            project_id: "project".into(),
            repository: "owner/repo".into(),
            branch: "main".into(),
            sha: "a".repeat(40),
            title: "feat(frontend): 页面 - 新增导航".into(),
            author: "真实作者".into(),
            committed_at: "2026-10-07T12:00:00Z".into(),
            url: format!("https://github.com/owner/repo/commit/{}", "a".repeat(40)),
            parsed: None,
            read,
            rewritten: false,
        }
    }

    #[test]
    fn empty_batch_does_not_notify() {
        assert_eq!(notification_batch(true, &[]), None);
    }
    #[test]
    fn two_updates_produce_one_batch_notification() {
        let messages = vec![message("1", false), message("2", false)];
        let batch =
            notification_batch(true, &messages).expect("new updates need a single notification");
        assert!(batch.body.contains('2'));
    }
    #[test]
    fn disabled_notifications_preserve_message_list() {
        let messages = vec![message("1", false), message("2", false)];
        assert_eq!(notification_batch(false, &messages), None);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].id, "1");
    }
    #[test]
    fn own_or_read_commits_do_not_notify() {
        assert_eq!(notification_batch(true, &[message("own", true)]), None);
    }
    #[tokio::test]
    async fn same_project_waits_while_other_project_proceeds() {
        let locks = ProjectLocks::default();
        let first = locks.for_project("a");
        let held = first.lock().await;
        let same = locks.for_project("a");
        assert!(
            same.try_lock().is_err(),
            "same project must share its active lock"
        );
        let other = locks.for_project("b");
        assert!(
            other.try_lock().is_ok(),
            "unrelated projects must remain available"
        );
        drop(held);
        assert!(same.try_lock().is_ok());
    }

    #[test]
    fn commit_link_is_built_from_safe_github_identity_only() {
        assert_eq!(
            commit_url("owner/repo", &"a".repeat(40)).unwrap(),
            format!("https://github.com/owner/repo/commit/{}", "a".repeat(40))
        );
        for repository in [
            "owner/repo?token=x",
            "https://evil.test/o/r",
            "../r",
            "o/r/extra",
            "-o/r",
        ] {
            assert!(commit_url(repository, &"a".repeat(40)).is_err());
        }
        for sha in ["", "abc", "HEAD", "abc;start evil"] {
            assert!(commit_url("owner/repo", sha).is_err());
        }
    }

    #[test]
    fn no_baseline_uses_pending_parent_to_recover_own_metadata() {
        use collaboration_core::{Project, Role};
        let receipt = PublishReceipt {
            persistence_warning: None,
            project_id: "p".into(),
            role: Role::Product,
            commit: "b".repeat(40),
            parent: "a".repeat(40),
            message: "title".into(),
            url: "".into(),
            pushed: false,
            push_requested: true,
            error: Some("网络失败".into()),
        };
        let mut state = ProjectState {
            project: Project {
                id: "p".into(),
                name: "p".into(),
                root: "root".into(),
                repository: "o/r".into(),
                remote_url: "https://github.com/o/r".into(),
                branch: "main".into(),
                role_paths: Default::default(),
                monitor_enabled: true,
            },
            cursor: None,
            last_error: None,
            pending: Some(receipt.clone()),
            last_receipt: None,
            last_local_push: None,
        };
        assert_eq!(fetch_cursor(&state, None), Some("a".repeat(40)));
        state.pending = None;
        assert_eq!(fetch_cursor(&state, Some(&receipt)), Some("a".repeat(40)));
        state.cursor = Some("c".repeat(40));
        assert_eq!(fetch_cursor(&state, Some(&receipt)), Some("c".repeat(40)));
    }
    #[test]
    fn task3_folder_picker_returns_only_repository_relative_directories() {
        let root = std::env::temp_dir().join(format!(
            "gitcollab-picker-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("packages/ui")).unwrap();
        std::fs::create_dir(root.join(".git")).unwrap();
        assert_eq!(
            role_folder_relative(&root, &root.join("packages/ui")).unwrap(),
            "packages/ui"
        );
        for bad in [&root, &root.join(".git"), &std::env::temp_dir()] {
            assert!(role_folder_relative(&root, bad).is_err());
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
