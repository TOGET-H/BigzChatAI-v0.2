use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub type Result<T> = std::result::Result<T, SyncError>;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SyncError {
    pub code: String,
    pub message: String,
}
impl SyncError {
    pub(crate) fn new(code: &str, message: &str) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}
impl std::fmt::Display for SyncError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for SyncError {}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Product,
    Frontend,
    Backend,
    Testing,
    Skills,
}
impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Product => "product",
            Self::Frontend => "frontend",
            Self::Backend => "backend",
            Self::Testing => "testing",
            Self::Skills => "skills",
        }
    }
    pub fn all() -> [Self; 5] {
        [
            Self::Product,
            Self::Frontend,
            Self::Backend,
            Self::Testing,
            Self::Skills,
        ]
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub root: String,
    pub repository: String,
    pub remote_url: String,
    pub branch: String,
    pub role_paths: BTreeMap<Role, Vec<String>>,
    pub monitor_enabled: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddProjectInput {
    pub root: String,
    pub name: String,
    pub branch: String,
    pub create_template: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileChange {
    pub path: String,
    pub status: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangePreview {
    pub project_id: String,
    pub role: Role,
    pub head: String,
    pub fingerprint: String,
    pub files: Vec<FileChange>,
    pub outside_count: usize,
    pub diff: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishInput {
    pub project_id: String,
    pub role: Role,
    pub kind: String,
    pub module: String,
    pub summary: String,
    pub fingerprint: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PublishReceipt {
    pub project_id: String,
    pub role: Role,
    pub commit: String,
    pub parent: String,
    pub message: String,
    pub url: String,
    pub pushed: bool,
    #[serde(default = "default_push_requested")]
    pub push_requested: bool,
    pub error: Option<String>,
    #[serde(default)]
    pub persistence_warning: Option<String>,
}
fn default_push_requested() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LocalCommit {
    pub sha: String,
    pub parents: Vec<String>,
    pub title: String,
    pub body: String,
    pub author: String,
    pub committed_at: String,
    pub parsed: Option<ParsedCommit>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalHistoryPage {
    pub head: String,
    pub commits: Vec<LocalCommit>,
    pub next_offset: Option<usize>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalPushPreview {
    pub project_id: String,
    pub head: String,
    pub remote_head: Option<String>,
    pub fingerprint: String,
    pub commits: Vec<LocalCommit>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LocalPushReceipt {
    pub project_id: String,
    pub head: String,
    pub remote_head: Option<String>,
    pub url: String,
    pub commits: Vec<LocalCommit>,
    pub pushed: bool,
    pub error: Option<String>,
    pub persistence_warning: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteCommit {
    pub sha: String,
    pub title: String,
    pub author: String,
    pub committed_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSnapshot {
    pub head: String,
    pub commits: Vec<RemoteCommit>,
    pub rewritten: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ParsedCommit {
    pub kind: String,
    pub role: Role,
    pub module: String,
    pub summary: String,
}
pub fn commit_title(kind: &str, role: Role, module: &str, summary: &str) -> Result<String> {
    if !["feat", "fix", "docs", "test", "refactor", "chore"].contains(&kind) {
        return Err(SyncError::new("invalid_title", "请选择有效提交类型"));
    }
    if module.chars().chain(summary.chars()).any(char::is_control) {
        return Err(SyncError::new(
            "invalid_title",
            "模块与说明不能含换行或控制字符",
        ));
    }
    let module = module.trim();
    let summary = summary.trim();
    if module.is_empty() || summary.is_empty() || module.contains(" - ") {
        return Err(SyncError::new(
            "invalid_title",
            "请填写明确的模块与说明，模块不能含分隔符",
        ));
    }
    let title = format!("{}({}): {} - {}", kind, role.as_str(), module, summary);
    if title.chars().count() > 160 {
        return Err(SyncError::new(
            "invalid_title",
            "提交标题不能超过 160 个字符",
        ));
    }
    Ok(title)
}
pub fn parse_commit(title: &str) -> Option<ParsedCommit> {
    let (prefix, rest) = title.split_once("): ")?;
    let (kind, role) = prefix.split_once('(')?;
    let role = Role::all().into_iter().find(|r| r.as_str() == role)?;
    let (module, summary) = rest.split_once(" - ")?;
    if commit_title(kind, role, module, summary).ok()?.as_str() != title {
        return None;
    }
    Some(ParsedCommit {
        kind: kind.into(),
        role,
        module: module.into(),
        summary: summary.into(),
    })
}
pub(crate) fn default_role_paths() -> BTreeMap<Role, Vec<String>> {
    Role::all()
        .into_iter()
        .map(|r| (r, vec![r.as_str().into()]))
        .collect()
}
pub(crate) fn validate_role_paths(paths: &BTreeMap<Role, Vec<String>>) -> Result<()> {
    if paths.len() != 5
        || Role::all()
            .iter()
            .any(|r| paths.get(r).is_none_or(Vec::is_empty))
    {
        return Err(SyncError::new(
            "invalid_mapping",
            "映射必须包含五种身份及非空目录",
        ));
    }
    let mut all = Vec::<String>::new();
    for path in paths.values().flatten() {
        if path.contains(['\\', ':'])
            || path.chars().any(char::is_control)
            || path.split('/').any(|part| {
                part.is_empty()
                    || part == "."
                    || part == ".."
                    || part.eq_ignore_ascii_case(".git")
                    || part.ends_with(['.', ' '])
            })
        {
            return Err(SyncError::new(
                "invalid_mapping",
                "身份路径必须是仓库内的规范相对目录",
            ));
        }
        let folded = path.to_lowercase();
        if all.iter().any(|other| {
            other == &folded
                || other.starts_with(&format!("{folded}/"))
                || folded.starts_with(&format!("{other}/"))
        }) {
            return Err(SyncError::new("invalid_mapping", "身份目录不能互相重叠"));
        }
        all.push(folded);
    }
    Ok(())
}
pub(crate) fn github_repository(url: &str) -> Result<String> {
    let path = url
        .strip_prefix("https://github.com/")
        .or_else(|| url.strip_prefix("git@github.com:"))
        .or_else(|| url.strip_prefix("ssh://git@github.com/"));
    let invalid = || {
        SyncError::new(
            "invalid_remote",
            "仅支持不含凭据的 GitHub HTTPS 或 SSH 仓库地址",
        )
    };
    let path = path
        .ok_or_else(invalid)?
        .strip_suffix(".git")
        .unwrap_or(path.unwrap());
    let parts: Vec<_> = path.split('/').collect();
    if parts.len() != 2
        || parts.iter().any(|s| {
            s.is_empty()
                || *s == "."
                || *s == ".."
                || s.starts_with('-')
                || !s
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "_- .".contains(c) && c != ' ')
        })
    {
        return Err(invalid());
    }
    Ok(format!("{}/{}", parts[0], parts[1]))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn creates_and_parses_standard_title() {
        assert_eq!(
            commit_title("fix", Role::Frontend, " 商品页 ", " 修复筛选 ").unwrap(),
            "fix(frontend): 商品页 - 修复筛选"
        );
        assert_eq!(
            parse_commit("fix(frontend): 商品页 - 修复筛选").unwrap(),
            ParsedCommit {
                kind: "fix".into(),
                role: Role::Frontend,
                module: "商品页".into(),
                summary: "修复筛选".into()
            }
        );
        assert!(parse_commit("ordinary external commit").is_none());
    }
    #[test]
    fn rejects_invalid_titles() {
        for (kind, module, summary) in [
            ("unknown", "x", "y"),
            ("fix", " ", "y"),
            ("feat", "x", ""),
            ("fix", "x\ny", "z"),
            ("fix", "x", "y\0"),
        ] {
            assert!(commit_title(kind, Role::Frontend, module, summary).is_err());
        }
        assert!(commit_title("fix", Role::Frontend, "模块", &"字".repeat(161)).is_err());
        assert!(parse_commit("fix(admin): x - y").is_none());
    }
    #[test]
    fn accepts_only_safe_github_origins() {
        for url in [
            "https://github.com/owner/repo.git",
            "git@github.com:owner/repo",
            "ssh://git@github.com/owner/repo.git",
        ] {
            assert_eq!(github_repository(url).unwrap(), "owner/repo");
        }
        for url in [
            "https://user:secret@github.com/o/r",
            "https://example.com/o/r",
            "-oProxyCommand=evil",
            "git@github.com:o/r extra",
            "https://github.com/o/../r",
            "https://github.com/o/r?token=secret",
        ] {
            assert!(github_repository(url).is_err());
        }
    }
    #[test]
    fn validates_complete_nonoverlapping_relative_role_mapping() {
        let mut paths = default_role_paths();
        validate_role_paths(&paths).unwrap();
        paths.insert(Role::Frontend, vec!["apps/web".into()]);
        validate_role_paths(&paths).unwrap();
        for bad in [
            "../outside",
            "/absolute",
            "C:/outside",
            "apps/../web",
            ".git/objects",
            "apps\\web",
            "apps//web",
        ] {
            paths.insert(Role::Frontend, vec![bad.into()]);
            assert!(validate_role_paths(&paths).is_err(), "{bad}");
        }
        paths.insert(Role::Frontend, vec!["backend/sub".into()]);
        assert!(validate_role_paths(&paths).is_err());
    }
    #[test]
    fn serializes_ipc_with_camel_case_and_lowercase_roles() {
        let p = Project {
            id: "1".into(),
            name: "test".into(),
            root: "x".into(),
            repository: "o/r".into(),
            remote_url: "https://github.com/o/r".into(),
            branch: "main".into(),
            role_paths: default_role_paths(),
            monitor_enabled: true,
        };
        let value = serde_json::to_value(p).unwrap();
        assert_eq!(
            value["rolePaths"]["frontend"],
            serde_json::json!(["frontend"])
        );
        assert_eq!(value["monitorEnabled"], true);
        assert!(value.get("remote_url").is_none());
    }
}
