mod policy;

use collaboration_core::{
    AddProjectInput, AppSnapshot, ChangePreview, CollaborationStore, GitWorkspace,
    LocalHistoryPage, LocalPushPreview, LocalPushReceipt, ProjectState, PublishInput,
    PublishReceipt, Result, Role, SyncError, UpdateMessage,
};
use policy::{ProjectLocks, commit_url, fetch_cursor, notification_batch};
use std::{
    sync::{Arc, Mutex, MutexGuard},
    time::Duration,
};
use tauri::{
    AppHandle, Emitter, Manager, State,
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_notification::{NotificationExt, PermissionState};

struct AppState {
    store: Mutex<CollaborationStore>,
    locks: ProjectLocks,
    shutdown: tokio::sync::watch::Sender<bool>,
    operations: Arc<tokio::sync::RwLock<()>>,
    drained: std::sync::atomic::AtomicBool,
}
impl AppState {
    fn accepting_work(&self) -> Result<()> {
        if *self.shutdown.borrow() {
            Err(error(
                "shutting_down",
                "应用正在等待当前操作安全完成并退出，请稍后重新打开",
            ))
        } else {
            Ok(())
        }
    }
    async fn begin_work(&self) -> Result<tokio::sync::OwnedRwLockReadGuard<()>> {
        self.accepting_work()?;
        let guard = self.operations.clone().read_owned().await;
        self.accepting_work()?;
        Ok(guard)
    }
    async fn drain(&self) {
        self.shutdown.send_replace(true);
        let _finished = self.operations.write().await;
        self.drained
            .store(true, std::sync::atomic::Ordering::Release);
    }
    // Caller holds the project lock; collect both proofs before any Store write.
    async fn recover_project(&self, id: &str) -> Result<Option<PublishReceipt>> {
        let project = self.project(id)?.project;
        let (role, range) = git_job(move || {
            let workspace = GitWorkspace::new(project);
            let range = workspace.recover_local_push()?;
            let role = workspace.recover_receipt()?;
            Ok((role, range))
        })
        .await?;
        self.persist_recovery(role, range)
    }
    fn persist_recovery(
        &self,
        role: Option<PublishReceipt>,
        range: Option<LocalPushReceipt>,
    ) -> Result<Option<PublishReceipt>> {
        let known_head = range
            .as_ref()
            .map(|r| r.head.as_str())
            .or_else(|| role.as_ref().map(|r| r.commit.as_str()))
            .unwrap_or("")
            .to_owned();
        let save = || -> Result<Option<PublishReceipt>> {
            let mut store = self.store()?;
            let mut role = role.map(|r| store.reconcile_receipt(r)).transpose()?;
            if let Some(range) = range {
                store.reconcile_local_push(range)?;
                if let Some(previous) = &role {
                    // Range coordination can upgrade the covered role outcome.
                    // Existing retry callers must see that effective success too.
                    role = store
                        .snapshot()
                        .projects
                        .into_iter()
                        .find(|p| p.project.id == previous.project_id)
                        .and_then(|p| p.last_receipt)
                        .filter(|r| r.commit == previous.commit)
                        .or(role);
                }
            }
            Ok(role)
        };
        save().map_err(|failure| error("recovery_required", &format!("交付 {known_head} 已保留；恢复应用状态保存权限后重试，当前暂不接受新的提交：{}", failure.message)))
    }
    async fn read_history(
        &self,
        id: &str,
        offset: usize,
        expected_head: Option<String>,
    ) -> Result<LocalHistoryPage> {
        let _work = self.begin_work().await?;
        let (_operation, current) = self.locked_project(id).await?;
        // History is purely read-only: no recovery or network.
        git_job(move || {
            GitWorkspace::new(current.project).local_history(offset, expected_head.as_deref())
        })
        .await
    }
    async fn commit_local_service(&self, input: PublishInput) -> Result<PublishReceipt> {
        let _work = self.begin_work().await?;
        let (_operation, current) = self.locked_project(&input.project_id).await?;
        let project = current.project;
        let (role, range) = git_job(move || {
            let workspace = GitWorkspace::new(project);
            let range = workspace.recover_local_push_offline()?;
            let role = workspace.recover_local_receipt()?;
            Ok((role, range))
        })
        .await?;
        // Both required saves succeed before HEAD is allowed to advance.
        self.persist_recovery(role, range)?;
        let current = self.project(&input.project_id)?;
        let receipt =
            git_job(move || GitWorkspace::new(current.project).commit_local(&input)).await?;
        Ok(match self.store() {
            Ok(mut store) => store.record_delivery(receipt),
            Err(failure) => {
                let mut receipt = receipt;
                receipt.persistence_warning = Some(failure.message);
                receipt
            }
        })
    }
    async fn configure_paths_service(
        &self,
        id: &str,
        paths: std::collections::BTreeMap<Role, Vec<String>>,
    ) -> Result<AppSnapshot> {
        let _work = self.begin_work().await?;
        let (_operation, current) = self.locked_project(id).await?;
        // configure_role_paths owns offline guards and same-mapping retry rules.
        let project =
            git_job(move || GitWorkspace::new(current.project).configure_role_paths(&paths))
                .await?;
        self.store()
            .and_then(|mut store| store.update_project_role_paths(project))
            .map_err(|failure| {
                error(
                    "settings_persistence",
                    &format!(
                        "配置已写入仓库，应用状态保存失败；请保留草稿并保存同一映射重试：{}",
                        failure.message
                    ),
                )
            })?;
        self.snapshot()
    }
    async fn preview_push_service(&self, id: &str) -> Result<LocalPushPreview> {
        let _work = self.begin_work().await?;
        let (_operation, _) = self.locked_project(id).await?;
        self.recover_project(id).await?;
        let current = self.project(id)?;
        git_job(move || GitWorkspace::new(current.project).preview_local_push()).await
    }
    async fn push_local_service(&self, id: &str, fingerprint: &str) -> Result<LocalPushReceipt> {
        let _work = self.begin_work().await?;
        let (_operation, _) = self.locked_project(id).await?;
        self.recover_project(id).await?;
        let current = self.project(id)?;
        let fingerprint = fingerprint.to_owned();
        let receipt =
            git_job(move || GitWorkspace::new(current.project).push_local_commits(&fingerprint))
                .await?;
        Ok(match self.store() {
            Ok(mut store) => store.record_local_push(receipt),
            Err(failure) => {
                let mut receipt = receipt;
                receipt.persistence_warning = Some(failure.message);
                receipt
            }
        })
    }

    fn store(&self) -> Result<MutexGuard<'_, CollaborationStore>> {
        self.store.lock().map_err(|_| {
            error(
                "store_unavailable",
                "状态服务不可用，请退出Git 协作台后重新打开；不要删除状态文件",
            )
        })
    }
    fn snapshot(&self) -> Result<AppSnapshot> {
        Ok(self.store()?.snapshot())
    }
    fn project(&self, id: &str) -> Result<ProjectState> {
        self.snapshot()?
            .projects
            .into_iter()
            .find(|state| state.project.id == id)
            .ok_or_else(|| error("project_missing", "项目已移除或不存在，请重新选择项目"))
    }
    async fn locked_project(
        &self,
        id: &str,
    ) -> Result<(tokio::sync::OwnedMutexGuard<()>, ProjectState)> {
        self.accepting_work()?;
        let operation = self.locks.for_project(id).lock_owned().await;
        self.accepting_work()?;
        let project = self.project(id)?;
        Ok((operation, project))
    }
}
fn error(code: &str, message: &str) -> SyncError {
    SyncError {
        code: code.into(),
        message: message.into(),
    }
}
async fn git_job<T: Send + 'static>(job: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    tokio::task::spawn_blocking(job).await.map_err(|_| {
        error(
            "operation_interrupted",
            "本地操作异常中断，请重新检查项目状态后重试",
        )
    })?
}
fn emit_snapshot(
    state: &AppState,
    emit: impl FnOnce(&AppSnapshot) -> Result<()>,
) -> Result<AppSnapshot> {
    let snapshot = state.snapshot()?;
    // The revision travels with this capture, even if delivery is delayed.
    emit(&snapshot)?;
    Ok(snapshot)
}
fn changed(app: &AppHandle, state: &AppState) -> Result<AppSnapshot> {
    emit_snapshot(state, |snapshot| {
        app.emit("gitcollab://changed", snapshot).map_err(|_| {
            error(
                "event_unavailable",
                "状态已保存，但界面刷新失败，请重新打开主窗口",
            )
        })
    })
}
fn git_failed(app: &AppHandle, state: &AppState, id: &str, failure: SyncError) -> SyncError {
    // Caller owns the project operation lock. No Store lock is held during Git.
    if let Err(save) = state
        .store()
        .and_then(|mut store| store.record_error(id, failure.message.clone()))
    {
        return save;
    }
    if let Err(emit) = changed(app, state) {
        return emit;
    }
    failure
}

