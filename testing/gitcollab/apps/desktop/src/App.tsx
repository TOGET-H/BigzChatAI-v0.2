import { useEffect, useRef, useState } from 'react';
import { ArrowDownToLine, ArrowUpRight, Bell, Check, ChevronRight, CircleAlert, FileDiff, FolderGit2, FolderPlus, GitBranch, Layers3, LogOut, RefreshCw, Send, ShieldCheck } from 'lucide-react';
import { getClient, type AppSnapshot, type ChangePreview, type CompanionClient, type CommitKind, type PublishReceipt, type LocalCommit, type LocalHistoryPage, type LocalPushPreview, type LocalPushReceipt, type Role } from './client';
const roles: { id: Role; label: string; initial: string }[] = [
  { id: 'product', label: '产品', initial: '产' },
  { id: 'frontend', label: '前端', initial: '前' },
  { id: 'backend', label: '后端', initial: '后' },
  { id: 'testing', label: '测试', initial: '测' },
  { id: 'skills', label: 'Skills 开发', initial: 'S' },
];
const kinds: CommitKind[] = ['feat', 'fix', 'docs', 'test', 'refactor', 'chore'];

function failure(error: unknown) {
  return error && typeof error === 'object' && 'message' in error
    ? String(error.message)
    : '操作未完成，请检查项目与网络后重试';
}

