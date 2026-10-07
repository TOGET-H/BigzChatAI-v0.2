use crate::model::{default_role_paths, github_repository, validate_role_paths};
use crate::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
use tempfile::{NamedTempFile, TempDir};

const LIMIT: usize = 8 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(30);

/// A workspace is a binding, not an authentication or directory access boundary.
pub struct GitWorkspace {
    project: Project,
    #[cfg(test)]
    transport: Option<String>,
    #[cfg(test)]
    push_hook: Option<Box<dyn Fn() + Send + Sync>>,
    #[cfg(test)]
    cas_hook: Option<Box<dyn Fn() + Send + Sync>>,
    #[cfg(test)]
    history_hook: Option<Box<dyn Fn() + Send + Sync>>,
    #[cfg(test)]
    local_proof_hook: Option<Box<dyn Fn() + Send + Sync>>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Mapping {
    version: u32,
    role_paths: BTreeMap<Role, Vec<String>>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LocalPushProof {
    origin: String,
    branch: String,
    role_paths: BTreeMap<Role, Vec<String>>,
    fingerprint: String,
    receipt: LocalPushReceipt,
}
struct Captured {
    preview: ChangePreview,
    entries: BTreeMap<String, Option<(String, String)>>,
    tree: String,
}
struct Lock {
    path: PathBuf,
    cleanup: bool,
}
impl Drop for Lock {
    fn drop(&mut self) {
        if self.cleanup {
            let _ = fs::remove_file(&self.path);
        }
    }
}
fn io_error() -> SyncError {
    SyncError::new("filesystem_error", "无法读取或保存仓库文件，请检查权限")
}
fn git_error() -> SyncError {
    SyncError::new("git_failed", "Git 操作失败，请检查仓库、网络和已有凭据配置")
}
fn text(bytes: Vec<u8>) -> Result<String> {
    String::from_utf8(bytes)
        .map_err(|_| SyncError::new("invalid_encoding", "仓库路径或 Git 输出必须为 UTF-8"))
}
#[cfg(windows)]
struct WinHandle(windows_sys::Win32::Foundation::HANDLE);
#[cfg(windows)]
impl Drop for WinHandle {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}
#[cfg(windows)]
struct ProcessJob(WinHandle);
#[cfg(windows)]
impl ProcessJob {
    fn new() -> Result<Self> {
        use windows_sys::Win32::System::JobObjects::*;
        // The owned job kills all Git descendants when closed, including blocked SSH helpers.
        unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if handle.is_null() {
                return Err(SyncError::new(
                    "process_control_failed",
                    "无法建立 Git 进程隔离，请检查系统限制",
                ));
            }
            let job = Self(WinHandle(handle));
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                std::mem::size_of_val(&info) as u32,
            ) == 0
            {
                return Err(SyncError::new(
                    "process_control_failed",
                    "无法设置 Git 进程超时隔离",
                ));
            }
            Ok(job)
        }
    }
    fn attach_and_resume(&self, child: &std::process::Child) -> Result<()> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::{
            Foundation::INVALID_HANDLE_VALUE,
            System::{
                Diagnostics::ToolHelp::*,
                JobObjects::AssignProcessToJobObject,
                Threading::{OpenThread, ResumeThread, THREAD_SUSPEND_RESUME},
            },
        };
        let error = || {
            SyncError::new(
                "process_control_failed",
                "无法启动隔离 Git 进程，请检查系统限制",
            )
        };
        // CREATE_SUSPENDED ensures no helper can spawn before assignment to the job.
        unsafe {
            if AssignProcessToJobObject(self.0.0, child.as_raw_handle()) == 0 {
                return Err(error());
            }
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
            if snapshot == INVALID_HANDLE_VALUE {
                return Err(error());
            }
            let snapshot = WinHandle(snapshot);
            let mut entry: THREADENTRY32 = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;
            let mut present = Thread32First(snapshot.0, &mut entry);
            while present != 0 {
                if entry.th32OwnerProcessID == child.id() {
                    let handle = OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID);
                    if handle.is_null() {
                        return Err(error());
                    }
                    let thread = WinHandle(handle);
                    if ResumeThread(thread.0) == u32::MAX {
                        return Err(error());
                    }
                    return Ok(());
                }
                present = Thread32Next(snapshot.0, &mut entry);
            }
            Err(error())
        }
    }
}
#[cfg(unix)]
struct ProcessGroup(i32);
#[cfg(unix)]
impl Drop for ProcessGroup {
    fn drop(&mut self) {
        unsafe {
            libc::kill(-self.0, libc::SIGKILL);
        }
    }
}
fn read_pipe(
    mut pipe: impl Read,
    tx: mpsc::Sender<std::io::Result<(Vec<u8>, bool)>>,
    mut prepared: Option<mpsc::Sender<()>>,
) {
    let mut data = Vec::new();
    let mut overflow = false;
    let mut chunk = [0; 8192];
    let result = (|| {
        loop {
            let n = pipe.read(&mut chunk)?;
            if n == 0 {
                break;
            }
            let room = LIMIT.saturating_sub(data.len());
            data.extend_from_slice(&chunk[..n.min(room)]);
            overflow |= n > room;
            if prepared.is_some()
                && data
                    .windows(b"prepare: ok".len())
                    .any(|part| part == b"prepare: ok")
            {
                if let Some(sender) = prepared.take() {
                    let _ = sender.send(());
                }
            }
        }
        Ok((data, overflow))
    })();
    let _ = tx.send(result);
}
/// Arguments never pass through a shell. Neither stderr nor remote URLs are exposed.
fn run(
    root: &Path,
    args: &[&str],
    index: Option<&Path>,
    input: Option<Vec<u8>>,
) -> Result<Vec<u8>> {
    run_with_timeout(root, args, index, input, TIMEOUT)
}
fn run_with_timeout(
    root: &Path,
    args: &[&str],
    index: Option<&Path>,
    input: Option<Vec<u8>>,
    timeout: Duration,
) -> Result<Vec<u8>> {
    run_controlled(root, args, index, input, timeout, None)
}
fn run_controlled(
    root: &Path,
    args: &[&str],
    index: Option<&Path>,
    input: Option<Vec<u8>>,
    timeout: Duration,
    prepared: Option<&dyn Fn(Duration) -> Result<()>>,
) -> Result<Vec<u8>> {
    let output = run_process(root, args, index, input, timeout, prepared)?;
    if output.code != Some(0) {
        return Err(git_error());
    }
    Ok(output.stdout)
}
struct GitOutput {
    stdout: Vec<u8>,
    code: Option<i32>,
}
#[cfg(any(target_os = "macos", test))]
fn compatible_git_version(version: &str) -> bool {
    let Some(version) = version.trim().strip_prefix("git version ") else {
        return false;
    };
    let mut parts = version.split('.');
    match (
        parts.next().and_then(|s| s.parse::<u32>().ok()),
        parts.next().and_then(|s| s.parse::<u32>().ok()),
    ) {
        (Some(major), Some(minor)) => major > 2 || major == 2 && minor >= 51,
        _ => false,
    }
}
#[cfg(target_os = "macos")]
fn macos_git_environment() -> Result<(std::ffi::OsString, std::ffi::OsString)> {
    use std::os::unix::{fs::PermissionsExt, process::CommandExt};
    static EXECUTABLE: std::sync::OnceLock<Option<std::ffi::OsString>> = std::sync::OnceLock::new();
    // Finder/Dock does not inherit shell startup files. Prefer installed system
    // package-manager Git without editing PATH or Git configuration globally.
    let prefixes = ["/opt/homebrew/bin", "/usr/local/bin", "/opt/local/bin"];
    let mut paths: Vec<PathBuf> = Vec::new();
    if let Some(current) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&current));
    }
    paths.extend(prefixes.iter().map(PathBuf::from));
    // System SSH and credential helpers also remain available to Git children.
    paths.extend(
        ["/usr/bin", "/bin", "/usr/sbin", "/sbin"]
            .iter()
            .map(PathBuf::from),
    );
    let path = std::env::join_paths(&paths).map_err(|_| io_error())?;
    let executable = EXECUTABLE
        .get_or_init(|| {
            for candidate in paths.iter().map(|p| p.join("git")) {
                if !fs::metadata(&candidate)
                    .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
                {
                    continue;
                }
                let Ok(mut child) = Command::new(&candidate)
                    .arg("--version")
                    .env("PATH", &path)
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .process_group(0)
                    .spawn()
                else {
                    continue;
                };
                let group = ProcessGroup(child.id() as i32);
                let started = Instant::now();
                let success = loop {
                    match child.try_wait() {
                        Ok(Some(status)) => break status.success(),
                        Ok(None) if started.elapsed() < Duration::from_secs(2) => {
                            thread::sleep(Duration::from_millis(10))
                        }
                        _ => {
                            let _ = child.kill();
                            let _ = child.wait();
                            break false;
                        }
                    }
                };
                // A wrapper can exit while descendants still hold its output pipe.
                drop(group);
                if success {
                    let mut output = String::new();
                    if child
                        .stdout
                        .take()
                        .is_some_and(|out| out.take(4096).read_to_string(&mut output).is_ok())
                        && compatible_git_version(&output)
                    {
                        return Some(candidate.into_os_string());
                    }
                }
            }
            None
        })
        .clone()
        .ok_or_else(|| {
            SyncError::new(
                "git_unavailable",
                "找不到可执行的 Git 2.51+，请安装兼容 Git 后重新打开应用",
            )
        })?;
    if let Some(parent) = Path::new(&executable).parent() {
        paths.insert(0, parent.into());
    }
    Ok((
        executable,
        std::env::join_paths(paths).map_err(|_| io_error())?,
    ))
}
fn run_status(root: &Path, args: &[&str], input: Option<Vec<u8>>) -> Result<GitOutput> {
    run_process(root, args, None, input, TIMEOUT, None)
}
fn run_process(
    root: &Path,
    args: &[&str],
    index: Option<&Path>,
    input: Option<Vec<u8>>,
    timeout: Duration,
    prepared: Option<&dyn Fn(Duration) -> Result<()>>,
) -> Result<GitOutput> {
    let started = Instant::now();
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let (executable, path) = macos_git_environment()?;
        let mut command = Command::new(executable);
        command.env("PATH", path);
        command
    };
    #[cfg(not(target_os = "macos"))]
    let mut cmd = Command::new("git");
    cmd.args([
        "-c",
        "core.hooksPath=/dev/null",
        "-c",
        "color.ui=false",
        "-c",
        "core.quotePath=false",
    ])
    .args(args)
    .current_dir(root)
    .stdin(if input.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    })
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .env("GIT_TERMINAL_PROMPT", "0")
    .env("GCM_INTERACTIVE", "never")
    .env("GIT_NO_REPLACE_OBJECTS", "1");
    for key in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_COMMON_DIR",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_CONFIG_PARAMETERS",
        "GIT_CONFIG_COUNT",
        "GIT_NAMESPACE",
        "GIT_SHALLOW_FILE",
        "GIT_REPLACE_REF_BASE",
        "GIT_GRAFT_FILE",
        "GIT_CONFIG",
    ] {
        cmd.env_remove(key);
    }
    if let Some(index) = index {
        cmd.env("GIT_INDEX_FILE", index);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000 | 0x00000004);
    }
    #[cfg(windows)]
    let job = ProcessJob::new()?;
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = cmd
        .spawn()
        .map_err(|_| SyncError::new("git_unavailable", "找不到 Git，请安装 Git 并加入 PATH"))?;
    #[cfg(windows)]
    if let Err(error) = job.attach_and_resume(&child) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }
    #[cfg(unix)]
    let _group = ProcessGroup(child.id() as i32);
    let (out_tx, out_rx) = mpsc::channel();
    let (err_tx, err_rx) = mpsc::channel();
    let stdout = child.stdout.take().ok_or_else(git_error)?;
    let stderr = child.stderr.take().ok_or_else(git_error)?;
    let (prepared_tx, prepared_rx) = mpsc::channel();
    let prepared_output = prepared.is_some().then_some(prepared_tx);
    thread::spawn(move || read_pipe(stdout, out_tx, prepared_output));
    thread::spawn(move || read_pipe(stderr, err_tx, None));
    let mut rejected = None;
    if let Some(validate) = prepared {
        let mut stdin = child.stdin.take().ok_or_else(git_error)?;
        if stdin
            .write_all(input.as_deref().ok_or_else(git_error)?)
            .is_err()
        {
            let _ = child.kill();
            let _ = child.wait();
            return Err(git_error());
        }
        loop {
            if prepared_rx.try_recv().is_ok() {
                let remaining = timeout.saturating_sub(started.elapsed());
                match validate(remaining) {
                    Ok(()) => {
                        if stdin.write_all(b"commit\n").is_err() {
                            rejected = Some(git_error());
                        }
                    }
                    Err(error) => {
                        let _ = stdin.write_all(b"abort\n");
                        rejected = Some(error);
                    }
                }
                break;
            }
            if child.try_wait().map_err(|_| git_error())?.is_some() {
                rejected = Some(git_error());
                break;
            }
            if started.elapsed() >= timeout {
                drop(stdin);
                let _ = child.kill();
                let _ = child.wait();
                return Err(SyncError::new(
                    "git_timeout",
                    "Git 安全事务准备超时，请检查仓库后重试",
                ));
            }
            thread::sleep(Duration::from_millis(10));
        }
        drop(stdin);
    } else if let Some(data) = input {
        let mut stdin = child.stdin.take().ok_or_else(git_error)?;
        thread::spawn(move || {
            let _ = stdin.write_all(&data);
        });
    }
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|_| git_error())? {
            break status;
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(SyncError::new(
                "git_timeout",
                "Git 操作超过 30 秒，请检查网络后重试",
            ));
        }
        thread::sleep(Duration::from_millis(10));
    };
    let remaining = timeout.saturating_sub(started.elapsed());
    let (stdout, overflow) = out_rx
        .recv_timeout(remaining)
        .map_err(|_| SyncError::new("git_timeout", "Git 输出等待超时"))?
        .map_err(|_| git_error())?;
    let (_, err_overflow) = err_rx
        .recv_timeout(timeout.saturating_sub(started.elapsed()))
        .map_err(|_| SyncError::new("git_timeout", "Git 输出等待超时"))?
        .map_err(|_| git_error())?;
    if overflow || err_overflow {
        return Err(SyncError::new(
            "output_limit",
            "Git 输出超出安全上限，本次结果未完成，请缩小变更范围",
        ));
    }
    if let Some(error) = rejected {
        return Err(error);
    }
    Ok(GitOutput {
        stdout,
        code: status.code(),
    })
}
fn string(root: &Path, args: &[&str]) -> Result<String> {
    Ok(text(run(root, args, None, None)?)?
        .trim_end_matches(['\r', '\n'])
        .into())
}
fn sha_valid(sha: &str) -> bool {
    (sha.len() == 40 || sha.len() == 64) && sha.bytes().all(|b| b.is_ascii_hexdigit())
}
fn plain_path(path: &str) -> Result<()> {
    if path.is_empty()
        || path.contains(['\\', ':'])
        || path.chars().any(char::is_control)
        || path.split('/').any(|p| {
            p.is_empty()
                || p == "."
                || p == ".."
                || p.eq_ignore_ascii_case(".git")
                || p.ends_with(['.', ' '])
        })
    {
        return Err(SyncError::new(
            "unsafe_path",
            "候选路径不是仓库内的规范相对路径",
        ));
    }
    Ok(())
}
fn is_link(meta: &fs::Metadata) -> bool {
    if meta.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    false
}
fn safe_path(root: &Path, relative: &str) -> Result<PathBuf> {
    plain_path(relative)?;
    let mut path = root.to_path_buf();
    for part in relative.split('/') {
        path.push(part);
        match fs::symlink_metadata(&path) {
            Ok(meta) if is_link(&meta) => {
                return Err(SyncError::new(
                    "link_escape",
                    "候选路径包含符号链接或重解析点，请改用普通工作区文件",
                ));
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(io_error()),
        }
    }
    Ok(path)
}
fn sensitive(path: &str) -> bool {
    path.split('/').any(|part| {
        let part = part.to_ascii_lowercase();
        let example = part == ".env.example" || part == ".env.sample" || part == ".env.template";
        (!example && (part == ".env" || part.starts_with(".env.")))
            || [
                "node_modules",
                "vendor",
                ".git",
                "target",
                ".venv",
                "venv",
                "id_rsa",
                "id_dsa",
                "id_ecdsa",
                "id_ed25519",
            ]
            .contains(&part.as_str())
            || [".pem", ".key", ".p12", ".pfx", ".ppk"]
                .iter()
                .any(|ext| part.ends_with(ext))
    })
}
fn bounded_file(path: &Path) -> Result<Vec<u8>> {
    let meta = fs::symlink_metadata(path).map_err(|_| io_error())?;
    if !meta.is_file() || is_link(&meta) {
        return Err(SyncError::new(
            "unsafe_file",
            "仅支持普通文件，不能提交链接、子模块或特殊文件",
        ));
    }
    let mut data = Vec::new();
    File::open(path)
        .map_err(|_| io_error())?
        .take((LIMIT + 1) as u64)
        .read_to_end(&mut data)
        .map_err(|_| io_error())?;
    if data.len() > LIMIT {
        return Err(SyncError::new(
            "file_limit",
            "单个候选文件超过 8 MiB，请缩小变更范围",
        ));
    }
    Ok(data)
}
fn role_for(project: &Project, path: &str) -> Option<Role> {
    project.role_paths.iter().find_map(|(role, roots)| {
        roots
            .iter()
            .any(|p| path == p || path.starts_with(&format!("{p}/")))
            .then_some(*role)
    })
}
fn mapping(root: &Path) -> Result<BTreeMap<Role, Vec<String>>> {
    let path = safe_path(root, "aijimu.workspace.json")?;
    if !path.exists() {
        return Ok(default_role_paths());
    }
    let value: Mapping = serde_json::from_slice(&bounded_file(&path)?).map_err(|_| {
        SyncError::new(
            "invalid_mapping",
            "现有映射文件不是 AI积木 version 1 格式，请检查后重新登记",
        )
    })?;
    if value.version != 1 {
        return Err(SyncError::new("invalid_mapping", "不支持此映射文件版本"));
    }
    validate_role_paths(&value.role_paths)?;
    Ok(value.role_paths)
}
fn root_check(root: &Path) -> Result<PathBuf> {
    let canonical = fs::canonicalize(root)
        .map_err(|_| SyncError::new("invalid_root", "项目根目录不存在或无法访问"))?;
    if is_link(&fs::symlink_metadata(root).map_err(|_| io_error())?) {
        return Err(SyncError::new(
            "link_escape",
            "根目录不能是符号链接或重解析点",
        ));
    }
    let git_dir = root.join(".git");
    let meta = fs::symlink_metadata(&git_dir)
        .map_err(|_| SyncError::new("not_repository", "请先初始化普通 Git 仓库并配置 origin"))?;
    if !meta.is_dir() || is_link(&meta) {
        return Err(SyncError::new(
            "unsupported_worktree",
            "首版只支持 .git 位于根目录内的普通仓库",
        ));
    }
    if string(&canonical, &["rev-parse", "--is-shallow-repository"])? == "true" {
        return Err(SyncError::new(
            "unsupported_shallow",
            "浅仓库无法保证完整更新历史，请先使用 Git 补齐历史后重新登记",
        ));
    }
    let actual = fs::canonicalize(string(&canonical, &["rev-parse", "--show-toplevel"])?)
        .map_err(|_| io_error())?;
    let actual_git = fs::canonicalize(string(&canonical, &["rev-parse", "--absolute-git-dir"])?)
        .map_err(|_| io_error())?;
    if actual != canonical || actual_git != fs::canonicalize(git_dir).map_err(|_| io_error())? {
        return Err(SyncError::new("invalid_root", "请选择实际 Git 仓库根目录"));
    }
    for path in [
        ".git/index",
        ".git/objects",
        ".git/refs",
        ".git/HEAD",
        ".git/config",
        ".git/aijimu",
        ".git/aijimu-operation.lock",
    ] {
        let full = root.join(path);
        if let Ok(meta) = fs::symlink_metadata(full) {
            if is_link(&meta) {
                return Err(SyncError::new(
                    "link_escape",
                    "Git 管理路径包含链接，请使用普通仓库",
                ));
            }
        }
    }
    Ok(canonical)
}
fn missing_file(path: &Path, data: &[u8]) -> Result<()> {
    match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(mut file) => file.write_all(data).map_err(|_| io_error()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(_) => Err(io_error()),
    }
}
fn acquire(path: PathBuf) -> Result<Lock> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|_| SyncError::new("workspace_busy", "仓库正在执行其他 Git 操作，请稍后重试"))?;
    Ok(Lock {
        path,
        cleanup: true,
    })
}
impl GitWorkspace {
    pub fn discover(input: &AddProjectInput) -> Result<Project> {
        let root = root_check(Path::new(&input.root))?;
        let remote = string(&root, &["remote", "get-url", "origin"])?;
        let repository = github_repository(&remote)?;
        run(
            &root,
            &["check-ref-format", "--branch", &input.branch],
            None,
            None,
        )
        .map_err(|_| SyncError::new("invalid_branch", "请输入有效分支名"))?;
        if string(&root, &["symbolic-ref", "--short", "HEAD"])? != input.branch {
            return Err(SyncError::new(
                "branch_mismatch",
                "请先切换到登记的工作分支",
            ));
        }
        let role_paths = mapping(&root)?;
        validate_role_paths(&role_paths)?;
        if input.create_template {
            for dirs in role_paths.values() {
                for dir in dirs {
                    let path = safe_path(&root, dir)?;
                    fs::create_dir_all(&path).map_err(|_| io_error())?;
                    missing_file(
                        &path.join("AGENTS.md"),
                        b"# Role workspace\n\nKeep changes within this role directory.\n",
                    )?;
                }
            }
            missing_file(
                &root.join("AGENTS.md"),
                "# AI积木协作工作区\n\n按身份目录编辑文件，交付前检查变更。\n".as_bytes(),
            )?;
            let data = serde_json::to_vec_pretty(&Mapping {
                version: 1,
                role_paths: role_paths.clone(),
            })
            .map_err(|_| io_error())?;
            missing_file(&root.join("aijimu.workspace.json"), &data)?;
        }
        let root = root.to_string_lossy().into_owned();
        let id = hex::encode(Sha256::digest(format!(
            "{root}\0{repository}\0{}",
            input.branch
        )));
        Ok(Project {
            id,
            name: input.name.trim().into(),
            root,
            repository,
            remote_url: remote,
            branch: input.branch.clone(),
            role_paths,
            monitor_enabled: true,
        })
    }
    pub fn new(project: Project) -> Self {
        Self {
            project,
            #[cfg(test)]
            transport: None,
            #[cfg(test)]
            push_hook: None,
            #[cfg(test)]
            cas_hook: None,
            #[cfg(test)]
            history_hook: None,
            #[cfg(test)]
            local_proof_hook: None,
        }
    }
    fn root(&self) -> &Path {
        Path::new(&self.project.root)
    }
    fn transport(&self) -> &str {
        #[cfg(test)]
        if let Some(remote) = &self.transport {
            return remote;
        }
        &self.project.remote_url
    }
    fn verify_binding(&self) -> Result<()> {
        root_check(self.root())?;
        validate_role_paths(&self.project.role_paths)?;
        if github_repository(&self.project.remote_url)? != self.project.repository {
            return Err(SyncError::new(
                "remote_mismatch",
                "仓库绑定已变化，请重新登记",
            ));
        }
        if string(self.root(), &["symbolic-ref", "--short", "HEAD"])? != self.project.branch {
            return Err(SyncError::new(
                "branch_mismatch",
                "当前分支与登记分支不同，请先切换分支",
            ));
        }
        if string(self.root(), &["remote", "get-url", "origin"])? != self.transport() {
            return Err(SyncError::new(
                "remote_mismatch",
                "origin 已变化，请重新登记仓库",
            ));
        }
        Ok(())
    }
    fn verify(&self) -> Result<()> {
        self.verify_binding()?;
        if mapping(self.root())? != self.project.role_paths {
            return Err(SyncError::new(
                "mapping_changed",
                "身份映射已变化，请重新登记并检查变更",
            ));
        }
        for path in self.project.role_paths.values().flatten() {
            safe_path(self.root(), path)?;
        }
        Ok(())
    }
    fn head(&self) -> Result<String> {
        match string(self.root(), &["rev-parse", "--verify", "HEAD"]) {
            Ok(head) => Ok(head),
            Err(_) => {
                if string(self.root(), &["symbolic-ref", "--short", "HEAD"])? == self.project.branch
                {
                    let refs = string(self.root(), &["show-ref", "--head"]).unwrap_or_default();
                    if refs.is_empty() {
                        return Ok(String::new());
                    }
                }
                Err(git_error())
            }
        }
    }
    fn status(&self) -> Result<BTreeMap<String, String>> {
        let bytes = run(
            self.root(),
            &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
            None,
            None,
        )?;
        let mut entries = bytes.split(|b| *b == 0).filter(|e| !e.is_empty());
        let mut paths = BTreeMap::new();
        while let Some(entry) = entries.next() {
            if entry.len() < 4 {
                return Err(git_error());
            }
            let status = text(entry[..2].to_vec())?;
            let path = text(entry[3..].to_vec())?;
            plain_path(&path)?;
            if status.contains(['R', 'C']) {
                let source = text(entries.next().ok_or_else(git_error)?.to_vec())?;
                plain_path(&source)?;
                if role_for(&self.project, &source) != role_for(&self.project, &path) {
                    return Err(SyncError::new(
                        "cross_role_rename",
                        "检测到跨身份重命名，请分开处理后重新预览",
                    ));
                }
                if status.contains('R') {
                    paths.insert(source, "D".into());
                }
            }
            paths.insert(path, status);
        }
        Ok(paths)
    }
    fn mode(&self, path: &Path) -> Result<String> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            return Ok(if fs::metadata(path)
                .map_err(|_| io_error())?
                .permissions()
                .mode()
                & 0o111
                != 0
            {
                "100755"
            } else {
                "100644"
            }
            .into());
        }
        #[cfg(windows)]
        {
            let relative = path
                .strip_prefix(self.root())
                .map_err(|_| io_error())?
                .to_string_lossy()
                .replace('\\', "/");
            let staged = string(self.root(), &["ls-files", "--stage", "--", &relative])?;
            return Ok(if staged.starts_with("100755 ") {
                "100755"
            } else {
                "100644"
            }
            .into());
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = path;
            Ok("100644".into())
        }
    }
    fn init_index(&self, index: &Path, head: &str) -> Result<()> {
        if head.is_empty() {
            run(self.root(), &["read-tree", "--empty"], Some(index), None)?;
        } else {
            run(self.root(), &["read-tree", head], Some(index), None)?;
        }
        Ok(())
    }
    fn apply_entries(
        &self,
        index: &Path,
        entries: &BTreeMap<String, Option<(String, String)>>,
    ) -> Result<()> {
        for (path, entry) in entries {
            if let Some((mode, sha)) = entry {
                run(
                    self.root(),
                    &["update-index", "--add", "--cacheinfo", mode, sha, path],
                    Some(index),
                    None,
                )?;
            } else {
                run(
                    self.root(),
                    &["update-index", "--force-remove", "--", path],
                    Some(index),
                    None,
                )?;
            }
        }
        Ok(())
    }
    fn reject_filter_values(&self, values: &[&[u8]]) -> Result<()> {
        let external = || {
            SyncError::new(
                "external_filter",
                "仓库文件使用或可能使用外部 Git filter，请先用原生 Git 处理",
            )
        };
        if values
            .iter()
            .any(|value| *value != b"unset" && *value != b"unspecified")
        {
            return Err(external());
        }
        if values.is_empty() {
            return Ok(());
        }
        // check-attr renders both special states and literal driver names this way.
        // A configured matching driver makes the result ambiguous: refuse before status.
        let output = run_status(
            self.root(),
            &[
                "config",
                "--name-only",
                "--get-regexp",
                "^filter\\.(unset|unspecified)\\.(clean|smudge|process|required)$",
            ],
            None,
        )?;
        if output.code == Some(1) && output.stdout.is_empty() {
            return Ok(());
        }
        if output.code != Some(0) {
            return Err(git_error());
        }
        let keys = text(output.stdout)?;
        if values.iter().any(|value| {
            let prefix = if *value == b"unset" {
                "filter.unset."
            } else {
                "filter.unspecified."
            };
            keys.lines().any(|key| key.starts_with(prefix))
        }) {
            return Err(external());
        }
        Ok(())
    }
    fn reject_external_filters(&self) -> Result<()> {
        // ls-files/check-attr inspect names/attributes without running a clean driver.
        let paths = run(
            self.root(),
            &[
                "ls-files",
                "--cached",
                "--others",
                "--exclude-standard",
                "-z",
            ],
            None,
            None,
        )?;
        let output = run(
            self.root(),
            &["check-attr", "-z", "--stdin", "filter"],
            None,
            Some(paths),
        )?;
        let fields: Vec<_> = output
            .split(|b| *b == 0)
            .filter(|part| !part.is_empty())
            .collect();
        if fields.len() % 3 != 0 {
            return Err(git_error());
        }
        let values: Vec<_> = fields.chunks(3).map(|entry| entry[2]).collect();
        self.reject_filter_values(&values)
    }
    fn reject_incoming_filters(&self, tree: &str) -> Result<()> {
        let paths = run(
            self.root(),
            &["ls-tree", "-r", "--name-only", "-z", tree],
            None,
            None,
        )?;
        // --source reads the fetched attributes without checking files out; info
        // and configured global attributes retain their normal precedence.
        let source = format!("--source={tree}");
        let output = run(
            self.root(),
            &["check-attr", &source, "-z", "--stdin", "filter"],
            None,
            Some(paths),
        )?;
        let fields: Vec<_> = output
            .split(|b| *b == 0)
            .filter(|part| !part.is_empty())
            .collect();
        if fields.len() % 3 != 0 {
            return Err(git_error());
        }
        self.reject_filter_values(&fields.chunks(3).map(|entry| entry[2]).collect::<Vec<_>>())
    }
    fn conversion_config(&self) -> Result<BTreeMap<String, String>> {
        let output = run_status(
            self.root(),
            &[
                "config",
                "--get-regexp",
                "^core\\.(autocrlf|eol|safecrlf|checkroundtripencoding)$",
            ],
            None,
        )?;
        if output.code == Some(1) && output.stdout.is_empty() {
            return Ok(BTreeMap::new());
        }
        if output.code != Some(0) {
            return Err(git_error());
        }
        let mut config = BTreeMap::new();
        for line in text(output.stdout)?.lines() {
            let (key, value) = line.split_once(' ').ok_or_else(git_error)?;
            if ![
                "core.autocrlf",
                "core.eol",
                "core.safecrlf",
                "core.checkroundtripencoding",
            ]
            .contains(&key)
            {
                return Err(git_error());
            }
            config.insert(key.into(), value.into());
        }
        Ok(config)
    }
    fn canonical_blob(
        &self,
        path: &str,
        bytes: Vec<u8>,
        config: &BTreeMap<String, String>,
    ) -> Result<(String, Vec<u8>)> {
        let output = run(
            self.root(),
            &[
                "check-attr",
                "-z",
                "text",
                "eol",
                "ident",
                "working-tree-encoding",
                "crlf",
                "filter",
                "--",
                path,
            ],
            None,
            None,
        )?;
        let fields: Vec<_> = output
            .split(|b| *b == 0)
            .filter(|part| !part.is_empty())
            .collect();
        if fields.len() != 18 {
            return Err(git_error());
        }
        let mut attrs = BTreeMap::new();
        for entry in fields.chunks(3) {
            let key = text(entry[1].to_vec())?;
            let value = text(entry[2].to_vec())?;
            if value
                .chars()
                .any(|c| !c.is_ascii_alphanumeric() && !"-._/".contains(c))
            {
                return Err(SyncError::new(
                    "unsupported_attributes",
                    "Git 转换属性格式不受支持，请先用原生 Git 处理此文件",
                ));
            }
            attrs.insert(key, value);
        }
        self.reject_filter_values(&[attrs["filter"].as_bytes()])?;

        let policy = serde_json::to_vec(&(attrs.clone(), config)).map_err(|_| io_error())?;
        let transforms = attrs
            .iter()
            .any(|(key, value)| key != "filter" && value != "unset" && value != "unspecified")
            || config
                .get("core.autocrlf")
                .is_some_and(|value| !["false", "0", "no", "off"].contains(&value.as_str()));
        let canonical = if transforms {
            // Apply only builtins in an isolated repository. Its highest-priority rule forces filter unset.
            let isolated = TempDir::new().map_err(|_| io_error())?;
            run(
                isolated.path(),
                &["init", "--bare", "--template="],
                None,
                None,
            )?;
            fs::create_dir_all(isolated.path().join("info")).map_err(|_| io_error())?;
            let mut rule = String::from("input");
            for key in ["text", "eol", "ident", "working-tree-encoding", "crlf"] {
                let value = &attrs[key];
                rule.push(' ');
                match value.as_str() {
                    "set" => rule.push_str(key),
                    "unset" => rule.push_str(&format!("-{key}")),
                    "unspecified" => rule.push_str(&format!("!{key}")),
                    _ => rule.push_str(&format!("{key}={value}")),
                }
            }
            rule.push_str(" -filter\n");
            fs::write(isolated.path().join("info/attributes"), rule).map_err(|_| io_error())?;
            let mut args = vec![
                "-c".to_string(),
                "core.attributesFile=/dev/null".to_string(),
            ];
            for (key, value) in config {
                args.push("-c".into());
                args.push(format!("{key}={value}"));
            }
            args.extend(
                ["hash-object", "-w", "--path=input", "--stdin"]
                    .into_iter()
                    .map(String::from),
            );
            let refs: Vec<_> = args.iter().map(String::as_str).collect();
            let oid = text(run(isolated.path(), &refs, None, Some(bytes))?)?
                .trim()
                .to_string();
            if !sha_valid(&oid) {
                return Err(git_error());
            }
            run(isolated.path(), &["cat-file", "blob", &oid], None, None)?
        } else {
            bytes
        };
        let oid = text(run(
            self.root(),
            &["hash-object", "-w", "--no-filters", "--stdin"],
            None,
            Some(canonical),
        )?)?
        .trim()
        .to_string();
        Ok((oid, policy))
    }
    fn capture(&self, role: Role) -> Result<Captured> {
        self.verify()?;
        for marker in [
            "MERGE_HEAD",
            "CHERRY_PICK_HEAD",
            "REVERT_HEAD",
            "rebase-merge",
            "rebase-apply",
            "sequencer",
        ] {
            if self.root().join(".git").join(marker).exists() {
                return Err(SyncError::new(
                    "unfinished_operation",
                    "仓库有未完成的合并、rebase 或拣选，请先用 Git 完成或中止",
                ));
            }
        }
        let head = self.head()?;
        self.reject_external_filters()?;
        let config = self.conversion_config()?;
        let status = self.status()?;
        let mut hash = Sha256::new();
        hash.update(head.as_bytes());
        hash.update(role.as_str());
        hash.update(serde_json::to_vec(&self.project.role_paths).map_err(|_| io_error())?);
        let mut files = Vec::new();
        let mut entries = BTreeMap::new();
        let outside_count = status
            .keys()
            .filter(|path| role_for(&self.project, path) != Some(role))
            .count();
        for (path, state) in &status {
            if role_for(&self.project, path) != Some(role) {
                continue;
            }
            if sensitive(path) {
                return Err(SyncError::new(
                    "sensitive_path",
                    "候选范围含密钥、环境文件或依赖目录，请移除敏感变更后重试",
                ));
            }
            let full = safe_path(self.root(), path)?;
            let adding = state == "??" || state.contains(['A', 'R', 'C', 'M']);
            let deleting = !full.exists();
            if status.iter().any(|(other, other_state)| {
                role_for(&self.project, other) != Some(role)
                    && ((adding && other_state.contains('D'))
                        || (deleting
                            && (other_state == "??" || other_state.contains(['A', 'R', 'C', 'M']))))
            }) {
                return Err(SyncError::new(
                    "cross_role_rename",
                    "跨身份删除与新增或修改可能是文件移动，请先用 Git 拆分处理后重新预览",
                ));
            }
            hash.update(path.as_bytes());
            hash.update([0]);
            hash.update(state.as_bytes());
            let entry = if full.exists() {
                let bytes = bounded_file(&full)?;
                let mode = self.mode(&full)?;
                hash.update(mode.as_bytes());
                hash.update(&bytes);
                let (sha, policy) = self.canonical_blob(path, bytes, &config)?;
                hash.update(policy);
                Some((mode, sha))
            } else {
                hash.update(b"deleted");
                None
            };
            files.push(FileChange {
                path: path.clone(),
                status: if entry.is_none() {
                    "D".into()
                } else if state == "??" {
                    "A".into()
                } else {
                    state.trim().into()
                },
            });
            entries.insert(path.clone(), entry);
            hash.update([0]);
        }
        let temp = tempfile::tempdir().map_err(|_| io_error())?;
        let index = temp.path().join("index");
        self.init_index(&index, &head)?;
        self.apply_entries(&index, &entries)?;
        let tree: String = text(run(self.root(), &["write-tree"], Some(&index), None)?)?
            .trim()
            .into();
        let base = if head.is_empty() {
            text(run(
                self.root(),
                &["hash-object", "-t", "tree", "-w", "--stdin"],
                None,
                Some(Vec::new()),
            )?)?
            .trim()
            .into()
        } else {
            head.clone()
        };
        let diff = text(run(
            self.root(),
            &[
                "diff",
                "--no-ext-diff",
                "--no-textconv",
                "--binary",
                &base,
                &tree,
                "--",
            ],
            None,
            None,
        )?)?;
        if self.head()? != head {
            return Err(SyncError::new(
                "stale_preview",
                "HEAD 已变化，请重新检查变更",
            ));
        }
        Ok(Captured {
            preview: ChangePreview {
                project_id: self.project.id.clone(),
                role,
                head,
                fingerprint: hex::encode(hash.finalize()),
                files,
                outside_count,
                diff,
            },
            entries,
            tree,
        })
    }
    pub fn preview(&self, role: Role) -> Result<ChangePreview> {
        Ok(self.capture(role)?.preview)
    }
    fn operation(&self) -> Result<File> {
        // Stable inode + OS ownership: an exited process cannot strand this lock.
        // Never remove this file or any standard Git lock owned by another process.
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.root().join(".git/aijimu-operation.lock"))
            .map_err(|_| io_error())?;
        file.try_lock().map_err(|_| {
            SyncError::new("workspace_busy", "仓库正在执行其他 Git 操作，请稍后重试")
        })?;
        Ok(file)
    }
    fn fetch(&self) -> Result<Option<String>> {
        self.verify()?;
        let branch = format!("refs/heads/{}", self.project.branch);
        let output = string(
            self.root(),
            &["ls-remote", "--heads", "--", self.transport(), &branch],
        )?;
        if output.is_empty() {
            let heads = string(
                self.root(),
                &["ls-remote", "--heads", "--", self.transport()],
            )?;
            if heads.is_empty() {
                return Ok(None);
            }
            return Err(SyncError::new(
                "branch_missing",
                "远端目标分支不存在，请检查分支或权限",
            ));
        }
        let target = format!(
            "refs/aijimu/remotes/{}",
            hex::encode(Sha256::digest(self.project.branch.as_bytes()))
        );
        let refspec = format!("+{branch}:{target}");
        run(
            self.root(),
            &["fetch", "--no-tags", "--", self.transport(), &refspec],
            None,
            None,
        )?;
        let sha = string(self.root(), &["rev-parse", "--verify", &target])?;
        if !sha_valid(&sha) {
            return Err(git_error());
        }
        Ok(Some(sha))
    }
    fn commit_exists(&self, sha: &str) -> Result<bool> {
        if !sha_valid(sha) {
            return Err(SyncError::new(
                "invalid_sha",
                "提交标识无效，请重新检查远端",
            ));
        }
        let query = format!("{sha}^{{commit}}");
        let output = run_status(
            self.root(),
            &["cat-file", "--batch-check"],
            Some(format!("{query}\n").into_bytes()),
        )?;
        if output.code != Some(0) {
            return Err(git_error());
        }
        let output = text(output.stdout)?;
        if output.trim_end_matches(['\r', '\n']) == format!("{query} missing") {
            return Ok(false);
        }
        let fields: Vec<_> = output.split_whitespace().collect();
        if fields.len() != 3
            || !sha_valid(fields[0])
            || fields[1] != "commit"
            || fields[2].parse::<usize>().is_err()
        {
            return Err(git_error());
        }
        Ok(true)
    }
    fn ancestor(&self, older: &str, newer: &str) -> Result<bool> {
        if !self.commit_exists(older)? || !self.commit_exists(newer)? {
            return Err(SyncError::new(
                "missing_object",
                "祖先检查所需提交不存在，请重新检查完整仓库历史",
            ));
        }
        let output = run_status(
            self.root(),
            &["merge-base", "--is-ancestor", older, newer],
            None,
        )?;
        match output.code {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => Err(git_error()),
        }
    }
    fn aligned(&self, head: &str, remote: Option<&str>) -> Result<()> {
        if remote == Some(head) || remote.is_none() && head.is_empty() {
            return Ok(());
        }
        let code = match remote {
            None => "local_ahead",
            Some(_) if head.is_empty() => "remote_ahead",
            Some(remote) if self.ancestor(remote, head)? => "local_ahead",
            Some(remote) if self.ancestor(head, remote)? => "remote_ahead",
            Some(_) => "diverged",
        };
        Err(SyncError::new(
            code,
            "本地与远端基线不同，请处理未推送提交或先拉取更新",
        ))
    }
    fn receipt_path(&self, commit: &str) -> Result<PathBuf> {
        if !sha_valid(commit) {
            return Err(SyncError::new("invalid_receipt", "交付回执的提交标识无效"));
        }
        Ok(self
            .root()
            .join(".git/aijimu")
            .join(format!("{commit}.json")))
    }
    fn save_receipt(&self, receipt: &PublishReceipt) -> Result<()> {
        let dir = self.root().join(".git/aijimu");
        fs::create_dir_all(&dir).map_err(|_| io_error())?;
        let data = serde_json::to_vec(receipt).map_err(|_| io_error())?;
        let mut temp = NamedTempFile::new_in(dir).map_err(|_| io_error())?;
        temp.write_all(&data).map_err(|_| io_error())?;
        temp.as_file().sync_all().map_err(|_| io_error())?;
        temp.persist(self.receipt_path(&receipt.commit)?)
            .map_err(|_| io_error())?;
        Ok(())
    }
    fn persist_outcome(&self, mut receipt: PublishReceipt) -> PublishReceipt {
        receipt.persistence_warning = None;
        if let Err(failure) = self.save_receipt(&receipt) {
            receipt.persistence_warning = Some(format!(
                "交付结果已知，但仓库回执保存失败；请恢复权限后检查远端以恢复 SHA {}：{}",
                receipt.commit, failure.message
            ));
        }
        receipt
    }
    /// Recover only a proof attached to the actual bound HEAD. Prepared orphan
    /// proofs are deliberately not scanned or replayed. No commit/push/index edit.
    pub fn recover_receipt(&self) -> Result<Option<PublishReceipt>> {
        self.verify()?;
        let _operation = self.operation()?;
        let head = self.head()?;
        if head.is_empty() {
            return Ok(None);
        }
        let path = self.receipt_path(&head)?;
        if !path.exists() {
            return Ok(None);
        }
        let mut receipt: PublishReceipt = serde_json::from_slice(&bounded_file(&path)?)
            .map_err(|_| SyncError::new("invalid_receipt", "交付回执记录损坏，请检查仓库"))?;
        self.validate_receipt(&receipt, &head)?;
        if !receipt.push_requested {
            return Ok(Some(receipt));
        }
        match self.fetch() {
            Ok(Some(remote)) if remote == head || self.ancestor(&head, &remote)? => {
                receipt.pushed = true;
                receipt.error = None;
            }
            Ok(_) => {
                if !receipt.pushed && receipt.error.is_none() {
                    receipt.error = Some("已恢复本地交付，远端尚未确认，请检查或重试推送".into());
                }
            }
            Err(failure) => {
                receipt.persistence_warning = Some(format!(
                    "本地交付已恢复，远端结果暂无法核实：{}",
                    failure.message
                ));
                return Ok(Some(receipt));
            }
        }
        Ok(Some(self.persist_outcome(receipt)))
    }
    fn validate_receipt(&self, receipt: &PublishReceipt, head: &str) -> Result<()> {
        let body = string(self.root(), &["show", "-s", "--format=%B", head])?;
        let parents = string(self.root(), &["show", "-s", "--format=%P", head])?;
        let prefix = format!(
            "{}\n\nAI-Jimu-Role: {}\nAI-Jimu-Delivery: ",
            receipt.message,
            receipt.role.as_str()
        );
        let delivery = body
            .strip_prefix(&prefix)
            .and_then(|s| uuid::Uuid::parse_str(s.trim()).ok());
        if receipt.project_id != self.project.id
            || receipt.commit != head
            || receipt.parent != parents
            || receipt.url
                != format!(
                    "https://github.com/{}/commit/{head}",
                    self.project.repository
                )
            || parse_commit(&receipt.message).is_none_or(|parsed| parsed.role != receipt.role)
            || delivery.is_none()
        {
            return Err(SyncError::new(
                "invalid_receipt",
                "本地提交不符合绑定的交付记录",
            ));
        }
        Ok(())
    }
    fn push_receipt(&self, mut receipt: PublishReceipt) -> Result<PublishReceipt> {
        #[cfg(test)]
        if let Some(hook) = &self.push_hook {
            hook();
        }
        if let Err(error) = self.verify() {
            receipt.pushed = false;
            receipt.error = Some(format!("{}: {}", error.code, error.message));
            // The durable pending proof was saved before HEAD changed. Do not write through an invalid root.
            return Ok(receipt);
        }
        let refspec = format!("{}:refs/heads/{}", receipt.commit, self.project.branch);
        match run(
            self.root(),
            &["push", "--no-verify", "--", self.transport(), &refspec],
            None,
            None,
        ) {
            Ok(_) => {
                receipt.pushed = true;
                receipt.error = None;
            }
            Err(error) => {
                receipt.pushed = false;
                receipt.error = Some(format!("{}: {}", error.code, error.message));
            }
        }
        Ok(self.persist_outcome(receipt))
    }
    pub fn publish(&self, input: &PublishInput) -> Result<PublishReceipt> {
        self.commit_role(input, true)
    }
    pub fn commit_local(&self, input: &PublishInput) -> Result<PublishReceipt> {
        self.commit_role(input, false)
    }
    fn commit_role(&self, input: &PublishInput, push_requested: bool) -> Result<PublishReceipt> {
        self.verify().map_err(|e| {
            if e.code == "mapping_changed" {
                SyncError::new("stale_preview", "身份映射已变化，请重新登记并预览")
            } else {
                e
            }
        })?;
        let _operation = self.operation()?;
        if input.project_id != self.project.id {
            return Err(SyncError::new("project_mismatch", "交付项目与当前仓库不同"));
        }
        self.reject_unresolved_delivery()?;
        let message = commit_title(&input.kind, input.role, &input.module, &input.summary)?;
        let captured = self.capture(input.role)?;
        if captured.preview.fingerprint != input.fingerprint {
            return Err(SyncError::new(
                "stale_preview",
                "文件或 HEAD 已变化，请重新检查变更",
            ));
        }
        if captured.preview.files.is_empty() || captured.preview.diff.is_empty() {
            return Err(SyncError::new("no_changes", "当前身份没有可交付变更"));
        }
        let parent = captured.preview.head;
        if push_requested {
            let remote = self.fetch()?;
            self.aligned(&parent, remote.as_deref())?;
        }
        if self.capture(input.role)?.preview.fingerprint != input.fingerprint {
            return Err(SyncError::new(
                "stale_preview",
                "检查远端期间文件已变化，请重新预览",
            ));
        }
        // Holding the real index lock prevents other staging operations. Prepare its replacement before changing HEAD.

        let index_path = self.root().join(".git/index");
        let mut index_lock = acquire(self.root().join(".git/index.lock"))?;
        let temp = TempDir::new().map_err(|_| io_error())?;
        let prepared = temp.path().join("index");
        if index_path.exists() {
            fs::copy(&index_path, &prepared).map_err(|_| io_error())?;
        } else {
            self.init_index(&prepared, &parent)?;
        }
        self.apply_entries(&prepared, &captured.entries)?;
        let body = format!(
            "{message}\n\nAI-Jimu-Role: {}\nAI-Jimu-Delivery: {}\n",
            input.role.as_str(),
            uuid::Uuid::new_v4()
        );
        let mut args = vec!["commit-tree", &captured.tree];
        if !parent.is_empty() {
            args.extend(["-p", &parent]);
        }
        let commit = text(run(self.root(), &args, None, Some(body.into_bytes()))?)?
            .trim()
            .into();
        let mut receipt = PublishReceipt {
            push_requested,
            persistence_warning: None,
            project_id: self.project.id.clone(),
            role: input.role,
            commit,
            parent,
            message,
            url: String::new(),
            pushed: false,
            error: None,
        };
        receipt.url = format!(
            "https://github.com/{}/commit/{}",
            self.project.repository, receipt.commit
        );
        self.save_receipt(&receipt)?;
        self.verify()?;
        if self.capture(input.role)?.preview.fingerprint != input.fingerprint {
            return Err(SyncError::new(
                "stale_preview",
                "文件已变化，请重新检查变更",
            ));
        }
        let branch = format!("refs/heads/{}", self.project.branch);
        #[cfg(test)]
        if let Some(hook) = &self.cas_hook {
            hook();
        }
        let old = if receipt.parent.is_empty() {
            "0".repeat(receipt.commit.len())
        } else {
            receipt.parent.clone()
        };
        // prepare locks HEAD and whichever branch it currently names. Validate the exact binding while locked.
        let transaction = format!("start\nupdate HEAD {} {old}\nprepare\n", receipt.commit);
        let validate_binding = |remaining: Duration| {
            let actual = text(run_with_timeout(
                self.root(),
                &["symbolic-ref", "-q", "HEAD"],
                None,
                None,
                remaining,
            )?)?;
            if actual.trim() != branch {
                return Err(SyncError::new(
                    "head_changed",
                    "工作分支绑定已变化，请重新检查分支和变更",
                ));
            }
            Ok(())
        };
        run_controlled(
            self.root(),
            &["update-ref", "-m", "AI-Jimu role delivery", "--stdin"],
            None,
            Some(transaction.into_bytes()),
            TIMEOUT,
            Some(&validate_binding),
        )
        .map_err(|error| {
            if error.code == "git_failed" {
                SyncError::new(
                    "head_changed",
                    "分支已变化或 Git 不支持安全事务，请检查分支并使用 Git 2.51 或更新版本",
                )
            } else {
                error
            }
        })?;
        if fs::copy(&prepared, &index_lock.path)
            .and_then(|_| fs::rename(&index_lock.path, &index_path))
            .is_err()
        {
            receipt.error = Some(
                "index_update_failed: 本地提交已保留，暂存区更新失败，请检查 Git 状态后恢复".into(),
            );
            return Ok(self.persist_outcome(receipt));
        }

        // Our index lock was renamed; a later file at that path belongs to Git.
        index_lock.cleanup = false;
        if push_requested {
            self.push_receipt(receipt)
        } else {
            Ok(receipt)
        }
    }
    pub fn retry_push(&self, receipt: &PublishReceipt) -> Result<PublishReceipt> {
        self.verify()?;
        let _operation = self.operation()?;
        let path = self.receipt_path(&receipt.commit)?;
        let saved: PublishReceipt = serde_json::from_slice(
            &bounded_file(&path)
                .map_err(|_| SyncError::new("invalid_receipt", "未找到本应用记录的交付回执"))?,
        )
        .map_err(|_| SyncError::new("invalid_receipt", "交付回执记录损坏，请检查仓库"))?;
        if !saved.push_requested
            || !receipt.push_requested
            || saved.project_id != self.project.id
            || saved.commit != receipt.commit
            || saved.parent != receipt.parent
            || saved.role != receipt.role
            || saved.message != receipt.message
            || saved.url != receipt.url
        {
            return Err(SyncError::new(
                "invalid_receipt",
                "回执与本应用记录不一致，拒绝推送",
            ));
        }
        if self.head()? != receipt.commit {
            return Err(SyncError::new(
                "head_changed",
                "本地 HEAD 已变化，不能重试此交付",
            ));
        }
        let body = string(self.root(), &["show", "-s", "--format=%B", &receipt.commit])?;
        let parents = string(self.root(), &["show", "-s", "--format=%P", &receipt.commit])?;
        if parents != receipt.parent
            || !body.starts_with(&format!(
                "{}\n\nAI-Jimu-Role: {}\nAI-Jimu-Delivery: ",
                receipt.message,
                receipt.role.as_str()
            ))
        {
            return Err(SyncError::new("invalid_receipt", "本地提交不符合交付记录"));
        }
        let remote = self.fetch()?;
        if remote.as_deref() == Some(&receipt.commit) {
            let mut done = saved;
            done.pushed = true;
            done.error = None;
            return Ok(self.persist_outcome(done));
        }
        if remote.as_deref() != (!receipt.parent.is_empty()).then_some(receipt.parent.as_str()) {
            return Err(SyncError::new(
                "remote_changed",
                "远端基线已变化，请检查后手动处理此交付",
            ));
        }
        self.push_receipt(saved)
    }
    /// Only local objects are read, including when the configured remote is offline.
    pub fn local_history(
        &self,
        offset: usize,
        expected_head: Option<&str>,
    ) -> Result<LocalHistoryPage> {
        self.verify()?;
        let head = self.head()?;
        if (offset > 0 && expected_head.is_none()) || expected_head.is_some_and(|h| h != head) {
            return Err(SyncError::new(
                "head_changed",
                "HEAD 已变化，请重新读取第一页历史",
            ));
        }
        let mut commits = if head.is_empty() {
            Vec::new()
        } else {
            self.read_commits(&head, Some(offset))?
        };
        #[cfg(test)]
        if let Some(hook) = &self.history_hook {
            hook();
        }
        self.verify()?;
        if self.head()? != head {
            return Err(SyncError::new(
                "head_changed",
                "读取期间 HEAD 已变化，请重新读取历史",
            ));
        }
        let next_offset = if commits.len() > 50 {
            commits.truncate(50);
            Some(offset.checked_add(50).ok_or_else(git_error)?)
        } else {
            None
        };
        Ok(LocalHistoryPage {
            head,
            commits,
            next_offset,
        })
    }
    fn read_commits(&self, range: &str, offset: Option<usize>) -> Result<Vec<LocalCommit>> {
        let skip = format!("--skip={}", offset.unwrap_or(0));
        let mut args = vec![
            "log",
            "--no-show-signature",
            "--no-notes",
            "-z",
            "--format=%H%x00%P%x00%s%x00%b%x00%an%x00%cI",
            "--topo-order",
        ];
        if offset.is_some() {
            args.extend(["--max-count=51", &skip]);
        } else {
            args.push("--reverse");
        }
        args.extend([range, "--"]);
        let output = text(run(self.root(), &args, None, None)?)?;
        if output.is_empty() {
            return Ok(Vec::new());
        }
        let fields: Vec<_> = output
            .strip_suffix('\0')
            .ok_or_else(git_error)?
            .split('\0')
            .collect();
        if !fields.len().is_multiple_of(6) {
            return Err(git_error());
        }
        fields
            .chunks_exact(6)
            .map(|f| {
                let parents: Vec<String> = f[1].split_whitespace().map(str::to_owned).collect();
                if !sha_valid(f[0]) || parents.iter().any(|p| !sha_valid(p)) {
                    return Err(git_error());
                }
                Ok(LocalCommit {
                    sha: f[0].into(),
                    parents,
                    title: f[2].into(),
                    body: f[3].into(),
                    author: f[4].into(),
                    committed_at: f[5].into(),
                    parsed: parse_commit(f[2]),
                })
            })
            .collect()
    }
    fn local_range(&self, head: &str, remote: Option<&str>) -> Result<Vec<LocalCommit>> {
        if head.is_empty() {
            return if remote.is_some() {
                Err(SyncError::new("remote_ahead", "远端领先，请先拉取更新"))
            } else {
                Ok(Vec::new())
            };
        }
        if !self.commit_exists(head)? {
            return Err(git_error());
        }
        let range = if let Some(remote) = remote {
            if !self.ancestor(remote, head)? {
                let code = if self.ancestor(head, remote)? {
                    "remote_ahead"
                } else {
                    "diverged"
                };
                return Err(SyncError::new(code, "远端领先或历史分叉，请先处理分支"));
            }
            format!("{remote}..{head}")
        } else {
            head.into()
        };
        let commits = self.read_commits(&range, None)?;
        // Check changed paths of each new commit against every merge parent.
        // This catches intermediate secrets subsequently deleted without rejecting
        // unchanged objects already present at the remote baseline. No index access.
        for commit in &commits {
            let paths = text(run(
                self.root(),
                &[
                    "diff-tree",
                    "--root",
                    "-r",
                    "-m",
                    "--no-renames",
                    "--diff-filter=ACMT",
                    "--no-commit-id",
                    "--name-only",
                    "-z",
                    &commit.sha,
                    "--",
                ],
                None,
                None,
            )?)?;
            for path in paths.split('\0').filter(|p| !p.is_empty()) {
                plain_path(path)?;
                if sensitive(path) {
                    return Err(SyncError::new(
                        "sensitive_path",
                        "待推送历史包含敏感路径或依赖目录，请先处理提交历史",
                    ));
                }
            }
        }
        Ok(commits)
    }
    fn push_fingerprint(
        &self,
        head: &str,
        remote: Option<&str>,
        commits: &[LocalCommit],
    ) -> Result<String> {
        self.push_fingerprint_with_paths(head, remote, commits, &self.project.role_paths)
    }
    fn push_fingerprint_with_paths(
        &self,
        head: &str,
        remote: Option<&str>,
        commits: &[LocalCommit],
        paths: &BTreeMap<Role, Vec<String>>,
    ) -> Result<String> {
        let bytes = serde_json::to_vec(&(
            &self.project.id,
            &self.project.repository,
            self.transport(),
            &self.project.branch,
            paths,
            head,
            remote,
            commits,
        ))
        .map_err(|_| io_error())?;
        Ok(hex::encode(Sha256::digest(bytes)))
    }
    fn push_preview(&self) -> Result<LocalPushPreview> {
        self.verify()?;
        let head = self.head()?;
        let remote_head = self.fetch()?;
        let commits = self.local_range(&head, remote_head.as_deref())?;
        self.verify()?;
        if self.head()? != head {
            return Err(SyncError::new(
                "head_changed",
                "HEAD 已变化，请重新检查待推送提交",
            ));
        }
        let fingerprint = self.push_fingerprint(&head, remote_head.as_deref(), &commits)?;
        Ok(LocalPushPreview {
            project_id: self.project.id.clone(),
            head,
            remote_head,
            fingerprint,
            commits,
        })
    }
    pub fn preview_local_push(&self) -> Result<LocalPushPreview> {
        self.verify()?;
        let _operation = self.operation()?;
        self.push_preview()
    }
    fn local_proof_path(&self, head: &str) -> Result<PathBuf> {
        if !sha_valid(head) {
            return Err(SyncError::new(
                "invalid_receipt",
                "范围推送证明的 HEAD 无效",
            ));
        }
        Ok(self
            .root()
            .join(".git/gitcollab/local-push")
            .join(format!("{head}.json")))
    }
    fn read_local_proof(&self, head: &str) -> Result<Option<LocalPushProof>> {
        if head.is_empty() {
            return Ok(None);
        }
        let path = self.local_proof_path(head)?;
        if !path.exists() {
            return Ok(None);
        }
        let proof: LocalPushProof = serde_json::from_slice(&bounded_file(&path)?)
            .map_err(|_| SyncError::new("invalid_receipt", "范围推送证明损坏，请检查仓库"))?;
        let r = &proof.receipt;
        if proof.origin != self.transport()
            || proof.branch != self.project.branch
            || r.project_id != self.project.id
            || r.head != head
            || r.url
                != format!(
                    "https://github.com/{}/commit/{head}",
                    self.project.repository
                )
            || r.commits.is_empty()
            || self.local_range(head, r.remote_head.as_deref())? != r.commits
            || self.push_fingerprint_with_paths(
                head,
                r.remote_head.as_deref(),
                &r.commits,
                &proof.role_paths,
            )? != proof.fingerprint
        {
            return Err(SyncError::new(
                "invalid_receipt",
                "范围推送证明与绑定或实际对象不一致",
            ));
        }
        Ok(Some(proof))
    }
    fn save_local_proof(&self, proof: &LocalPushProof) -> Result<()> {
        let path = self.local_proof_path(&proof.receipt.head)?;
        let dir = path.parent().ok_or_else(io_error)?;
        let data = serde_json::to_vec(proof).map_err(|_| io_error())?;
        if data.len() > LIMIT {
            return Err(SyncError::new(
                "output_limit",
                "完整范围证明超过 8 MiB，请缩小待推送历史",
            ));
        }
        fs::create_dir_all(dir).map_err(|_| io_error())?;
        let mut temp = NamedTempFile::new_in(dir).map_err(|_| io_error())?;
        temp.write_all(&data).map_err(|_| io_error())?;
        temp.as_file().sync_all().map_err(|_| io_error())?;
        temp.persist(path).map_err(|_| io_error())?;
        Ok(())
    }
    fn persist_local_outcome(&self, mut proof: LocalPushProof) -> LocalPushReceipt {
        proof.receipt.persistence_warning = None;
        if let Err(failure) = self.save_local_proof(&proof) {
            proof.receipt.persistence_warning = Some(format!(
                "推送结果已知，但仓库证明保存失败；请恢复权限后检查 SHA {}：{}",
                proof.receipt.head, failure.message
            ));
        }
        proof.receipt
    }
    fn successful_local_push(&self, head: &str) -> Result<Option<LocalPushReceipt>> {
        let Some(proof) = self.read_local_proof(head)? else {
            return Ok(None);
        };
        if !proof.receipt.pushed {
            return Err(SyncError::new(
                "pending_delivery",
                "当前 HEAD 有未解决的范围推送，请先检查或重试",
            ));
        }
        Ok(Some(proof.receipt))
    }
    /// Return the complete already-successful current-HEAD proof without transport.
    /// Callers must reconcile it durably before allowing a new commit to orphan it.
    pub fn recover_local_push_offline(&self) -> Result<Option<LocalPushReceipt>> {
        self.verify()?;
        let _operation = self.operation()?;
        self.successful_local_push(&self.head()?)
    }
    pub fn recover_local_push(&self) -> Result<Option<LocalPushReceipt>> {
        self.verify()?;
        let _operation = self.operation()?;
        let Some(mut proof) = self.read_local_proof(&self.head()?)? else {
            return Ok(None);
        };
        match self.fetch().and_then(|remote| match remote {
            Some(remote) => self.ancestor(&proof.receipt.head, &remote),
            None => Ok(false),
        }) {
            Ok(true) => {
                proof.receipt.pushed = true;
                proof.receipt.error = None;
            }
            Ok(false) => {
                if !proof.receipt.pushed && proof.receipt.error.is_none() {
                    proof.receipt.error =
                        Some("已恢复待推送范围，远端尚未确认，请检查或重试".into());
                }
            }
            Err(failure) => {
                proof.receipt.persistence_warning = Some(format!(
                    "本地范围证明已恢复，远端结果暂无法核实：{}",
                    failure.message
                ));
                return Ok(Some(proof.receipt));
            }
        }
        Ok(Some(self.persist_local_outcome(proof)))
    }
    pub fn push_local_commits(&self, fingerprint: &str) -> Result<LocalPushReceipt> {
        self.verify()?;
        let _operation = self.operation()?;
        let head = self.head()?;
        // A successful durable proof is monotonic and makes an identical retry a no-op.
        let saved = self.read_local_proof(&head)?;
        if let Some(proof) = &saved {
            if proof.fingerprint == fingerprint && proof.receipt.pushed {
                return Ok(proof.receipt.clone());
            }
        }
        #[cfg(test)]
        if let Some(hook) = &self.push_hook {
            hook();
        }
        self.verify()?;
        if self.head()? != head {
            return Err(SyncError::new(
                "head_changed",
                "HEAD 已变化，请重新检查待推送范围",
            ));
        }
        let preview = self.push_preview()?;
        if saved.as_ref().is_some_and(|proof| {
            proof.fingerprint == fingerprint
                && preview.remote_head.as_ref().is_some_and(|r| r == &head)
        }) {
            let mut proof = saved.expect("matching saved proof");
            proof.receipt.pushed = true;
            proof.receipt.error = None;
            return Ok(self.persist_local_outcome(proof));
        }
        if preview.fingerprint != fingerprint {
            return Err(SyncError::new(
                "stale_preview",
                "推送范围或远端基线已变化，请重新检查待推送提交",
            ));
        }
        if preview.commits.is_empty() {
            return Err(SyncError::new("no_changes", "当前没有待推送提交"));
        }
        // A new valid preview cannot downgrade an already verified same-HEAD success.
        // Return historical metadata, never rewrite its proof or repeat its upload.
        if let Some(proof) = saved.filter(|proof| proof.receipt.pushed) {
            let mut receipt = proof.receipt;
            let warning = "当前 HEAD 已有成功推送记录；回执保留原成功范围与远端基线，不代表当前远端状态；本次未重复上传。";
            receipt.persistence_warning = Some(match receipt.persistence_warning.take() {
                Some(previous) => format!("{previous}；{warning}"),
                None => warning.into(),
            });
            return Ok(receipt);
        }
        let mut proof = LocalPushProof {
            origin: self.transport().into(),
            branch: self.project.branch.clone(),
            role_paths: self.project.role_paths.clone(),
            fingerprint: preview.fingerprint,
            receipt: LocalPushReceipt {
                project_id: self.project.id.clone(),
                head: head.clone(),
                remote_head: preview.remote_head,
                commits: preview.commits,
                url: format!(
                    "https://github.com/{}/commit/{head}",
                    self.project.repository
                ),
                pushed: false,
                error: None,
                persistence_warning: None,
            },
        };
        self.save_local_proof(&proof)?;
        #[cfg(test)]
        if let Some(hook) = &self.local_proof_hook {
            hook();
        }
        self.verify()?;
        if self.head()? != head {
            return Err(SyncError::new(
                "head_changed",
                "HEAD 已变化，请重新检查待推送范围",
            ));
        }
        let remote = self.fetch()?;
        self.verify()?;
        if self.head()? != head {
            return Err(SyncError::new(
                "head_changed",
                "推送前 HEAD 已变化，请重新检查",
            ));
        }
        if remote != proof.receipt.remote_head {
            return Err(SyncError::new(
                "remote_changed",
                "推送前远端基线已变化，请重新检查完整范围",
            ));
        }
        let refspec = format!("{head}:refs/heads/{}", self.project.branch);
        match run(
            self.root(),
            &["push", "--no-verify", "--", self.transport(), &refspec],
            None,
            None,
        ) {
            Ok(_) => proof.receipt.pushed = true,
            Err(error) => proof.receipt.error = Some(format!("{}: {}", error.code, error.message)),
        }
        Ok(self.persist_local_outcome(proof))
    }
    // Offline check: no recovery helper that can fetch is called by local writes.
    fn reject_unresolved_delivery(&self) -> Result<Option<LocalPushReceipt>> {
        let head = self.head()?;
        if head.is_empty() {
            return Ok(None);
        }
        let local_proof = self.successful_local_push(&head)?;
        let path = self.receipt_path(&head)?;
        if path.exists() {
            let receipt: PublishReceipt = serde_json::from_slice(&bounded_file(&path)?)
                .map_err(|_| SyncError::new("invalid_receipt", "交付回执损坏"))?;
            self.validate_receipt(&receipt, &head)?;
            if receipt.push_requested && !receipt.pushed && local_proof.is_none() {
                return Err(SyncError::new(
                    "pending_delivery",
                    "当前 HEAD 有未解决的推送交付，请先检查或重试推送",
                ));
            }
        }
        Ok(local_proof)
    }
    /// Recover local-only or already-successful current-HEAD role results offline.
    /// A validated covering range preserves success and the original requested action;
    /// unresolved remote delivery explicitly blocks writes without contacting a remote.
    pub fn recover_local_receipt(&self) -> Result<Option<PublishReceipt>> {
        self.verify()?;
        let _operation = self.operation()?;
        let covering = self.reject_unresolved_delivery()?;
        #[cfg(test)]
        if let Some(hook) = &self.history_hook {
            hook();
        }
        let head = self.head()?;
        if head.is_empty() {
            return Ok(None);
        }
        let path = self.receipt_path(&head)?;
        if !path.exists() {
            return Ok(None);
        }
        let mut receipt: PublishReceipt = serde_json::from_slice(&bounded_file(&path)?)
            .map_err(|_| SyncError::new("invalid_receipt", "交付回执损坏"))?;
        self.validate_receipt(&receipt, &head)?;
        // Match the actual role SHA explicitly: external Git can change HEAD
        // between the guard and this read despite the application's operation lock.
        if covering
            .as_ref()
            .is_some_and(|proof| proof.head == receipt.commit)
        {
            receipt.pushed = true;
            receipt.error = None;
        }
        Ok((!receipt.push_requested || receipt.pushed).then_some(receipt))
    }
    pub fn configure_role_paths(&self, paths: &BTreeMap<Role, Vec<String>>) -> Result<Project> {
        self.verify_binding()?;
        let _operation = self.operation()?;
        validate_role_paths(paths)?;
        for path in paths.values().flatten() {
            let path = safe_path(self.root(), path)?;
            if path.exists() && !path.is_dir() {
                return Err(SyncError::new("invalid_mapping", "身份路径必须是文件夹"));
            }
        }
        let current = mapping(self.root())?;
        // Allows retry after config succeeded but Store persistence failed.
        if current != self.project.role_paths && current != *paths {
            return Err(SyncError::new(
                "mapping_changed",
                "身份映射已被外部修改，请重新登记",
            ));
        }
        self.reject_unresolved_delivery()?;
        let target = safe_path(self.root(), "aijimu.workspace.json")?;
        let data = serde_json::to_vec_pretty(&Mapping {
            version: 1,
            role_paths: paths.clone(),
        })
        .map_err(|_| io_error())?;
        let mut temp = NamedTempFile::new_in(self.root()).map_err(|_| io_error())?;
        temp.write_all(&data).map_err(|_| io_error())?;
        temp.as_file().sync_all().map_err(|_| io_error())?;
        self.verify_binding()?;
        if mapping(self.root())? != current {
            return Err(SyncError::new(
                "mapping_changed",
                "保存期间身份映射已变化，请重试",
            ));
        }
        safe_path(self.root(), "aijimu.workspace.json")?;
        temp.persist(target).map_err(|_| io_error())?;
        let mut project = self.project.clone();
        project.role_paths = paths.clone();
        Ok(project)
    }
    pub fn pull(&self) -> Result<String> {
        self.verify()?;
        let _operation = self.operation()?;
        self.reject_external_filters()?;
        if !self.status()?.is_empty() {
            return Err(SyncError::new(
                "dirty_workspace",
                "工作区或暂存区有修改，请先处理后拉取",
            ));
        }
        let head = self.head()?;
        let remote = self
            .fetch()?
            .ok_or_else(|| SyncError::new("empty_remote", "远端尚无提交"))?;
        if head == remote {
            return Ok(head);
        }
        if head.is_empty() {
            return Err(SyncError::new(
                "empty_local",
                "请先使用 Git 拉取初始化空工作区",
            ));
        }
        if !self.ancestor(&head, &remote)? {
            let code = if self.ancestor(&remote, &head)? {
                "local_ahead"
            } else {
                "diverged"
            };
            return Err(SyncError::new(
                code,
                "本地包含未推送提交或历史分叉，不能 fast-forward 拉取",
            ));
        }
        self.verify()?;
        self.reject_external_filters()?;
        self.reject_incoming_filters(&remote)?;
        if self.head()? != head || !self.status()?.is_empty() {
            return Err(SyncError::new(
                "dirty_workspace",
                "拉取检查期间本地内容已变化，请重试",
            ));
        }
        run(
            self.root(),
            &[
                "merge",
                "--ff-only",
                "--no-overwrite-ignore",
                "--no-edit",
                &remote,
            ],
            None,
            None,
        )?;
        self.head()
    }
    pub fn fetch_updates(&self, previous: Option<&str>) -> Result<RemoteSnapshot> {
        self.verify()?;
        let head = self
            .fetch()?
            .ok_or_else(|| SyncError::new("empty_remote", "远端尚无提交，稍后再检查"))?;
        #[cfg(test)]
        if let Some(hook) = &self.history_hook {
            hook();
        }
        let Some(previous) = previous else {
            return Ok(RemoteSnapshot {
                head,
                commits: Vec::new(),
                rewritten: false,
            });
        };
        if !sha_valid(previous) {
            return Err(SyncError::new("invalid_sha", "监控游标提交标识无效"));
        }
        if previous == head {
            return Ok(RemoteSnapshot {
                head,
                commits: Vec::new(),
                rewritten: false,
            });
        }
        if !self.commit_exists(previous)? || !self.ancestor(previous, &head)? {
            return Ok(RemoteSnapshot {
                head,
                commits: Vec::new(),
                rewritten: true,
            });
        }
        let range = format!("{previous}..{head}");
        let shas = string(
            self.root(),
            &["rev-list", "--reverse", "--topo-order", &range],
        )?;
        let mut commits = Vec::new();
        for sha in shas.lines() {
            if !sha_valid(sha) {
                return Err(git_error());
            }
            let record = text(run(
                self.root(),
                &["show", "-s", "--format=%s%x00%an%x00%cI", sha],
                None,
                None,
            )?)?;
            let mut fields = record.trim_end_matches(['\r', '\n']).split('\0');
            commits.push(RemoteCommit {
                sha: sha.into(),
                title: fields.next().ok_or_else(git_error)?.into(),
                author: fields.next().ok_or_else(git_error)?.into(),
                committed_at: fields.next().ok_or_else(git_error)?.into(),
            });
        }
        Ok(RemoteSnapshot {
            head,
            commits,
            rewritten: false,
        })
    }
    #[cfg(test)]
    fn with_test_transport(mut self, remote: String) -> Self {
        self.transport = Some(remote);
        self
    }
    #[cfg(test)]
    fn after_local_proof(mut self, hook: impl Fn() + Send + Sync + 'static) -> Self {
        self.local_proof_hook = Some(Box::new(hook));
        self
    }
    #[cfg(test)]
    fn before_history(mut self, hook: impl Fn() + Send + Sync + 'static) -> Self {
        self.history_hook = Some(Box::new(hook));
        self
    }
    #[cfg(test)]
    fn before_cas(mut self, hook: impl Fn() + Send + Sync + 'static) -> Self {
        self.cas_hook = Some(Box::new(hook));
        self
    }
    #[cfg(test)]
    fn before_push(mut self, hook: impl Fn() + Send + Sync + 'static) -> Self {
        self.push_hook = Some(Box::new(hook));
        self
    }
}
#[cfg(test)]
#[path = "git_tests.rs"]
mod tests;