#[tauri::command]
fn gitcollab_snapshot(state: State<'_, Arc<AppState>>) -> Result<AppSnapshot> {
    state.snapshot()
}
#[tauri::command]
async fn gitcollab_add_project(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    input: AddProjectInput,
) -> Result<AppSnapshot> {
    let _work = state.begin_work().await?;
    let _registration = state.locks.for_project("<registration>").lock_owned().await;
    let project = git_job(move || GitWorkspace::discover(&input)).await?;
    let _operation = state.locks.for_project(&project.id).lock_owned().await;
    state.store()?.add_project(project)?;
    changed(&app, &state)
}
#[tauri::command]
async fn gitcollab_remove_project(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    project_id: String,
) -> Result<AppSnapshot> {
    let _work = state.begin_work().await?;
    let _operation = state.locks.for_project(&project_id).lock_owned().await;
    state.store()?.remove_project(&project_id)?;
    changed(&app, &state)
}
#[tauri::command]
async fn gitcollab_select(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    project_id: Option<String>,
    role: Role,
) -> Result<AppSnapshot> {
    let _work = state.begin_work().await?;
    let _operation = match project_id.as_ref() {
        Some(id) => Some(state.locks.for_project(id).lock_owned().await),
        None => None,
    };
    if let Some(id) = &project_id {
        state.project(id)?;
    }
    state.store()?.select(project_id, role)?;
    changed(&app, &state)
}
#[tauri::command]
async fn gitcollab_preview(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    project_id: String,
    role: Role,
) -> Result<ChangePreview> {
    let _work = state.begin_work().await?;
    let (_operation, _) = state.locked_project(&project_id).await?;
    state.recover_project(&project_id).await?;
    changed(&app, &state)?;
    let current = state.project(&project_id)?;
    let project = current.project;
    git_job(move || GitWorkspace::new(project).preview(role))
        .await
        .map_err(|failure| git_failed(&app, &state, &project_id, failure))
}

#[tauri::command]
async fn gitcollab_local_history(
    state: State<'_, Arc<AppState>>,
    project_id: String,
    offset: Option<usize>,
    expected_head: Option<String>,
) -> Result<LocalHistoryPage> {
    state
        .read_history(&project_id, offset.unwrap_or(0), expected_head)
        .await
}
#[tauri::command]
async fn gitcollab_commit_local(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    input: PublishInput,
) -> Result<PublishReceipt> {
    let mut receipt = match state.commit_local_service(input).await {
        Ok(receipt) => receipt,
        Err(failure) => {
            changed(&app, &state)?;
            return Err(failure);
        }
    };
    if let Err(failure) = changed(&app, &state) {
        receipt.persistence_warning.get_or_insert(failure.message);
    }
    Ok(receipt)
}
#[tauri::command]
async fn gitcollab_set_role_paths(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    project_id: String,
    role_paths: std::collections::BTreeMap<Role, Vec<String>>,
) -> Result<AppSnapshot> {
    let result = state.configure_paths_service(&project_id, role_paths).await;
    let snapshot = changed(&app, &state)?;
    result.map(|_| snapshot)
}
#[tauri::command]
async fn gitcollab_pick_role_folder(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    project_id: String,
) -> Result<Option<String>> {
    let _work = state.begin_work().await?;
    let (_operation, current) = state.locked_project(&project_id).await?;
    let root = current.project.root;
    git_job(move || {
        let chosen = app
            .dialog()
            .file()
            .set_directory(&root)
            .set_title("选择仓库内身份目录")
            .blocking_pick_folder();
        chosen
            .map(|path| {
                let path = path
                    .into_path()
                    .map_err(|_| error("invalid_mapping", "请选择本地仓库内文件夹"))?;
                policy::role_folder_relative(std::path::Path::new(&root), &path)
            })
            .transpose()
    })
    .await
}
#[tauri::command]
async fn gitcollab_preview_local_push(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    project_id: String,
) -> Result<LocalPushPreview> {
    let result = state.preview_push_service(&project_id).await;
    changed(&app, &state)?;
    result
}
#[tauri::command]
async fn gitcollab_push_local_commits(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    project_id: String,
    fingerprint: String,
) -> Result<LocalPushReceipt> {
    let mut receipt = match state.push_local_service(&project_id, &fingerprint).await {
        Ok(receipt) => receipt,
        Err(failure) => {
            changed(&app, &state)?;
            return Err(failure);
        }
    };
    if let Err(failure) = changed(&app, &state) {
        receipt.persistence_warning.get_or_insert(failure.message);
    }
    Ok(receipt)
}

