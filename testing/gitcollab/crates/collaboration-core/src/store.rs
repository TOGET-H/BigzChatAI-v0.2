use crate::{
    LocalPushReceipt, ParsedCommit, Project, PublishReceipt, RemoteCommit, RemoteSnapshot, Result,
    Role, SyncError, parse_commit,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMessage {
    #[serde(default)]
    pub own: bool,
    pub id: String,
    pub project_id: String,
    pub repository: String,
    pub branch: String,
    pub sha: String,
    pub title: String,
    pub author: String,
    pub committed_at: String,
    pub url: String,
    pub parsed: Option<ParsedCommit>,
    pub read: bool,
    pub rewritten: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectState {
    pub project: Project,
    pub cursor: Option<String>,
    pub last_error: Option<String>,
    pub pending: Option<PublishReceipt>,
    #[serde(default)]
    pub last_receipt: Option<PublishReceipt>,
    #[serde(default)]
    pub last_local_push: Option<LocalPushReceipt>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    #[serde(default)]
    pub revision: u64,
    pub projects: Vec<ProjectState>,
    pub messages: Vec<UpdateMessage>,
    pub selected_project: Option<String>,
    pub selected_role: Role,
    pub system_notifications: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CommitIdentity {
    repository: String,
    branch: String,
    sha: String,
}
impl CommitIdentity {
    fn new(project: &Project, sha: &str) -> Self {
        Self {
            repository: project.repository.to_lowercase(),
            branch: project.branch.clone(),
            sha: sha.into(),
        }
    }
    fn matches(&self, message: &UpdateMessage) -> bool {
        !message.rewritten
            && self.repository == message.repository.to_lowercase()
            && self.branch == message.branch
            && self.sha == message.sha
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DurableState {
    schema_version: u32,
    snapshot: AppSnapshot,
    own_commits: BTreeSet<CommitIdentity>,
}
impl Default for DurableState {
    fn default() -> Self {
        Self {
            schema_version: 1,
            snapshot: AppSnapshot {
                revision: 0,
                projects: vec![],
                messages: vec![],
                selected_project: None,
                selected_role: Role::Product,
                system_notifications: false,
            },
            own_commits: BTreeSet::new(),
        }
    }
}

/// One process owns a store until it is dropped. All mutations commit to disk
/// before replacing memory, so a failed save can be safely retried.
pub struct CollaborationStore {
    path: PathBuf,
    state: DurableState,
    _lock: File,
}
impl CollaborationStore {
    pub fn open(path: PathBuf) -> Result<Self> {
        let path = if path.is_absolute() {
            path
        } else {
            std::env::current_dir()
                .map_err(|e| io_error("定位状态目录", &path, e))?
                .join(path)
        };
        let parent = path
            .parent()
            .ok_or_else(|| SyncError::new("store_io", "请指定有效状态文件路径"))?;
        fs::create_dir_all(parent).map_err(|e| io_error("创建状态目录", parent, e))?;
        // Canonicalize aliases before choosing the separate, stable lock file.
        let path = if path.exists() {
            fs::canonicalize(&path).map_err(|e| io_error("定位状态文件", &path, e))?
        } else {
            fs::canonicalize(parent)
                .map_err(|e| io_error("定位状态目录", parent, e))?
                .join(
                    path.file_name()
                        .ok_or_else(|| SyncError::new("store_io", "请指定状态文件名"))?,
                )
        };
        let mut lock_path = path.as_os_str().to_os_string();
        lock_path.push(".lock");
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(PathBuf::from(lock_path))
            .map_err(|e| io_error("打开状态锁", &path, e))?;
        lock.try_lock().map_err(|_| {
            SyncError::new(
                "store_locked",
                "状态文件已被其他进程占用或无法加锁，请退出其他协作端后重试",
            )
        })?;
        let existing = match fs::read(&path) {
            Ok(bytes) => Some(bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(io_error("读取状态文件", &path, e)),
        };
        let mut state: DurableState = match existing.as_ref() {
            Some(bytes) => {
                let value: serde_json::Value = serde_json::from_slice(bytes)
                    .map_err(|_| invalid_state(&path, "JSON 已损坏", "invalid_store"))?;
                if value
                    .get("schemaVersion")
                    .and_then(serde_json::Value::as_u64)
                    != Some(1)
                {
                    return Err(invalid_state(
                        &path,
                        "状态版本不受支持",
                        "unsupported_schema",
                    ));
                }
                serde_json::from_value(value)
                    .map_err(|_| invalid_state(&path, "状态结构不完整或无效", "invalid_store"))?
            }
            None => DurableState::default(),
        };
        for message in &mut state.snapshot.messages {
            message.own = state
                .own_commits
                .iter()
                .any(|identity| identity.matches(message));
        }
        let store = Self {
            path,
            state,
            _lock: lock,
        };
        if existing.is_none() {
            store.save(&store.state)?;
        }
        Ok(store)
    }
    pub fn snapshot(&self) -> AppSnapshot {
        self.state.snapshot.clone()
    }
    pub fn add_project(&mut self, project: Project) -> Result<()> {
        let mut candidate = self.state.clone();
        let root = normalized_root(&project.root);
        if candidate
            .snapshot
            .projects
            .iter()
            .any(|s| s.project.id == project.id || normalized_root(&s.project.root) == root)
        {
            return Err(SyncError::new(
                "duplicate_project",
                "项目 ID 或本地根目录已登记，请选择已有项目",
            ));
        }
        candidate.snapshot.projects.push(ProjectState {
            project,
            cursor: None,
            last_error: None,
            pending: None,
            last_receipt: None,
            last_local_push: None,
        });
        self.commit(candidate)
    }
    pub fn update_project_role_paths(&mut self, project: Project) -> Result<()> {
        crate::model::validate_role_paths(&project.role_paths)?;
        let mut candidate = self.state.clone();
        let index = project_index(&candidate, &project.id)?;
        let existing = &mut candidate.snapshot.projects[index].project;
        if existing.root != project.root
            || existing.repository != project.repository
            || existing.remote_url != project.remote_url
            || existing.branch != project.branch
        {
            return Err(SyncError::new(
                "project_mismatch",
                "目录配置与登记项目绑定不一致",
            ));
        }
        existing.role_paths = project.role_paths;
        self.commit(candidate)
    }
    pub fn remove_project(&mut self, id: &str) -> Result<()> {
        let mut candidate = self.state.clone();
        let index = project_index(&candidate, id)?;
        candidate.snapshot.projects.remove(index);
        if candidate.snapshot.selected_project.as_deref() == Some(id) {
            candidate.snapshot.selected_project = None;
        }
        // Historical messages and durable own identities deliberately remain.
        self.commit(candidate)
    }
    pub fn select(&mut self, project_id: Option<String>, role: Role) -> Result<()> {
        let mut candidate = self.state.clone();
        candidate.snapshot.selected_project = project_id.filter(|id| {
            candidate
                .snapshot
                .projects
                .iter()
                .any(|s| &s.project.id == id)
        });
        candidate.snapshot.selected_role = role;
        self.commit(candidate)
    }
    pub fn set_monitor(&mut self, id: &str, enabled: bool) -> Result<()> {
        let mut candidate = self.state.clone();
        let index = project_index(&candidate, id)?;
        candidate.snapshot.projects[index].project.monitor_enabled = enabled;
        self.commit(candidate)
    }
    pub fn set_notifications(&mut self, enabled: bool) -> Result<()> {
        let mut candidate = self.state.clone();
        candidate.snapshot.system_notifications = enabled;
        self.commit(candidate)
    }
    pub fn mark_read(&mut self, ids: &[String]) -> Result<()> {
        let mut candidate = self.state.clone();
        for message in &mut candidate.snapshot.messages {
            if ids.contains(&message.id) {
                message.read = true;
            }
        }
        self.commit(candidate)
    }
    pub fn record_receipt(&mut self, receipt: PublishReceipt) -> Result<()> {
        self.reconcile_receipt(receipt).map(|_| ())
    }
    fn effective_receipt(&self, mut receipt: PublishReceipt) -> PublishReceipt {
        if let Some(state) = self
            .state
            .snapshot
            .projects
            .iter()
            .find(|s| s.project.id == receipt.project_id)
        {
            let identity = CommitIdentity::new(&state.project, &receipt.commit);
            if state
                .last_receipt
                .as_ref()
                .is_some_and(|r| r.commit == receipt.commit && r.pushed)
                || self
                    .state
                    .snapshot
                    .messages
                    .iter()
                    .any(|m| identity.matches(m))
            {
                receipt.pushed = true;
                receipt.error = None;
            }
        }
        receipt
    }
    pub fn reconcile_receipt(&mut self, receipt: PublishReceipt) -> Result<PublishReceipt> {
        let receipt = self.effective_receipt(receipt);
        let mut candidate = self.state.clone();
        let index = project_index(&candidate, &receipt.project_id)?;
        let project = candidate.snapshot.projects[index].project.clone();
        let identity = CommitIdentity::new(&project, &receipt.commit);
        // Even a failed push created our local commit. Remember ownership if a
        // lost network acknowledgement later exposes that commit remotely.
        candidate.own_commits.insert(identity.clone());
        for message in &mut candidate.snapshot.messages {
            if identity.matches(message) {
                message.own = true;
            }
        }
        if receipt.pushed {
            if !candidate
                .snapshot
                .messages
                .iter()
                .any(|m| identity.matches(m))
            {
                candidate
                    .snapshot
                    .messages
                    .push(receipt_message(&project, &receipt));
            }
            let state = &mut candidate.snapshot.projects[index];
            if state
                .pending
                .as_ref()
                .is_none_or(|pending| pending.commit == receipt.commit)
            {
                state.pending = None;
                state.last_error = receipt.persistence_warning.clone();
            }
        } else if receipt.push_requested {
            let state = &mut candidate.snapshot.projects[index];
            state.last_error = receipt.error.clone();
            state.pending = Some(receipt.clone());
        } else {
            candidate.snapshot.projects[index].last_error = receipt
                .error
                .clone()
                .or(receipt.persistence_warning.clone());
        }
        let state = &mut candidate.snapshot.projects[index];
        // Reconciling an older result must not hide a different active failure.
        if state
            .pending
            .as_ref()
            .is_none_or(|p| p.commit == receipt.commit)
        {
            state.last_receipt = Some(receipt.clone());
        }
        // A receipt never acknowledges monitoring history or advances cursor.
        self.commit(candidate)?;
        Ok(receipt)
    }
    pub fn record_delivery(&mut self, receipt: PublishReceipt) -> PublishReceipt {
        let mut receipt = self.effective_receipt(receipt);
        let failure = match self.reconcile_receipt(receipt.clone()) {
            Ok(saved) => return saved,
            Err(failure) => failure,
        };
        {
            let warning = format!(
                "交付结果已知，但应用状态保存失败；请恢复目录权限后检查远端或重启以恢复 SHA {}：{}",
                receipt.commit, failure.message
            );
            receipt.persistence_warning = Some(match receipt.persistence_warning.take() {
                Some(previous) => format!("{previous}；{warning}"),
                None => warning,
            });
        }
        receipt
    }
    fn effective_local_push(&self, mut receipt: LocalPushReceipt) -> LocalPushReceipt {
        if let Some(state) = self
            .state
            .snapshot
            .projects
            .iter()
            .find(|s| s.project.id == receipt.project_id)
        {
            let identity = CommitIdentity::new(&state.project, &receipt.head);
            if state
                .last_local_push
                .as_ref()
                .is_some_and(|r| r.head == receipt.head && r.pushed)
                || self
                    .state
                    .snapshot
                    .messages
                    .iter()
                    .any(|m| identity.matches(m))
            {
                receipt.pushed = true;
                receipt.error = None;
            }
        }
        receipt
    }
    pub fn reconcile_local_push(&mut self, receipt: LocalPushReceipt) -> Result<LocalPushReceipt> {
        let receipt = self.effective_local_push(receipt);
        let mut candidate = self.state.clone();
        let index = project_index(&candidate, &receipt.project_id)?;
        let project = candidate.snapshot.projects[index].project.clone();
        for commit in &receipt.commits {
            let identity = CommitIdentity::new(&project, &commit.sha);
            let metadata = RemoteCommit {
                sha: commit.sha.clone(),
                title: commit.title.clone(),
                author: commit.author.clone(),
                committed_at: commit.committed_at.clone(),
            };
            candidate.own_commits.insert(identity.clone());
            if let Some(message) = candidate
                .snapshot
                .messages
                .iter_mut()
                .find(|m| identity.matches(m))
            {
                message.own = true;
                // Role reconciliation may have created a title-only message first.
                // The validated range supplies real metadata without replacing read/id.
                hydrate(message, &metadata);
            } else if receipt.pushed {
                candidate
                    .snapshot
                    .messages
                    .push(remote_message(&project, &metadata, true));
            }
        }
        let state = &mut candidate.snapshot.projects[index];
        apply_covered_outcome(state, &receipt);
        state.last_error = receipt
            .error
            .clone()
            .or(receipt.persistence_warning.clone());
        state.last_local_push = Some(receipt.clone());
        self.commit(candidate)?;
        Ok(receipt)
    }
    pub fn record_local_push(&mut self, receipt: LocalPushReceipt) -> LocalPushReceipt {
        let mut receipt = self.effective_local_push(receipt);
        match self.reconcile_local_push(receipt.clone()) {
            Ok(saved) => saved,
            Err(failure) => {
                let warning = format!(
                    "推送结果已知，但应用状态保存失败；请恢复权限后检查或重启以恢复 SHA {}：{}",
                    receipt.head, failure.message
                );
                receipt.persistence_warning = Some(match receipt.persistence_warning.take() {
                    Some(previous) => format!("{previous}；{warning}"),
                    None => warning,
                });
                receipt
            }
        }
    }
    pub fn apply_remote(&mut self, id: &str, remote: RemoteSnapshot) -> Result<Vec<UpdateMessage>> {
        let mut candidate = self.state.clone();
        let index = project_index(&candidate, id)?;
        let project = candidate.snapshot.projects[index].project.clone();
        let previous = candidate.snapshot.projects[index].cursor.clone();
        let baseline = previous.is_none();
        let mut notifications = Vec::new();
        for remote_commit in &remote.commits {
            let identity = CommitIdentity::new(&project, &remote_commit.sha);
            let own = candidate.own_commits.contains(&identity);
            if let Some(message) = candidate
                .snapshot
                .messages
                .iter_mut()
                .find(|m| identity.matches(m))
            {
                message.own = own;
                if own || !baseline {
                    hydrate(message, remote_commit);
                }
                continue;
            }
            if (baseline || remote.rewritten) && !own {
                continue;
            }
            let message = remote_message(&project, remote_commit, own);
            candidate.snapshot.messages.push(message.clone());
            if !own {
                notifications.push(message);
            }
        }
        if !baseline && remote.rewritten && previous.as_deref() != Some(&remote.head) {
            let message = UpdateMessage {
                own: false,
                id: uuid::Uuid::new_v4().to_string(),
                project_id: project.id.clone(),
                repository: project.repository.clone(),
                branch: project.branch.clone(),
                sha: remote.head.clone(),
                title: "远端历史已变化".into(),
                author: String::new(),
                committed_at: String::new(),
                url: commit_url(&project, &remote.head),
                parsed: None,
                read: false,
                rewritten: true,
            };
            candidate.snapshot.messages.push(message.clone());
            notifications.push(message);
        }
        // The fetched head or a commit in the complete remote history is
        // positive delivery proof, even when the push acknowledgement was lost.
        // Ownership also checks repository/branch; project_id binds the receipt.
        let acknowledged = candidate.snapshot.projects[index]
            .pending
            .clone()
            .filter(|pending| {
                pending.project_id == project.id
                    && candidate
                        .own_commits
                        .contains(&CommitIdentity::new(&project, &pending.commit))
                    && (remote.head == pending.commit
                        || remote
                            .commits
                            .iter()
                            .any(|commit| commit.sha == pending.commit))
            });
        if let Some(mut receipt) = acknowledged {
            receipt.pushed = true;
            receipt.error = None;
            let identity = CommitIdentity::new(&project, &receipt.commit);
            if !candidate
                .snapshot
                .messages
                .iter()
                .any(|message| identity.matches(message))
            {
                // Head-only proof has no author/time. Preserve the receipt title
                // and leave metadata empty until a RemoteCommit can hydrate it.
                candidate
                    .snapshot
                    .messages
                    .push(receipt_message(&project, &receipt));
            }
            candidate.snapshot.projects[index].pending = None;
            candidate.snapshot.projects[index].last_receipt = Some(receipt);
        }
        let confirms =
            |sha: &str| remote.head == sha || remote.commits.iter().any(|c| c.sha == sha);
        let state = &mut candidate.snapshot.projects[index];
        if let Some(receipt) = &mut state.last_receipt {
            if confirms(&receipt.commit) {
                receipt.pushed = true;
                receipt.error = None;
            }
        }
        if let Some(receipt) = &mut state.last_local_push {
            if confirms(&receipt.head) {
                receipt.pushed = true;
                receipt.error = None;
            }
        }
        let delivered = state
            .last_local_push
            .as_ref()
            .filter(|r| r.pushed && confirms(&r.head))
            .cloned();
        if let Some(receipt) = &delivered {
            apply_covered_outcome(state, receipt);
        }
        let delivered_role = state
            .last_receipt
            .as_ref()
            .filter(|r| r.pushed && confirms(&r.commit))
            .cloned();
        if let Some(receipt) = delivered {
            for commit in receipt.commits {
                let identity = CommitIdentity::new(&project, &commit.sha);
                let metadata = RemoteCommit {
                    sha: commit.sha,
                    title: commit.title,
                    author: commit.author,
                    committed_at: commit.committed_at,
                };
                if let Some(message) = candidate
                    .snapshot
                    .messages
                    .iter_mut()
                    .find(|m| identity.matches(m))
                {
                    message.own = true;
                    hydrate(message, &metadata);
                } else {
                    candidate
                        .snapshot
                        .messages
                        .push(remote_message(&project, &metadata, true));
                }
            }
        }
        if let Some(receipt) = delivered_role {
            let identity = CommitIdentity::new(&project, &receipt.commit);
            if !candidate
                .snapshot
                .messages
                .iter()
                .any(|m| identity.matches(m))
            {
                candidate
                    .snapshot
                    .messages
                    .push(receipt_message(&project, &receipt));
            }
        }
        candidate.snapshot.projects[index].cursor = Some(remote.head);
        candidate.snapshot.projects[index].last_error = None;
        self.commit(candidate)?;
        Ok(notifications)
    }
    pub fn record_error(&mut self, id: &str, message: String) -> Result<()> {
        let mut candidate = self.state.clone();
        let index = project_index(&candidate, id)?;
        candidate.snapshot.projects[index].last_error = Some(message);
        self.commit(candidate)
    }
    fn commit(&mut self, mut candidate: DurableState) -> Result<()> {
        candidate.snapshot.revision = self
            .state
            .snapshot
            .revision
            .checked_add(1)
            .filter(|value| *value <= 9_007_199_254_740_991)
            .ok_or_else(|| {
                SyncError::new("revision_exhausted", "状态版本已达到上限，请升级协作端")
            })?;
        self.save(&candidate)?;
        self.state = candidate;
        Ok(())
    }
    fn save(&self, candidate: &DurableState) -> Result<()> {
        let parent = self.path.parent().expect("open normalizes state path");
        let mut temporary = tempfile::NamedTempFile::new_in(parent)
            .map_err(|e| io_error("创建状态临时文件", &self.path, e))?;
        serde_json::to_writer_pretty(&mut temporary, candidate).map_err(|e| {
            SyncError::new(
                "store_io",
                &format!("保存状态失败，请检查目录权限和磁盘空间后重试：{e}"),
            )
        })?;
        temporary
            .write_all(b"\n")
            .map_err(|e| io_error("写入状态", &self.path, e))?;
        temporary
            .flush()
            .map_err(|e| io_error("刷新状态", &self.path, e))?;
        temporary
            .as_file()
            .sync_all()
            .map_err(|e| io_error("同步状态到磁盘", &self.path, e))?;
        temporary
            .persist(&self.path)
            .map_err(|e| io_error("原子替换状态", &self.path, e.error))?;
        Ok(())
    }
}
/// A confirmed complete range is positive evidence for every covered role SHA.
/// Both direct reconciliation and head-only monitoring apply it atomically.
fn apply_covered_outcome(state: &mut ProjectState, receipt: &LocalPushReceipt) {
    if !receipt.pushed {
        return;
    }
    let covers = |sha: &str| receipt.head == sha || receipt.commits.iter().any(|c| c.sha == sha);
    if let Some(role_receipt) = &mut state.last_receipt {
        if covers(&role_receipt.commit) {
            role_receipt.pushed = true;
            role_receipt.error = None;
        }
    }
    if state.pending.as_ref().is_some_and(|p| covers(&p.commit)) {
        let mut delivered = state.pending.take().expect("covered pending receipt");
        delivered.pushed = true;
        delivered.error = None;
        // Old persisted state may contain pending but no lastReceipt yet.
        if state
            .last_receipt
            .as_ref()
            .is_none_or(|r| r.commit == delivered.commit)
        {
            state.last_receipt = Some(delivered);
        }
    }
}
fn project_index(state: &DurableState, id: &str) -> Result<usize> {
    state
        .snapshot
        .projects
        .iter()
        .position(|s| s.project.id == id)
        .ok_or_else(|| SyncError::new("project_not_found", "项目不存在，请重新选择项目"))
}
fn normalized_root(root: &str) -> String {
    let path = fs::canonicalize(root).unwrap_or_else(|_| PathBuf::from(root));
    // A backslash is a valid Unix filename character, not a path separator.
    let normalized = if cfg!(windows) {
        path.to_string_lossy().replace('\\', "/")
    } else {
        path.to_string_lossy().into_owned()
    };
    let normalized = normalized.trim_end_matches('/');
    if cfg!(windows) {
        normalized.to_lowercase()
    } else {
        normalized.into()
    }
}
fn commit_url(project: &Project, sha: &str) -> String {
    format!("https://github.com/{}/commit/{sha}", project.repository)
}
fn receipt_message(project: &Project, receipt: &PublishReceipt) -> UpdateMessage {
    UpdateMessage {
        own: true,
        id: uuid::Uuid::new_v4().to_string(),
        project_id: project.id.clone(),
        repository: project.repository.clone(),
        branch: project.branch.clone(),
        sha: receipt.commit.clone(),
        title: receipt.message.clone(),
        author: String::new(),
        committed_at: String::new(),
        url: receipt.url.clone(),
        parsed: parse_commit(&receipt.message),
        read: true,
        rewritten: false,
    }
}
fn remote_message(project: &Project, commit: &RemoteCommit, own: bool) -> UpdateMessage {
    UpdateMessage {
        own,
        id: uuid::Uuid::new_v4().to_string(),
        project_id: project.id.clone(),
        repository: project.repository.clone(),
        branch: project.branch.clone(),
        sha: commit.sha.clone(),
        title: commit.title.clone(),
        author: commit.author.clone(),
        committed_at: commit.committed_at.clone(),
        url: commit_url(project, &commit.sha),
        parsed: parse_commit(&commit.title),
        read: own,
        rewritten: false,
    }
}
fn hydrate(message: &mut UpdateMessage, commit: &RemoteCommit) {
    message.title = commit.title.clone();
    message.parsed = parse_commit(&commit.title);
    message.author = commit.author.clone();
    message.committed_at = commit.committed_at.clone();
}
fn invalid_state(path: &Path, reason: &str, code: &str) -> SyncError {
    SyncError::new(
        code,
        &format!(
            "{reason}：{}。请先备份该文件，再修复内容或使用兼容版本；原文件已保留",
            path.display()
        ),
    )
}
fn io_error(action: &str, path: &Path, error: std::io::Error) -> SyncError {
    SyncError::new(
        "store_io",
        &format!(
            "{action}失败：{}。请检查目录权限和磁盘空间后重试：{error}",
            path.display()
        ),
    )
}