export default function App({ client = getClient() }: {
  client?: CompanionClient | null;
}) {
  const [state, setState] = useState<AppSnapshot | null>(null);
  const [busy, setBusy] = useState('');
  const busyRef = useRef(false);
  const snapshotVersion = useRef(-1);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const [preview, setPreview] = useState<ChangePreview | null>(null);
  const [receipt, setReceipt] = useState<PublishReceipt | null>(null);
  const [history, setHistory] = useState<LocalHistoryPage | null>(null);
  const [pushPreview, setPushPreview] = useState<LocalPushPreview | null>(null);
  const [pushReceipt, setPushReceipt] = useState<LocalPushReceipt | null>(null);
  const [mappingDraft, setMappingDraft] = useState<Record<Role, string> | null>(null);
  const context = useRef('');
  const currentProject = useRef<string | null>(null);
  const generation = useRef(0);
  const historyGeneration = useRef(0);
  const pushGeneration = useRef(0);
  const knownHead = useRef('');
  function invalidatePreviews() {
    generation.current++;
    pushGeneration.current++;
    setPreview(null);
    setPushPreview(null);
  }
  function invalidateHistory() { historyGeneration.current++; setHistory(null); }
  const [kind, setKind] = useState<CommitKind>('feat');
  const [module, setModule] = useState('');
  const [summary, setSummary] = useState('');
  const [register, setRegister] = useState(false);
  const [root, setRoot] = useState('');
  const [name, setName] = useState('');
  const [branch, setBranch] = useState('main');
  const [createTemplate, setCreateTemplate] = useState(false);
  const [messageState, setMessageState] = useState('all');
  const [messageRole, setMessageRole] = useState('all');
  const [messageProject, setMessageProject] = useState('all');
  // The Store mutation revision orders every event and command response.
  function acceptSnapshot(value: AppSnapshot) {
    if (value.revision >= snapshotVersion.current) {
      snapshotVersion.current = value.revision;
      const active = value.projects.find(p => p.project.id === value.selectedProject);
      const nextContext = JSON.stringify([value.selectedProject, value.selectedRole, active?.project.rolePaths]);
      const nextHead = JSON.stringify([active?.lastReceipt?.commit, active?.lastReceipt?.pushed, active?.lastLocalPush?.head, active?.lastLocalPush?.pushed]);
      if (nextContext !== context.current) {
        if (value.selectedProject !== currentProject.current) {
          setReceipt(null); setPushReceipt(null); setModule(''); setSummary(''); setNotice(''); setError('');
        }
        currentProject.current = value.selectedProject;
        context.current = nextContext;
        invalidatePreviews(); invalidateHistory(); setMappingDraft(null);
      }
      if (nextHead && knownHead.current && nextHead !== knownHead.current) { invalidatePreviews(); invalidateHistory(); }
      knownHead.current = nextHead;
      setState(value);
    }
  }
  async function updateSnapshot(request: () => Promise<AppSnapshot>, isActive: () => boolean = () => true) {
    const value = await request();
    if (isActive())
      acceptSnapshot(value);
  }
  useEffect(() => {
    if (!client)
      return;
    snapshotVersion.current = -1;
    let alive = true, dispose: (() => void) | undefined;
    setBusy('连接桌面服务');
    busyRef.current = true;
    void (async () => {
      try {
        const stop = await client.subscribe(value => {
          if (alive)
            acceptSnapshot(value);
        });
        if (!alive) {
          stop();
          return;
        }
        dispose = stop;
        await updateSnapshot(() => client.snapshot(), () => alive);
      }
      catch (e) {
        if (alive)
          setError(failure(e));
      }
      finally {
        if (alive) {
          setBusy('');
          busyRef.current = false;
        }
      }
    })();
    return () => {
      alive = false;
      dispose?.();
    };
  }, [client]);
  const selected = state?.projects.find(p => p.project.id === state.selectedProject);
  const project = selected?.project;
  const role = state?.selectedRole ?? 'product';
  const connected = Boolean(client && state);
  const disabled = !connected || Boolean(busy);
  const title = `${kind}(${role}): ${module.trim()} - ${summary.trim()}`;
  const titleCount = Array.from(title).length;
  const validTitle = Boolean(module.trim() && summary.trim() && !/[\u0000-\u001f\u007f-\u009f]/u.test(module + summary) && !module.trim().includes(' - ') && titleCount <= 160);
  const currentPreview = preview?.projectId === project?.id && preview?.role === role ? preview : null;
  const validReceipt = (value: PublishReceipt | null | undefined) => value?.projectId === project?.id ? value : null;
  const pending = validReceipt(selected?.pending);
  const localReceipt = validReceipt(receipt);
  const persistedReceipt = validReceipt(selected?.lastReceipt);
  const acknowledged = Boolean(localReceipt && state?.messages.some(message =>
    message.own && !message.rewritten && message.projectId === project?.id &&
    message.repository.toLowerCase() === project?.repository.toLowerCase() &&
    message.branch === project?.branch && message.sha === localReceipt.commit
  ));
  // A per-project durable success survives message deduplication across clones.
  const knownReceipt = localReceipt && acknowledged ? { ...localReceipt, pushed: true, error: null } : localReceipt;
  const sameSuccess = [knownReceipt, persistedReceipt].find(r => r?.pushed && r.commit === (pending?.commit ?? knownReceipt?.commit ?? persistedReceipt?.commit));
  const candidateReceipt = sameSuccess ?? (pending && pending.commit !== knownReceipt?.commit ? pending : knownReceipt ?? persistedReceipt ?? pending);
  const savedRange = selected?.lastLocalPush && selected.lastLocalPush.projectId === project?.id ? selected.lastLocalPush : null;
  const returnedRange = pushReceipt?.projectId === project?.id ? pushReceipt : null;
  const matchingSavedRange = returnedRange && savedRange?.head === returnedRange.head ? savedRange : null;
  // Same-project/HEAD success is monotonic; fresh metadata and warnings still
  // belong to the current command (including an emit failure after Store save).
  const visibleRange = returnedRange && matchingSavedRange ? {
    ...returnedRange,
    pushed: returnedRange.pushed || matchingSavedRange.pushed,
    error: returnedRange.pushed || matchingSavedRange.pushed ? null : returnedRange.error,
    persistenceWarning: returnedRange.persistenceWarning ?? (returnedRange.pushed ? null : matchingSavedRange.persistenceWarning),
  } : returnedRange ?? savedRange;
  // A validated successful range is positive proof for exactly its covered SHAs,
  // even when saving the upgraded role receipt failed. Keep the range warning.
  const rangeCoversRole = Boolean(candidateReceipt && [visibleRange, matchingSavedRange].some(range => range?.pushed && range.commits.some(commit => commit.sha === candidateReceipt.commit)));
  const visibleReceipt = candidateReceipt && rangeCoversRole ? { ...candidateReceipt, pushed: true, error: null } : candidateReceipt;
  const unresolvedRole = Boolean(pending && !pending.pushed && !(visibleReceipt?.pushed && visibleReceipt.commit === pending.commit));
  const unresolvedRange = Boolean(visibleRange && !visibleRange.pushed);
  const deliveryNotice = visibleRange?.pushed && notice === '已有提交已保留，范围推送未完成' ? '完整范围已推送'
    : visibleReceipt?.pushed && notice === '提交已保留，推送未完成' ? '交付已推送' : notice;
  const currentPushPreview = pushPreview?.projectId === project?.id ? pushPreview : null;
  const messages = (state?.messages ?? []).filter(message =>
    (messageState !== 'unread' || !message.read) &&
    (messageRole === 'all' || message.parsed?.role === messageRole) &&
    (messageProject === 'all' || message.projectId === messageProject)
  );
  async function run(label: string, action: () => Promise<void>) {
    if (!client || busyRef.current)
      return;
    busyRef.current = true;
    setBusy(label);
    setError('');
    setNotice('');
    const requestContext = context.current;
    try {
      await action();
    }
    catch (e) {
      if (requestContext === context.current) setError(failure(e));
    }
    finally {
      busyRef.current = false;
      setBusy('');
    }
  }
  async function refresh() {
    if (client) await updateSnapshot(() => client.snapshot());
  }
  function select(projectId: string | null, nextRole: Role) {
    void run('切换工作身份', async () => {
      invalidatePreviews(); invalidateHistory(); setMappingDraft(null);
      await updateSnapshot(() => client!.select(projectId, nextRole));
    });
  }
  function deliver(retry = false, local = false) {
    if (!project)
      return;
    void run(retry ? '重试推送' : local ? '仅提交到本地' : '提交并推送', async () => {
      try {
        const result = retry ? await client!.retryPush(project.id) : await (local ? client!.commitLocal : client!.publish)({
          projectId: project.id,
          role,
          kind,
          module: module.trim(),
          summary: summary.trim(),
          fingerprint: currentPreview!.fingerprint,
        });
        if (result.projectId === currentProject.current) {
          setReceipt(result); invalidatePreviews(); invalidateHistory();
          setNotice(result.pushed ? '交付已推送' : result.pushRequested ? '提交已保留，推送未完成' : '本地提交完成');
        }
        await refresh();
      }
      catch (e) {
        setPreview(null);
        throw e;
      }
    });
  }

  function readHistory(more = false) {
    if (!project) return;
    const id = project.id;
    const request = ++historyGeneration.current;
    const page = more ? history : null;
    void run('读取本地历史', async () => {
      try {
        const result = await client!.localHistory(id, page?.nextOffset ?? 0, page?.head ?? null);
        if (request !== historyGeneration.current || id !== currentProject.current) return;
        if (page && result.head !== page.head) { throw { message: 'HEAD 已变化，请重新读取第一页历史' }; }
        if (currentPreview && currentPreview.head !== result.head || currentPushPreview && currentPushPreview.head !== result.head) invalidatePreviews();
        setHistory({ ...result, commits: page ? [...page.commits, ...result.commits] : result.commits });
      } catch (e) {
        if (request !== historyGeneration.current || id !== currentProject.current) return;
        invalidateHistory(); throw e;
      }
    });
  }
  function fillHistory(commit: LocalCommit) {
    invalidatePreviews();
    if (commit.parsed && kinds.includes(commit.parsed.kind as CommitKind)) {
      setKind(commit.parsed.kind as CommitKind); setModule(commit.parsed.module); setSummary(commit.parsed.summary);
      if (commit.parsed.role !== role) select(project!.id, commit.parsed.role);
    } else { setModule(''); setSummary(commit.title); }
  }
  function beginMapping() {
    if (!project) return;
    invalidatePreviews();
    setMappingDraft(Object.fromEntries(roles.map(r => [r.id, project.rolePaths[r.id].join('\n')])) as Record<Role, string>);
  }

  return (
    <div className="app-shell">
      <header className="topbar">
        <div className="brand-mark"><Layers3 size={21} /></div>
        <div className="brand">
          <strong>Git <span>协作台</span></strong>
          <small>让每一份改动，清楚地交付。</small>
        </div>
        <span className={`connection ${connected ? 'native' : ''}`}>
          <i />{client ? (state ? '桌面服务已连接' : '连接中') : '浏览器预览'}
        </span>
      </header>
      {!client && (
        <div className="boundary" role="status">
          <CircleAlert size={16} />
          <span>本地 Git 操作需要桌面端。当前为浏览器预览，项目与更新数据仅在桌面端读取。</span>
        </div>
      )}
      <main className="workspace">
        <aside className="sidebar">
          <div className="section-heading">
            <span>工作空间</span><span className="quiet">PROJECTS</span>
          </div>
          <div className="project-list">
            {state?.projects.map(item => (
              <button
                key={item.project.id}
                className={`project-card ${project?.id === item.project.id ? 'active' : ''}`}
                disabled={disabled}
                onClick={() => select(item.project.id, role)}
              >
                <FolderGit2 size={19} />
                <span><strong>{item.project.name}</strong><small>{item.project.root}</small></span>
                <ChevronRight size={14} />
              </button>
            ))}
            {!state?.projects.length && (
              <div className="empty-project">
                <FolderGit2 size={27} /><strong>从一个项目开始</strong>
                <p>登记已有 Git 仓库，将工作目录与交付身份连接起来。</p>
              </div>
            )}
          </div>
          <button className="add-project" disabled={disabled} onClick={() => setRegister(true)}>
            <FolderPlus size={16} />登记项目
          </button>
          <div className="identity-heading"><span>当前工作身份</span><small>五种固定身份</small></div>
          <div className="roles">
            {roles.map(item => (
              <button
                aria-label={`${item.label} ${item.id}`}
                key={item.id}
                className={`role-button ${role === item.id ? 'active' : ''}`}
                disabled={disabled}
                onClick={() => select(project?.id ?? null, item.id)}
              >
                <span className="avatar">{item.initial}</span>
                <span><strong>{item.label}</strong><small>{item.id}</small></span>
                {role === item.id && <Check size={14} />}
              </button>
            ))}
          </div>
          <div className="scope-note">
            <ShieldCheck size={16} /><p>身份决定交付目录范围。<br />拉取会更新同一分支的全部目录。</p>
          </div>
          <div className="lifecycle">
            <p>关闭窗口后进入托盘，监控继续。<br />退出应用将停止监控。</p>
            <button disabled={!client || Boolean(busy)} onClick={() => void run('退出应用', () => client!.exit())}>
              <LogOut size={14} />退出应用
            </button>
          </div>
        </aside>
        <section className="delivery">
          <div className="delivery-heading">
            <div><div className="eyebrow">WORK & DELIVERY</div><h1>检查改动，完成交付</h1></div>
            <span className="role-pill">{roles.find(item => item.id === role)?.label}</span>
          </div>
          <div className="repository-bar">
            <FolderGit2 size={16} /><strong>{project?.repository ?? '尚未选择项目'}</strong>
            <span className="branch"><GitBranch size={13} /><span>{project?.branch ?? '—'}</span></span>
          </div>
          {project && (
            <details className="mapping">
              <summary>身份目录映射 <span>{project.rolePaths[role].join(' · ')}</span></summary>
              <dl>{roles.map(item => (
                <div key={item.id}><dt>{item.label}</dt><dd>{project.rolePaths[item.id].join(' · ')}</dd></div>
              ))}</dl>
              {mappingDraft ? <div className="mapping-editor">
                <p>每行一个仓库内相对目录；保存后生效。目录不能跨身份重叠。</p>
                {roles.map(item => <div className="mapping-field" key={item.id}>
                  <label>{item.label}目录<textarea disabled={disabled} value={mappingDraft[item.id]} onChange={event => {
                    invalidatePreviews(); setMappingDraft({ ...mappingDraft, [item.id]: event.target.value });
                  }} /></label>
                  <button disabled={disabled} onClick={() => void run('选择身份目录', async () => {
                    const request = generation.current, id = project.id;
                    const path = await client!.pickRoleFolder(id);
                    if (path && request === generation.current && id === currentProject.current) setMappingDraft(previous => previous ? { ...previous, [item.id]: [...new Set([...previous[item.id].split('\n').filter(Boolean), path])].join('\n') } : null);
                  })}>选择{item.label}目录</button>
                </div>)}
                <div className="action-row">
                  <button disabled={disabled} onClick={() => void run('保存身份目录', async () => {
                    invalidatePreviews(); const request = generation.current, id = project.id;
                    const paths = Object.fromEntries(roles.map(r => [r.id, mappingDraft[r.id].split('\n').map(p => p.trim()).filter(Boolean)])) as Record<Role, string[]>;
                    const result = await client!.setRolePaths(id, paths);
                    if (request === generation.current && id === currentProject.current) { acceptSnapshot(result); setMappingDraft(null); setNotice('身份目录已保存'); }
                  })}>保存身份目录</button>
                  <button disabled={disabled} onClick={() => { invalidatePreviews(); setMappingDraft(null); }}>取消目录编辑</button>
                  <button disabled={disabled} onClick={() => { invalidatePreviews(); setMappingDraft(Object.fromEntries(roles.map(r => [r.id, r.id])) as Record<Role, string>); }}>恢复默认草稿</button>
                </div>
              </div> : <button disabled={disabled || unresolvedRole || unresolvedRange} onClick={beginMapping}>编辑身份目录</button>}
              <p>当前身份范围：{project.rolePaths[role].join(' · ')}。保存沿用兼容文件 aijimu.workspace.json。</p>
              <button disabled={disabled || unresolvedRole || unresolvedRange} onClick={() => void run('移除项目登记', async () => {
                await updateSnapshot(() => client!.removeProject(project.id));
                setPreview(null);
              })}>移除项目登记（保留文件与消息）</button>
              {(unresolvedRole || unresolvedRange) && <p>请先完成待重试交付，再移除登记。</p>}
            </details>
          )}
          {busy && <div className="operation" role="status"><RefreshCw size={14} className="spin" />{busy}…</div>}
          {error && <div className="feedback error" role="alert"><CircleAlert size={15} />{error}</div>}
          {deliveryNotice && <div className="feedback success" role="status"><Check size={15} />{deliveryNotice}</div>}
          {selected?.lastError && <div className="feedback error"><CircleAlert size={15} />{selected.lastError}</div>}
          <div className="change-header">
            <h2><FileDiff size={17} />当前身份的变更</h2>
            <button disabled={disabled || !project} onClick={() => void run('检查变更', async () => {
              invalidatePreviews(); const request = generation.current;
              const result = await client!.preview(project!.id, role);
              if (request === generation.current && result.projectId === currentProject.current && result.role === role) setPreview(result);
            })}><RefreshCw size={14} />检查变更</button>
          </div>
          {currentPreview ? (
            <div className="preview">
              <div className="preview-summary">
                <strong>{currentPreview.files.length} 个候选文件</strong>
                <span>其他身份改动 {currentPreview.outsideCount} 项</span>
              </div>
              <ul className="file-list">{currentPreview.files.map(file => (
                <li key={file.path}><span className="file-status">{file.status}</span><code>{file.path}</code></li>
              ))}</ul>
              <details className="diff" open>
                <summary>变更 Diff <small>HEAD {currentPreview.head ? currentPreview.head.slice(0, 12) : '空仓库'}</small></summary>
                <pre>{currentPreview.diff || '没有可展示的文本 Diff'}</pre>
              </details>
              {!currentPreview.files.length && <p className="quiet">当前身份没有可交付变更。</p>}
            </div>
          ) : (
            <div className="change-empty">
              <FileDiff size={29} /><strong>{project ? '检查一次，确认交付范围' : '准备好你的协作项目'}</strong>
              <p>{project ? '查看文件清单与 Diff 后，再提交当前身份的改动。' : '选择或登记项目后，这里将展示真实的文件变更。'}</p>
            </div>
          )}
          <section className="local-history">
            <div className="change-header"><h2>本地提交历史</h2><button disabled={disabled || !project} onClick={() => readHistory()}>读取本地历史</button></div>
            <p className="quiet">仅读取本地，每页 50 条。填充只复制新提交草稿，原提交 SHA 保持不变。</p>
            {history && <><p className="history-head">历史 HEAD <code>{history.head || '空仓库'}</code></p>
              <ol className="history-list">{history.commits.map(commit => <li key={commit.sha}>
                <h3>{commit.title}</h3><code>{commit.sha}</code><p className="commit-meta">{commit.author} · {commit.committedAt}</p>
                <p className="commit-meta">Parents: {commit.parents.join(' · ') || '无'}</p>
                {commit.body && <pre>{commit.body}</pre>}
                <button disabled={disabled || Boolean(mappingDraft)} onClick={() => fillHistory(commit)}>填充提交说明</button>
              </li>)}</ol>{!history.commits.length && <p>本地历史为空</p>}
              {history.nextOffset !== null && <button disabled={disabled} onClick={() => readHistory(true)}>加载更多历史</button>}
            </>}
          </section>
          <section className="commit-form">
            <div className="form-title"><h2>提交说明</h2><small>统一规范 · 清晰可追溯</small></div>
            <div className="form-row">
              <label>提交类型
                <select disabled={Boolean(busy)} value={kind} onChange={event => setKind(event.target.value as CommitKind)}>
                  {kinds.map(item => <option key={item}>{item}</option>)}
                </select>
              </label>
              <label className="grow">页面或模块
                <input disabled={Boolean(busy)} value={module} onChange={event => setModule(event.target.value)} placeholder="例如：商品页" />
              </label>
            </div>
            <label>变更说明
              <textarea disabled={Boolean(busy)} value={summary} onChange={event => setSummary(event.target.value)} placeholder="描述这次改动解决了什么问题" rows={2} />
            </label>
            <div className="title-draft">
              <span>COMMIT PREVIEW <small>{titleCount}/160</small></span>
              <code>{module || summary ? title : '填写模块和说明，生成标准提交标题'}</code>
            </div>
            {(module || summary) && !validTitle && (
              <p className="validation">模块与说明不能为空或含控制字符；模块不能含「 - 」，完整标题最多 160 个字符。</p>
            )}
            <div className="commit-actions">
            <button disabled={disabled || !project || !currentPreview?.files.length || !validTitle || unresolvedRole || unresolvedRange || Boolean(mappingDraft)} onClick={() => deliver(false, true)}>仅提交到本地</button>
            <button
              className="primary publish"
              disabled={disabled || !project || !currentPreview?.files.length || !validTitle || unresolvedRole || unresolvedRange || Boolean(mappingDraft)}
              onClick={() => deliver()}
            ><Send size={15} />提交并推送</button></div>
            <p className="commit-help">仅交付当前身份目录内的变更。仅提交到本地不联网；提交并推送会核对远端基线。</p>
          </section>
          {visibleReceipt && (
            <div className={`receipt ${visibleReceipt.pushed ? '' : visibleReceipt.pushRequested ? 'pending' : 'local-only'}`}>
              <strong>{visibleReceipt.pushed ? '交付已推送' : visibleReceipt.pushRequested ? '待重试交付' : '本地已提交，未推送'}</strong>
              <code>{visibleReceipt.commit.slice(0, 12)}</code><p>{visibleReceipt.message}</p>
              {visibleReceipt.error && <p className="error-text">{visibleReceipt.error}</p>}
              {visibleReceipt.persistenceWarning && <p className="error-text" role="alert">{visibleReceipt.persistenceWarning}</p>}
              {!visibleReceipt.pushed && visibleReceipt.pushRequested && <button disabled={disabled} onClick={() => deliver(true)}>重试推送</button>}
              <button disabled={disabled || !project || !visibleReceipt.pushed} onClick={() => void run('打开提交', () => client!.openCommit(project!.repository, visibleReceipt.commit))}>
                打开提交<ArrowUpRight size={13} />
              </button>
            </div>
          )}
          <section className="push-range">
            <div className="change-header"><h2>已有本地提交</h2><button disabled={disabled || !project || Boolean(mappingDraft)} onClick={() => void run('检查待推送提交', async () => {
              setPushPreview(null); const request = ++pushGeneration.current, id = project!.id;
              const result = await client!.previewLocalPush(id);
              if (request === pushGeneration.current && result.projectId === id && id === currentProject.current) {
                if (history && history.head !== result.head) invalidateHistory();
                if (currentPreview && currentPreview.head !== result.head) { generation.current++; setPreview(null); }
                setPushPreview(result);
              }
            })}>检查待推送提交</button></div>
            <p className="quiet">检查会联系远端。Git 推送完整祖先链范围，以下全部提交将一起推送。</p>
            {currentPushPreview && <div className="range-details">
              <dl><div><dt>目标库 / 分支</dt><dd>{project!.repository} / {project!.branch}</dd></div>
                <div><dt>远端基线</dt><dd><code>{currentPushPreview.remoteHead ?? '空远端 / 新分支'}</code></dd></div>
                <div><dt>目标 HEAD</dt><dd><code>{currentPushPreview.head}</code></dd></div></dl>
              <strong>完整范围 · {currentPushPreview.commits.length} 条提交</strong>
              <ol className="range-list">{currentPushPreview.commits.map(commit => <li key={commit.sha}>
                <h3>{commit.title}</h3><code>{commit.sha}</code><p>{commit.parsed?.role ?? '身份未标注'} · {commit.author} · {commit.committedAt}</p>
                {commit.body && <pre>{commit.body}</pre>}
              </li>)}</ol>{!currentPushPreview.commits.length && <p>没有待推送提交</p>}
            </div>}
            <button className="primary" disabled={disabled || !project || !currentPushPreview?.commits.length || Boolean(mappingDraft)} onClick={() => void run('推送这些提交', async () => {
              const id = project!.id;
              try {
                const result = await client!.pushLocalCommits(id, currentPushPreview!.fingerprint);
                if (result.projectId === id && id === currentProject.current) {
                  setPushReceipt(result); invalidatePreviews(); setNotice(result.pushed ? '完整范围已推送' : '已有提交已保留，范围推送未完成');
                }
                await refresh();
              } catch (e) { setPushPreview(null); throw e; }
            })}>推送这些提交</button>
            {visibleRange && <div className={`receipt ${visibleRange.pushed ? '' : 'pending'}`}>
              <strong>{visibleRange.pushed ? '完整范围已推送' : '范围推送未完成，请重新检查'}</strong><code>{visibleRange.head.slice(0, 12)}</code>
              <p>{visibleRange.commits.length} 条提交 · 原 SHA 保留</p>{visibleRange.error && <p className="error-text">{visibleRange.error}</p>}
              {visibleRange.persistenceWarning && <p className="error-text" role="alert">{visibleRange.persistenceWarning}</p>}
            </div>}
          </section>
          <div className="sync-row">
            <button disabled={disabled || !project} onClick={() => void run('拉取更新', async () => {
              const head = await client!.pull(project!.id);
              invalidatePreviews(); invalidateHistory();
              setNotice(`拉取完成 · HEAD ${head.slice(0, 12)}`);
              await refresh();
            })}><ArrowDownToLine size={14} />拉取更新</button>
            <button disabled={disabled || !project} onClick={() => void run('检查远端', async () => {
              await updateSnapshot(() => client!.checkUpdates(project!.id));
            })}><RefreshCw size={14} />立即检查远端</button>
            <label className="switch">
              <input type="checkbox" checked={project?.monitorEnabled ?? false} disabled={disabled || !project} onChange={event => void run('设置监控', async () => {
                await updateSnapshot(() => client!.setMonitor(project!.id, event.target.checked));
              })} /><span>后台监控</span>
            </label>
          </div>
          <p className="sync-help">拉取需要干净工作区，且只接受快进更新。监控每 120 秒检查一次。</p>
        </section>
        <aside className="messages">
          <div className="messages-heading">
            <div><div className="eyebrow">TEAM UPDATES</div><h2><Bell size={17} />更新消息</h2></div>
            {state && <span className="unread-count">{state.messages.filter(message => !message.read).length}</span>}
          </div>
          <p className="messages-intro">跨项目的交付与远端变化，保存在这里。</p>
          <div className="message-filters">
            <select aria-label="消息状态" value={messageState} onChange={event => setMessageState(event.target.value)}>
              <option value="all">全部消息</option><option value="unread">仅未读</option>
            </select>
            <select aria-label="消息身份" value={messageRole} onChange={event => setMessageRole(event.target.value)}>
              <option value="all">全部身份</option>{roles.map(item => <option key={item.id} value={item.id}>{item.label}</option>)}
            </select>
            <select aria-label="消息项目" value={messageProject} onChange={event => setMessageProject(event.target.value)}>
              <option value="all">全部项目</option>{state?.projects.map(item => <option key={item.project.id} value={item.project.id}>{item.project.name}</option>)}
            </select>
          </div>
          <button className="read-all" disabled={disabled || !state?.messages.some(message => !message.read)} onClick={() => void run('标记全部已读', async () => {
            await updateSnapshot(() => client!.markRead(state!.messages.filter(message => !message.read).map(message => message.id)));
          })}><Check size={13} />全部已读</button>
          <div className="message-list">
            {messages.map(message => (
              <article className={`message-card ${message.read ? 'read' : ''}`} key={message.id}>
                <div className="message-meta">
                  <span>{message.parsed ? roles.find(item => item.id === message.parsed!.role)?.label : '身份未标注'}</span>
                  <small>{message.read ? '已读' : '未读'}</small>
                </div>
                {message.rewritten && <strong className="rewritten">远端历史已变化</strong>}
                <h3>{message.title}</h3><p className="message-origin">{message.repository} · {message.branch}</p>
                <p className="commit-meta">{message.author && message.committedAt ? `${message.author} · ${message.committedAt}` : '元数据待补全'}</p>
                <div className="message-actions">
                  <code>{message.sha.slice(0, 8)}</code>
                  {!message.read && <button disabled={disabled} onClick={() => void run('标记已读', async () => {
                    await updateSnapshot(() => client!.markRead([message.id]));
                  })}>标记已读</button>}
                  <button aria-label={`打开提交 ${message.sha.slice(0, 8)}`} disabled={disabled} onClick={() => void run('打开提交', () => client!.openCommit(message.repository, message.sha))}>
                    <ArrowUpRight size={15} />
                  </button>
                </div>
              </article>
            ))}
            {!messages.length && (
              <div className="messages-empty">
                <div><Bell size={26} /></div><strong>{state?.messages.length ? '当前筛选下没有消息' : '还没有更新消息'}</strong>
                <p>首次检查建立基线。<br />后续检测到新提交时，会显示在这里。</p>
              </div>
            )}
          </div>
          <div className="notification-settings">
            <label className="switch">
              <input type="checkbox" disabled={disabled} checked={state?.systemNotifications ?? false} onChange={event => void run('设置系统通知', async () => {
                await updateSnapshot(() => client!.setNotifications(event.target.checked));
              })} /><span>系统通知</span>
            </label>
            <p>默认关闭。开启时请求权限；实际显示受系统设置影响，应用内消息会保留。</p>
          </div>
        </aside>
      </main>
      {register && (
        <div className="modal-backdrop">
          <section className="registration" role="dialog" aria-modal="true" aria-labelledby="registration-title">
            <div className="form-title">
              <h2 id="registration-title">登记协作项目</h2><button disabled={Boolean(busy)} onClick={() => setRegister(false)}>取消</button>
            </div>
            <p>选择已有 GitHub origin 的普通本地 Git 仓库。</p>
            <label>仓库根目录
              <div className="folder-input">
                <input value={root} disabled={Boolean(busy)} onChange={event => setRoot(event.target.value)} placeholder="C:\work\project" />
                <button disabled={disabled} onClick={() => void run('选择文件夹', async () => {
                  const path = await client!.pickFolder();
                  if (path) setRoot(path);
                })}>选择文件夹</button>
              </div>
            </label>
            <label>项目名称<input value={name} disabled={Boolean(busy)} onChange={event => setName(event.target.value)} placeholder="便于辨认的名称" /></label>
            <label>目标分支<input value={branch} disabled={Boolean(busy)} onChange={event => setBranch(event.target.value)} /></label>
            <label className="template-option">
              <input type="checkbox" checked={createTemplate} disabled={Boolean(busy)} onChange={event => setCreateTemplate(event.target.checked)} />创建缺失的五种身份目录与规范文件
            </label>
            <p className="quiet">保留已有文件；现有映射文件必须符合本应用格式。</p>
            {error && <p className="error-text" role="alert">{error}</p>}
            <button className="primary" disabled={disabled || !root.trim() || !name.trim() || !branch.trim()} onClick={() => void run('登记项目', async () => {
              await updateSnapshot(() => client!.addProject({ root: root.trim(), name: name.trim(), branch: branch.trim(), createTemplate }));
              setRegister(false);
              setRoot('');
              setName('');
              setCreateTemplate(false);
              setNotice('项目已登记');
            })}>保存项目</button>
          </section>
        </div>
      )}
    </div>
  );
}