// Already holding the project lock: hydrate successful receipts from real Git
// metadata. Failure here cannot turn an acknowledged push into a failed push.
async fn hydrate_receipt(
    app: &AppHandle,
    state: &AppState,
    receipt: &PublishReceipt,
) -> Result<Vec<UpdateMessage>> {
    let current = state.project(&receipt.project_id)?;
    let cursor = fetch_cursor(&current, Some(receipt));
    let project = current.project;
    let messages =
        match git_job(move || GitWorkspace::new(project).fetch_updates(cursor.as_deref())).await {
            Ok(remote) => state.store()?.apply_remote(&receipt.project_id, remote)?,
            Err(failure) => {
                state.store()?.record_error(
                    &receipt.project_id,
                    format!("交付已推送成功，但更新元数据获取失败：{}", failure.message),
                )?;
                vec![]
            }
        };
    changed(app, state)?;
    Ok(messages)
}
#[tauri::command]
async fn gitcollab_publish(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    input: PublishInput,
) -> Result<PublishReceipt> {
    let _work = state.begin_work().await?;
    let id = input.project_id.clone();
    let (operation, _) = state.locked_project(&id).await?;
    state.recover_project(&id).await?;
    changed(&app, &state)?;
    let current = state.project(&id)?;
    if current.pending.is_some() {
        return Err(error(
            "pending_delivery",
            "该项目有待重试交付，请先检查或重试推送",
        ));
    }
    let mut receipt = git_job(move || GitWorkspace::new(current.project).publish(&input))
        .await
        .map_err(|failure| git_failed(&app, &state, &id, failure))?;
    receipt = match state.store() {
        Ok(mut store) => store.record_delivery(receipt),
        Err(failure) => {
            receipt.persistence_warning = Some(failure.message);
            receipt
        }
    };
    if let Err(failure) = changed(&app, &state) {
        receipt.persistence_warning.get_or_insert(failure.message);
    }
    // If ownership could not be durably recorded, recover it before fetching
    // history; otherwise our delivery could be misclassified as an external one.
    let messages = if receipt.pushed && receipt.persistence_warning.is_none() {
        match hydrate_receipt(&app, &state, &receipt).await {
            Ok(messages) => messages,
            Err(failure) => {
                receipt
                    .persistence_warning
                    .get_or_insert(format!("交付已推送，但应用更新未保存：{}", failure.message));
                vec![]
            }
        }
    } else {
        vec![]
    };
    drop(operation);
    // A notification failure persists a monitor error and must not misreport
    // the already successful push as a failed delivery.
    let _ = notify_batch(&app, &state, &messages).await;
    Ok(receipt)
}
#[tauri::command]
async fn gitcollab_retry_push(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    project_id: String,
) -> Result<PublishReceipt> {
    let _work = state.begin_work().await?;
    let (operation, _) = state.locked_project(&project_id).await?;
    let recovered = state.recover_project(&project_id).await?;
    changed(&app, &state)?;
    if let Some(receipt) = recovered.filter(|receipt| receipt.pushed) {
        return Ok(receipt);
    }
    let current = state.project(&project_id)?;
    let pending = current
        .pending
        .ok_or_else(|| error("no_pending_delivery", "没有待重试交付，请重新检查变更"))?;
    let mut receipt = git_job(move || GitWorkspace::new(current.project).retry_push(&pending))
        .await
        .map_err(|failure| git_failed(&app, &state, &project_id, failure))?;
    receipt = match state.store() {
        Ok(mut store) => store.record_delivery(receipt),
        Err(failure) => {
            receipt.persistence_warning = Some(failure.message);
            receipt
        }
    };
    if let Err(failure) = changed(&app, &state) {
        receipt.persistence_warning.get_or_insert(failure.message);
    }
    // If ownership could not be durably recorded, recover it before fetching
    // history; otherwise our delivery could be misclassified as an external one.
    let messages = if receipt.pushed && receipt.persistence_warning.is_none() {
        match hydrate_receipt(&app, &state, &receipt).await {
            Ok(messages) => messages,
            Err(failure) => {
                receipt
                    .persistence_warning
                    .get_or_insert(format!("交付已推送，但应用更新未保存：{}", failure.message));
                vec![]
            }
        }
    } else {
        vec![]
    };
    drop(operation);
    let _ = notify_batch(&app, &state, &messages).await;
    Ok(receipt)
}
#[tauri::command]
async fn gitcollab_pull(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    project_id: String,
) -> Result<String> {
    let _work = state.begin_work().await?;
    let (_operation, _) = state.locked_project(&project_id).await?;
    state.recover_project(&project_id).await?;
    changed(&app, &state)?;
    let current = state.project(&project_id)?;
    let project = current.project;
    let head = git_job(move || GitWorkspace::new(project).pull())
        .await
        .map_err(|failure| git_failed(&app, &state, &project_id, failure))?;
    // A local pull does not acknowledge remote monitoring history.
    changed(&app, &state)?;
    Ok(head)
}

async fn check_one(
    app: &AppHandle,
    state: &AppState,
    id: &str,
    monitored_only: bool,
) -> Result<Vec<UpdateMessage>> {
    let (_operation, _) = state.locked_project(id).await?;
    state.recover_project(&id).await?;
    changed(&app, &state)?;
    let current = state.project(&id)?;
    if monitored_only && !current.project.monitor_enabled {
        return Ok(vec![]);
    }
    let cursor = fetch_cursor(&current, None);
    let project = current.project;
    let remote = git_job(move || GitWorkspace::new(project).fetch_updates(cursor.as_deref()))
        .await
        .map_err(|failure| git_failed(app, state, id, failure))?;
    let messages = state.store()?.apply_remote(id, remote)?; // Atomic durable cursor + list.
    changed(app, state)?;
    Ok(messages)
}
async fn notify_batch(app: &AppHandle, state: &AppState, messages: &[UpdateMessage]) -> Result<()> {
    let identifier = app.config().identifier.clone();
    deliver_notifications(
        state,
        messages,
        move |batch| {
            #[cfg(target_os = "macos")]
            {
                // mac-notification-sys rejects a second set_application call.
                // Keep one initialization result for this process and identifier.
                static APPLICATION: std::sync::OnceLock<std::result::Result<(), ()>> =
                    std::sync::OnceLock::new();
                (*APPLICATION
                    .get_or_init(|| notify_rust::set_application(&identifier).map_err(|_| ())))?;
            }
            // Plugin 2.4.0 builder.show() starts an unobserved async task and loses
            // the OS Result. Await notify-rust transport in our blocking worker.
            let mut notification = notify_rust::Notification::new();
            notification
                .summary(&batch.title)
                .body(&batch.body)
                .appname(&identifier);
            #[cfg(windows)]
            notification.app_id(&identifier);
            notification.show().map(|_| ()).map_err(|_| ())
        },
        |snapshot| {
            app.emit("gitcollab://changed", snapshot).map_err(|_| {
                error(
                    "event_unavailable",
                    "状态已保存，但界面刷新失败，请重新打开主窗口",
                )
            })
        },
    )
    .await
}
async fn deliver_notifications(
    state: &AppState,
    messages: &[UpdateMessage],
    sender: impl FnOnce(policy::NotificationBatch) -> std::result::Result<(), ()> + Send + 'static,
    emit: impl FnOnce(&AppSnapshot) -> Result<()>,
) -> Result<()> {
    let Some(batch) = notification_batch(state.snapshot()?.system_notifications, messages) else {
        return Ok(());
    };
    let delivered = git_job(move || {
        sender(batch).map_err(|_| {
            error(
                "notification_unavailable",
                "系统通知发送失败，更新已保留在消息列表；请检查系统通知设置及应用安装状态",
            )
        })
    })
    .await;
    if let Err(failure) = delivered {
        let ids: std::collections::BTreeSet<_> =
            messages.iter().map(|message| &message.project_id).collect();
        for id in ids {
            let _operation = state.locks.for_project(id).lock_owned().await;
            if state.project(id).is_ok() {
                state.store()?.record_error(id, failure.message.clone())?;
            }
        }
        emit_snapshot(state, emit)?;
        return Err(failure);
    }
    Ok(())
}
async fn check_batch(
    app: &AppHandle,
    state: &AppState,
    project_id: Option<String>,
    monitored_only: bool,
) -> Result<AppSnapshot> {
    let _work = state.begin_work().await?;
    let ids = project_id.map(|id| vec![id]).unwrap_or(
        state
            .snapshot()?
            .projects
            .into_iter()
            .filter(|s| !monitored_only || s.project.monitor_enabled)
            .map(|s| s.project.id)
            .collect(),
    );
    let mut messages = Vec::new();
    let mut first_error = None;
    for id in ids {
        match check_one(app, state, &id, monitored_only).await {
            Ok(batch) => messages.extend(batch),
            Err(failure) => {
                first_error.get_or_insert(failure);
            }
        }
    }
    if let Err(failure) = notify_batch(app, state, &messages).await {
        first_error.get_or_insert(failure);
    }
    let snapshot = changed(app, state)?;
    match first_error {
        Some(failure) => Err(failure),
        None => Ok(snapshot),
    }
}
#[tauri::command]
async fn gitcollab_check_updates(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    project_id: Option<String>,
) -> Result<AppSnapshot> {
    check_batch(&app, &state, project_id, false).await
}
#[tauri::command]
async fn gitcollab_set_monitor(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    project_id: String,
    enabled: bool,
) -> Result<AppSnapshot> {
    let _work = state.begin_work().await?;
    let _operation = state.locks.for_project(&project_id).lock_owned().await;
    state.store()?.set_monitor(&project_id, enabled)?;
    changed(&app, &state)
}
#[tauri::command]
async fn gitcollab_set_notifications(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    enabled: bool,
) -> Result<AppSnapshot> {
    let _work = state.begin_work().await?;
    if enabled {
        let permission_app = app.clone();
        git_job(move || {
            let plugin = permission_app.notification();
            let permission = plugin.permission_state().map_err(|_| {
                error(
                    "notification_permission",
                    "无法检查系统通知权限，请检查系统通知设置后重试",
                )
            })?;
            let permission = if permission == PermissionState::Granted {
                permission
            } else {
                plugin.request_permission().map_err(|_| {
                    error(
                        "notification_permission",
                        "无法请求系统通知权限，请在系统设置中允许通知后重试",
                    )
                })?
            };
            if permission != PermissionState::Granted {
                return Err(error(
                    "notification_permission",
                    "系统通知权限未获允许，请在系统设置中启用通知后重试",
                ));
            }
            Ok(())
        })
        .await?;
    }
    state.store()?.set_notifications(enabled)?;
    changed(&app, &state)
}
#[tauri::command]
async fn gitcollab_mark_read(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    ids: Vec<String>,
) -> Result<AppSnapshot> {
    let _work = state.begin_work().await?;
    state.store()?.mark_read(&ids)?;
    changed(&app, &state)
}
#[tauri::command]
async fn gitcollab_open_commit(
    state: State<'_, Arc<AppState>>,
    repository: String,
    sha: String,
) -> Result<()> {
    let _work = state.begin_work().await?;
    let url = commit_url(&repository, &sha)?;
    git_job(move || {
        #[cfg(windows)]
        let mut command = std::process::Command::new("explorer.exe");
        #[cfg(target_os = "macos")]
        let mut command = std::process::Command::new("open");
        #[cfg(all(not(windows), not(target_os = "macos")))]
        let mut command = std::process::Command::new("xdg-open");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        command.arg(url).spawn().map_err(|_| {
            error(
                "open_commit_failed",
                "无法打开提交链接，请检查默认浏览器设置后重试",
            )
        })?;
        Ok(())
    })
    .await
}
fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        observe("main-opened");
    }
}
fn exit(app: &AppHandle) {
    let state = app.state::<Arc<AppState>>().inner().clone();
    // Flip admission synchronously for both the tray and IPC entry points.
    if state.shutdown.send_replace(true) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        state.drain().await;
        observe("exit");
        app.exit(0);
    });
}
#[tauri::command]
fn gitcollab_exit(app: AppHandle) {
    exit(&app);
}

async fn poll(
    app: AppHandle,
    state: Arc<AppState>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    if let Ok(_work) = state.begin_work().await {
        if let Ok(snapshot) = state.snapshot() {
            for current in snapshot.projects {
                if state.accepting_work().is_err() {
                    break;
                }
                let id = current.project.id;
                let _operation = state.locks.for_project(&id).lock_owned().await;
                if let Err(failure) = state.recover_project(&id).await {
                    let _ = git_failed(&app, &state, &id, failure);
                }
                let _ = changed(&app, &state);
            }
        }
    }
    let mut ticks = tokio::time::interval(Duration::from_secs(120));
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        if *shutdown.borrow() {
            break;
        }
        tokio::select! {
            _ = shutdown.changed() => break,
            _ = ticks.tick() => {
                // Persisted project errors are the UI boundary. One failure never
                // terminates monitoring or prevents later projects in the batch.
                let _ = check_batch(&app, &state, None, true).await;
            }
        }
    }
}
// Opt-in local EXE smoke observability; no test commands or runtime Git fixtures.
fn observe(event: &str) {
    if let Some(path) = std::env::var_os("GITCOLLAB_TEST_REPORT") {
        use std::io::Write;
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(file, "{event}");
        }
    }
}
pub fn run() {
    let built = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(tauri::generate_handler![
            gitcollab_snapshot,
            gitcollab_add_project,
            gitcollab_remove_project,
            gitcollab_select,
            gitcollab_preview,
            gitcollab_local_history,
            gitcollab_commit_local,
            gitcollab_set_role_paths,
            gitcollab_pick_role_folder,
            gitcollab_preview_local_push,
            gitcollab_push_local_commits,
            gitcollab_publish,
            gitcollab_retry_push,
            gitcollab_pull,
            gitcollab_check_updates,
            gitcollab_set_monitor,
            gitcollab_set_notifications,
            gitcollab_mark_read,
            gitcollab_open_commit,
            gitcollab_exit
        ])
        .setup(|app| {
            let directory = match std::env::var_os("GITCOLLAB_DATA_DIR") {
                Some(path) => std::path::PathBuf::from(path),
                None => app.path().app_data_dir()?,
            };
            let store = match CollaborationStore::open(directory.join("state.json")) {
                Ok(store) => store,
                Err(failure) => {
                    observe("startup-error");
                    app.dialog()
                        .message(&failure.message)
                        .title("Git 协作台启动失败")
                        .blocking_show();
                    return Err(Box::new(failure));
                }
            };
            let (shutdown, receiver) = tokio::sync::watch::channel(false);
            let state = Arc::new(AppState {
                store: Mutex::new(store),
                locks: ProjectLocks::default(),
                shutdown,
                operations: Arc::new(tokio::sync::RwLock::new(())),
                drained: std::sync::atomic::AtomicBool::new(false),
            });
            app.manage(state.clone());
            let open = MenuItem::with_id(app, "open", "打开Git 协作台", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "exit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &quit])?;
            let icon = app
                .default_window_icon()
                .cloned()
                .ok_or("应用图标缺失，请重新安装Git 协作台")?;
            TrayIconBuilder::new()
                .icon(icon)
                .tooltip("Git 协作台")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "open" => {
                        show_main(app);
                    }
                    "exit" => exit(app),
                    _ => {}
                })
                .build(app)?;
            tauri::async_runtime::spawn(poll(app.handle().clone(), state, receiver));
            observe("ready");
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                if window.hide().is_ok() {
                    observe("main-hidden");
                }
            }
        })
        .build(tauri::generate_context!());
    match built {
        Ok(app) => app.run(|app, event| {
            // Closing the last macOS window keeps monitoring alive. A Dock
            // click must reopen it, while Cmd+Q still uses the drain below.
            #[cfg(target_os = "macos")]
            if matches!(&event, tauri::RunEvent::Reopen { .. }) {
                if app.state::<Arc<AppState>>().accepting_work().is_ok() {
                    show_main(app);
                }
            }
            if let tauri::RunEvent::ExitRequested { api, .. } = &event {
                if let Some(state) = app.try_state::<Arc<AppState>>() {
                    if !state.drained.load(std::sync::atomic::Ordering::Acquire) {
                        api.prevent_exit();
                        exit(app);
                    }
                }
            }
            if matches!(event, tauri::RunEvent::Exit) {
                if let Some(state) = app.try_state::<Arc<AppState>>() {
                    let _ = state.shutdown.send(true);
                }
                observe("stopped");
            }
        }),
        Err(_) => {
            // Store failures have an actionable native dialog above. Do not leak
            // plugin/OS diagnostics or accidentally reset the existing state.
            eprintln!("Git 协作台启动失败，请检查状态目录权限、其他运行实例及安装文件后重试。");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use collaboration_core::Project;

    struct NotificationFixture {
        state: Option<AppState>,
        directory: std::path::PathBuf,
        messages: Vec<UpdateMessage>,
    }
    impl NotificationFixture {
        fn new(enabled: bool) -> Self {
            use collaboration_core::{RemoteCommit, RemoteSnapshot};
            let directory = std::env::temp_dir().join(format!(
                "gitcollab-notify-test-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            let mut store = CollaborationStore::open(directory.join("state.json")).unwrap();
            store
                .add_project(Project {
                    id: "p".into(),
                    name: "p".into(),
                    root: "local".into(),
                    repository: "o/r".into(),
                    remote_url: "https://github.com/o/r".into(),
                    branch: "main".into(),
                    role_paths: Default::default(),
                    monitor_enabled: true,
                })
                .unwrap();
            store.set_notifications(enabled).unwrap();
            store
                .apply_remote(
                    "p",
                    RemoteSnapshot {
                        head: "a".repeat(40),
                        commits: vec![],
                        rewritten: false,
                    },
                )
                .unwrap();
            let messages = store
                .apply_remote(
                    "p",
                    RemoteSnapshot {
                        head: "b".repeat(40),
                        commits: vec![RemoteCommit {
                            sha: "b".repeat(40),
                            title: "feat(frontend): 页面 - 新增导航".into(),
                            author: "真实作者".into(),
                            committed_at: "2026-10-07T12:00:00Z".into(),
                        }],
                        rewritten: false,
                    },
                )
                .unwrap();
            let (shutdown, _) = tokio::sync::watch::channel(false);
            Self {
                state: Some(AppState {
                    store: Mutex::new(store),
                    locks: ProjectLocks::default(),
                    shutdown,
                    operations: Arc::new(tokio::sync::RwLock::new(())),
                    drained: std::sync::atomic::AtomicBool::new(false),
                }),
                directory,
                messages,
            }
        }
        fn state(&self) -> &AppState {
            self.state.as_ref().unwrap()
        }
    }
    impl Drop for NotificationFixture {
        fn drop(&mut self) {
            self.state.take();
            std::fs::remove_dir_all(&self.directory).unwrap();
        }
    }

    #[tokio::test]
    async fn failed_notification_preserves_durable_updates_and_emits_actionable_state() {
        let mut fixture = NotificationFixture::new(true);
        let emitted = Mutex::new(None);
        let result = deliver_notifications(
            fixture.state(),
            &fixture.messages,
            |_| Err(()),
            |snapshot| {
                *emitted.lock().unwrap() = Some(snapshot.clone());
                Ok(())
            },
        )
        .await;
        assert!(matches!(result, Err(failure) if failure.code == "notification_unavailable"));
        let snapshot = fixture.state().snapshot().unwrap();
        assert_eq!(snapshot.messages.len(), 1);
        assert_eq!(snapshot.messages[0].sha, "b".repeat(40));
        assert_eq!(snapshot.projects[0].cursor, Some("b".repeat(40)));
        assert!(
            snapshot.projects[0]
                .last_error
                .as_ref()
                .unwrap()
                .contains("系统通知发送失败")
        );
        assert!(
            emitted.lock().unwrap().as_ref().unwrap().projects[0]
                .last_error
                .is_some()
        );
        fixture.state.take();
        let reopened = CollaborationStore::open(fixture.directory.join("state.json")).unwrap();
        assert_eq!(reopened.snapshot().messages.len(), 1);
        assert!(reopened.snapshot().projects[0].last_error.is_some());
        drop(reopened);
    }

    #[tokio::test]
    async fn successful_notification_keeps_updates_without_error() {
        let fixture = NotificationFixture::new(true);
        deliver_notifications(fixture.state(), &fixture.messages, |_| Ok(()), |_| Ok(()))
            .await
            .unwrap();
        let snapshot = fixture.state().snapshot().unwrap();
        assert_eq!(snapshot.messages.len(), 1);
        assert_eq!(snapshot.projects[0].cursor, Some("b".repeat(40)));
        assert!(snapshot.projects[0].last_error.is_none());
    }

    #[tokio::test]
    async fn disabled_and_empty_batches_skip_native_transport_and_keep_updates() {
        let fixture = NotificationFixture::new(false);
        deliver_notifications(
            fixture.state(),
            &fixture.messages,
            |_| panic!("disabled notification must not reach OS"),
            |_| Ok(()),
        )
        .await
        .unwrap();
        fixture
            .state()
            .store()
            .unwrap()
            .set_notifications(true)
            .unwrap();
        deliver_notifications(
            fixture.state(),
            &[],
            |_| panic!("empty batch must not reach OS"),
            |_| Ok(()),
        )
        .await
        .unwrap();
        assert_eq!(fixture.state().snapshot().unwrap().messages.len(), 1);
        assert!(
            fixture.state().snapshot().unwrap().projects[0]
                .last_error
                .is_none()
        );
    }

    #[tokio::test]
    async fn queued_git_operation_rechecks_project_after_removal() {
        // Reading before awaiting the project lock would return a removed
        // project's stale cursor/config and allow an old fetch to write back.
        let directory = std::env::temp_dir().join(format!(
            "gitcollab-shell-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut store = CollaborationStore::open(directory.join("state.json")).unwrap();
        store
            .add_project(Project {
                id: "p".into(),
                name: "p".into(),
                root: "local".into(),
                repository: "o/r".into(),
                remote_url: "https://github.com/o/r".into(),
                branch: "main".into(),
                role_paths: Default::default(),
                monitor_enabled: true,
            })
            .unwrap();
        let (shutdown, _) = tokio::sync::watch::channel(false);
        let state = AppState {
            store: Mutex::new(store),
            locks: ProjectLocks::default(),
            shutdown,
            operations: Arc::new(tokio::sync::RwLock::new(())),
            drained: std::sync::atomic::AtomicBool::new(false),
        };
        {
            let active_removal = state.locks.for_project("p").lock_owned().await;
            let queued = state.locked_project("p");
            tokio::pin!(queued);
            assert!(
                tokio::time::timeout(Duration::from_millis(10), &mut queued)
                    .await
                    .is_err()
            );
            state.store().unwrap().remove_project("p").unwrap();
            drop(active_removal);
            assert!(matches!(queued.await, Err(failure) if failure.code == "project_missing"));
        }
        assert!(state.snapshot().unwrap().projects.is_empty());
        drop(state);
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[tokio::test]
    async fn final_review_shutdown_rejects_new_project_work() {
        let fixture = NotificationFixture::new(false);
        fixture.state().shutdown.send_replace(true);
        let result = fixture.state().locked_project("p").await;
        assert!(matches!(result, Err(failure) if failure.code == "shutting_down"));
    }
    fn local_git(root: &std::path::Path, args: &[&str]) -> String {
        let result = std::process::Command::new("git")
            .current_dir(root)
            .args(args)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        String::from_utf8(result.stdout).unwrap().trim().into()
    }
    #[tokio::test]
    async fn final_review_shutdown_drains_owned_git_and_durable_completion() {
        let mut fixture = NotificationFixture::new(false);
        let state = Arc::new(fixture.state.take().unwrap());
        let root = fixture.directory.join("repo");
        std::fs::create_dir(&root).unwrap();
        local_git(&root, &["init", "--initial-branch=main"]);
        local_git(&root, &["config", "user.name", "Test"]);
        local_git(&root, &["config", "user.email", "test@example.invalid"]);
        std::fs::write(root.join("file.txt"), "safe completion").unwrap();
        let (at_boundary, reached) = tokio::sync::oneshot::channel();
        let (release, proceed) = std::sync::mpsc::channel();
        let worker_state = state.clone();
        let worker_root = root.clone();
        let worker = tokio::spawn(async move {
            let _work = worker_state.begin_work().await.unwrap();
            let (_project, _) = worker_state.locked_project("p").await.unwrap();
            let sha = git_job(move || {
                at_boundary.send(()).unwrap();
                proceed.recv().unwrap();
                local_git(&worker_root, &["add", "."]);
                local_git(&worker_root, &["commit", "-m", "owned completion"]);
                Ok(local_git(&worker_root, &["rev-parse", "HEAD"]))
            })
            .await
            .unwrap();
            worker_state
                .store()
                .unwrap()
                .record_receipt(PublishReceipt {
                    project_id: "p".into(),
                    role: Role::Frontend,
                    commit: sha.clone(),
                    parent: String::new(),
                    message: "fix(frontend): fixture - safe completion".into(),
                    url: format!("https://github.com/o/r/commit/{sha}"),
                    pushed: false,
                    push_requested: true,
                    error: Some("fixture push unavailable".into()),
                    persistence_warning: None,
                })
                .unwrap();
            sha
        });
        reached.await.unwrap();
        let shutdown_state = state.clone();
        let mut shutting_down = tokio::spawn(async move {
            shutdown_state.drain().await;
        });
        assert!(
            tokio::time::timeout(Duration::from_millis(25), &mut shutting_down)
                .await
                .is_err()
        );
        assert!(
            matches!(state.begin_work().await, Err(failure) if failure.code == "shutting_down")
        );
        assert!(!state.drained.load(std::sync::atomic::Ordering::Acquire));
        release.send(()).unwrap();
        let sha = worker.await.unwrap();
        shutting_down.await.unwrap();
        assert!(state.drained.load(std::sync::atomic::Ordering::Acquire));
        drop(state);
        let reopened = CollaborationStore::open(fixture.directory.join("state.json")).unwrap();
        assert_eq!(
            reopened.snapshot().projects[0]
                .pending
                .as_ref()
                .unwrap()
                .commit,
            sha
        );
        assert_eq!(local_git(&root, &["rev-parse", "HEAD"]), sha);
        assert_eq!(local_git(&root, &["status", "--porcelain"]), "");
        assert!(!root.join(".git/index.lock").exists());
        drop(reopened);
    }
    #[tokio::test]
    async fn final_review_changed_and_notification_error_emits_carry_store_order() {
        let fixture = NotificationFixture::new(true);
        let state = fixture.state();
        let emitted = Mutex::new(Vec::<AppSnapshot>::new());
        // Capture S1, commit+emit S2, then finish S1 delivery, as independent
        // projects or settings commands can do while Git/OS work is awaiting.
        emit_snapshot(state, |older| {
            state.store()?.set_monitor("p", false)?;
            emit_snapshot(state, |newer| {
                emitted.lock().unwrap().push(newer.clone());
                Ok(())
            })?;
            emitted.lock().unwrap().push(older.clone());
            Ok(())
        })
        .unwrap();
        let pair = emitted.lock().unwrap().clone();
        assert!(pair[0].revision > pair[1].revision);
        assert!(!pair[0].projects[0].project.monitor_enabled);
        assert!(pair[1].projects[0].project.monitor_enabled);
        deliver_notifications(
            state,
            &fixture.messages,
            |_| Err(()),
            |older| {
                state.store()?.set_notifications(false)?;
                emit_snapshot(state, |newer| {
                    emitted.lock().unwrap().push(newer.clone());
                    Ok(())
                })?;
                emitted.lock().unwrap().push(older.clone());
                Ok(())
            },
        )
        .await
        .unwrap_err();
        let values = emitted.lock().unwrap();
        assert!(values[2].revision > values[3].revision);
        assert!(!values[2].system_notifications);
        assert!(values[3].system_notifications);
        assert!(values[2].projects[0].last_error.is_some());
        assert!(values[3].projects[0].last_error.is_some());
    }
    #[tokio::test]
    async fn task3_new_services_reject_shutdown_and_missing_projects_before_git() {
        let fixture = NotificationFixture::new(false);
        let state = fixture.state();
        assert_eq!(
            state
                .read_history("missing", 0, None)
                .await
                .unwrap_err()
                .code,
            "project_missing"
        );
        assert_eq!(
            state
                .preview_push_service("missing")
                .await
                .unwrap_err()
                .code,
            "project_missing"
        );
        assert_eq!(
            state
                .push_local_service("missing", "fp")
                .await
                .unwrap_err()
                .code,
            "project_missing"
        );
        state.shutdown.send_replace(true);
        assert_eq!(
            state.read_history("p", 0, None).await.unwrap_err().code,
            "shutting_down"
        );
        assert_eq!(
            state
                .configure_paths_service("p", Default::default())
                .await
                .unwrap_err()
                .code,
            "shutting_down"
        );
        let input = PublishInput {
            project_id: "p".into(),
            role: Role::Frontend,
            kind: "fix".into(),
            module: "page".into(),
            summary: "change".into(),
            fingerprint: "fp".into(),
        };
        assert_eq!(
            state.commit_local_service(input).await.unwrap_err().code,
            "shutting_down"
        );
    }
    fn offline_fixture() -> (NotificationFixture, Project, std::path::PathBuf) {
        let fixture = NotificationFixture::new(false);
        let root = fixture.directory.join("repo");
        std::fs::create_dir(&root).unwrap();
        local_git(&root, &["init", "--initial-branch=main"]);
        local_git(&root, &["config", "user.name", "Offline Test"]);
        local_git(&root, &["config", "user.email", "test@example.invalid"]);
        local_git(&root, &["config", "protocol.https.allow", "never"]);
        local_git(
            &root,
            &[
                "remote",
                "add",
                "origin",
                "https://github.com/offline/fixture.git",
            ],
        );
        let project = GitWorkspace::discover(&AddProjectInput {
            root: root.to_string_lossy().into(),
            name: "Offline".into(),
            branch: "main".into(),
            create_template: true,
        })
        .unwrap();
        local_git(&root, &["add", "."]);
        local_git(&root, &["commit", "-m", "initial"]);
        fixture
            .state()
            .store()
            .unwrap()
            .add_project(project.clone())
            .unwrap();
        (fixture, project, root)
    }
    fn local_input(project: &Project, root: &std::path::Path, text: &str) -> PublishInput {
        std::fs::write(root.join("frontend/page.txt"), text).unwrap();
        let preview = GitWorkspace::new(project.clone())
            .preview(Role::Frontend)
            .unwrap();
        PublishInput {
            project_id: project.id.clone(),
            role: Role::Frontend,
            kind: "fix".into(),
            module: "page".into(),
            summary: text.into(),
            fingerprint: preview.fingerprint,
        }
    }
    #[tokio::test]
    async fn task3_history_local_commit_and_settings_work_with_network_forbidden() {
        let (fixture, project, root) = offline_fixture();
        let state = fixture.state();
        let reading = state.read_history(&project.id, 0, None);
        tokio::pin!(reading);
        assert!(
            tokio::time::timeout(Duration::from_millis(3), &mut reading)
                .await
                .is_err()
        );
        assert!(
            state.locks.for_project(&project.id).try_lock().is_err(),
            "the actual read service owns its project lock while awaiting Git"
        );
        assert!(
            state.store.try_lock().is_ok(),
            "the Store must remain available while the service awaits its Git job"
        );
        let history = reading.await.unwrap();
        assert_eq!(history.commits.len(), 1);
        let first = state
            .commit_local_service(local_input(&project, &root, "first"))
            .await
            .unwrap();
        assert!(!first.pushed && !first.push_requested && first.error.is_none());
        let second = state
            .commit_local_service(local_input(&project, &root, "second"))
            .await
            .unwrap();
        assert_eq!(second.parent, first.commit);
        assert!(state.project(&project.id).unwrap().pending.is_none());
        let head = local_git(&root, &["rev-parse", "HEAD"]);
        let index = std::fs::read(root.join(".git/index")).unwrap();
        let mut paths = project.role_paths.clone();
        paths.insert(
            Role::Frontend,
            vec!["frontend".into(), "packages/ui".into()],
        );
        state
            .configure_paths_service(&project.id, paths.clone())
            .await
            .unwrap();
        assert_eq!(
            state.project(&project.id).unwrap().project.role_paths,
            paths
        );
        assert_eq!(local_git(&root, &["rev-parse", "HEAD"]), head);
        assert_eq!(std::fs::read(root.join(".git/index")).unwrap(), index);
        assert_eq!(
            state
                .project(&project.id)
                .unwrap()
                .last_receipt
                .unwrap()
                .commit,
            second.commit
        );
    }
    #[tokio::test]
    async fn task3_offline_recovery_must_save_before_new_head_and_settings_retry_is_idempotent() {
        let (mut fixture, project, root) = offline_fixture();
        let old = GitWorkspace::new(project.clone())
            .commit_local(&local_input(&project, &root, "first"))
            .unwrap();
        let next = local_input(&project, &root, "second");
        let path = fixture.directory.join("state.json");
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        let failure = fixture
            .state()
            .commit_local_service(next.clone())
            .await
            .unwrap_err();
        assert_eq!(failure.code, "recovery_required");
        assert!(failure.message.contains(&old.commit));
        assert_eq!(local_git(&root, &["rev-parse", "HEAD"]), old.commit);
        let mut paths = project.role_paths.clone();
        paths.insert(
            Role::Frontend,
            vec!["frontend".into(), "packages/ui".into()],
        );
        let failure = fixture
            .state()
            .configure_paths_service(&project.id, paths.clone())
            .await
            .unwrap_err();
        assert_eq!(failure.code, "settings_persistence");
        assert!(failure.message.contains("同一映射"));
        std::fs::remove_dir(&path).unwrap();
        fixture
            .state()
            .configure_paths_service(&project.id, paths.clone())
            .await
            .unwrap();
        assert_eq!(
            fixture
                .state()
                .project(&project.id)
                .unwrap()
                .project
                .role_paths,
            paths
        );
        // Use current mapping's preview after the successful settings retry.
        let updated = fixture.state().project(&project.id).unwrap().project;
        let next = local_input(&updated, &root, "second");
        fixture.state().commit_local_service(next).await.unwrap();
        fixture.state.take();
        let reopened = CollaborationStore::open(path).unwrap();
        let saved = reopened.snapshot();
        assert!(
            saved
                .projects
                .iter()
                .find(|p| p.project.id == project.id)
                .unwrap()
                .last_receipt
                .is_some()
        );
        drop(reopened);
    }

    #[tokio::test]
    async fn task3_queued_history_rechecks_removal() {
        let fixture = NotificationFixture::new(false);
        let state = fixture.state();
        let active = state.locks.for_project("p").lock_owned().await;
        let queued = state.read_history("p", 0, None);
        tokio::pin!(queued);
        assert!(
            tokio::time::timeout(Duration::from_millis(10), &mut queued)
                .await
                .is_err()
        );
        state.store().unwrap().remove_project("p").unwrap();
        drop(active);
        assert_eq!(queued.await.unwrap_err().code, "project_missing");
    }
    #[tokio::test]
    async fn task3_two_proof_reconciliation_must_save_before_advancing_head() {
        let (mut fixture, project, root) = offline_fixture();
        let old = GitWorkspace::new(project.clone())
            .commit_local(&local_input(&project, &root, "first"))
            .unwrap();
        let history = fixture
            .state()
            .read_history(&project.id, 0, None)
            .await
            .unwrap();
        // Core Task2 tests verify real range proof parsing/transport. This native
        // layer consumes its exact public receipt with real local Git metadata.
        let known = LocalPushReceipt {
            project_id: project.id.clone(),
            head: history.head.clone(),
            remote_head: None,
            url: format!(
                "https://github.com/{}/commit/{}",
                project.repository, history.head
            ),
            commits: history.commits,
            pushed: true,
            error: None,
            persistence_warning: None,
        };
        let path = fixture.directory.join("state.json");
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        {
            let (_operation, _) = fixture.state().locked_project(&project.id).await.unwrap();
            let failure = fixture
                .state()
                .persist_recovery(Some(old.clone()), Some(known.clone()))
                .unwrap_err();
            assert_eq!(failure.code, "recovery_required");
            assert_eq!(local_git(&root, &["rev-parse", "HEAD"]), old.commit);
        }
        std::fs::remove_dir(&path).unwrap();
        {
            let (_operation, _) = fixture.state().locked_project(&project.id).await.unwrap();
            // The first save can succeed while a second proof is rejected;
            // the combined action must still report failure and keep HEAD.
            let mut wrong = known.clone();
            wrong.project_id = "missing".into();
            assert_eq!(
                fixture
                    .state()
                    .persist_recovery(Some(old.clone()), Some(wrong))
                    .unwrap_err()
                    .code,
                "recovery_required"
            );
            assert_eq!(local_git(&root, &["rev-parse", "HEAD"]), old.commit);
            let recovered_role = fixture
                .state()
                .persist_recovery(Some(old.clone()), Some(known.clone()))
                .unwrap()
                .unwrap();
            assert!(
                recovered_role.pushed,
                "range reconciliation must return its covered role success to retry callers"
            );
        }
        let saved = fixture.state().project(&project.id).unwrap();
        assert_eq!(saved.last_local_push.as_ref().unwrap(), &known);
        let role = saved.last_receipt.unwrap();
        assert_eq!(role.commit, old.commit);
        assert!(role.pushed);
        assert!(!role.push_requested);
        let new = fixture
            .state()
            .commit_local_service(local_input(&project, &root, "second"))
            .await
            .unwrap();
        assert_eq!(new.parent, known.head);
        fixture.state.take();
        let reopened = CollaborationStore::open(path).unwrap();
        let saved = reopened.snapshot();
        let current = saved
            .projects
            .iter()
            .find(|s| s.project.id == project.id)
            .unwrap();
        assert_eq!(current.last_local_push.as_ref().unwrap(), &known);
        assert_eq!(current.last_receipt.as_ref().unwrap().commit, new.commit);
        assert!(current.pending.is_none());
        drop(reopened);
    }
    #[tokio::test]
    async fn task3_invalid_mapping_and_unresolved_receipt_refuse_without_head_changes() {
        let (fixture, project, root) = offline_fixture();
        let state = fixture.state();
        let head = local_git(&root, &["rev-parse", "HEAD"]);
        let config = std::fs::read(root.join("aijimu.workspace.json")).unwrap();
        let mut paths = project.role_paths.clone();
        paths.insert(Role::Frontend, vec!["../outside".into()]);
        assert!(
            state
                .configure_paths_service(&project.id, paths)
                .await
                .is_err()
        );
        assert_eq!(
            std::fs::read(root.join("aijimu.workspace.json")).unwrap(),
            config
        );
        assert_eq!(local_git(&root, &["rev-parse", "HEAD"]), head);
        let old = GitWorkspace::new(project.clone())
            .commit_local(&local_input(&project, &root, "first"))
            .unwrap();
        let proof = root.join(format!(".git/aijimu/{}.json", old.commit));
        let data = std::fs::read_to_string(&proof)
            .unwrap()
            .replace("\"pushRequested\":false", "\"pushRequested\":true");
        std::fs::write(proof, data).unwrap();
        assert_eq!(
            state
                .commit_local_service(local_input(&project, &root, "second"))
                .await
                .unwrap_err()
                .code,
            "pending_delivery"
        );
        assert_eq!(local_git(&root, &["rev-parse", "HEAD"]), old.commit);
        assert_eq!(
            state
                .configure_paths_service(&project.id, project.role_paths.clone())
                .await
                .unwrap_err()
                .code,
            "pending_delivery"
        );
        assert!(state.read_history(&project.id, 0, None).await.is_ok());
    }
}
