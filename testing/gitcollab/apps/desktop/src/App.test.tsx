import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { vi } from 'vitest';
import App from './App';
import { getClient, type AppSnapshot, type Project, type PublishReceipt, type UpdateMessage, type ChangePreview, type CompanionClient } from './client';
const project: Project = {
  id: 'p1', name: '协作工程', root: 'C:/work/team', repository: 'team/project',
  remoteUrl: 'https://github.com/team/project', branch: 'main', monitorEnabled: true,
  rolePaths: { product: ['product'], frontend: ['apps/web'], backend: ['backend'], testing: ['testing'], skills: ['skills'] },
};
const pending: PublishReceipt = {
  persistenceWarning: null, projectId: 'p1', role: 'frontend', commit: 'a'.repeat(40), parent: 'b'.repeat(40),
  message: 'fix(frontend): 商品页 - 修复筛选', url: 'https://github.com/team/project/commit/' + 'a'.repeat(40),
  pushed: false, pushRequested: true, error: '网络失败，请重试推送',
};
const message: UpdateMessage = {
  own: false, id: 'm1', projectId: 'p1', repository: 'team/project', branch: 'main', sha: 'c'.repeat(40),
  title: 'ordinary external commit', author: 'A Developer', committedAt: '2026-10-07T10:00:00Z',
  url: 'https://github.com/team/project/commit/' + 'c'.repeat(40), parsed: null, read: false, rewritten: false,
};
const historyCommit = { sha: 'e'.repeat(40), parents: ['b'.repeat(40)], title: 'fix(frontend): 商品页 - 修复筛选', body: '原始正文', author: 'Real Author', committedAt: '2026-10-07T10:00:00Z', parsed: { kind: 'fix', role: 'frontend' as const, module: '商品页', summary: '修复筛选' } };
const pushPreview = { projectId: 'p1', head: 'e'.repeat(40), remoteHead: 'b'.repeat(40), fingerprint: 'range-fp', commits: [historyCommit, { ...historyCommit, sha: 'f'.repeat(40), title: 'ordinary title', parsed: null }] };
function fixture(overrides: Partial<AppSnapshot> = {}) {
  let state: AppSnapshot = {
    revision: 1, projects: [{ project, cursor: null, lastError: null, pending: null, lastReceipt: null, lastLocalPush: null }], messages: [message],
    selectedProject: 'p1', selectedRole: 'frontend', systemNotifications: false, ...overrides,
  };
  const client: CompanionClient = {
    snapshot: async () => state,
    subscribe: async () => () => {},
    pickFolder: async () => 'C:/new/project',
    addProject: async () => state,
    removeProject: async () => state,
    select: async (projectId, role) => {
      state = { ...state, revision: state.revision + 1, selectedProject: projectId, selectedRole: role };
      return state;
    },
    preview: async () => ({
      projectId: 'p1', role: 'frontend', head: 'b'.repeat(40), fingerprint: 'fp1',
      files: [{ path: 'apps/web/product.tsx', status: 'M' }], outsideCount: 2, diff: '-old filter\n+fixed filter',
    }),
    localHistory: async () => ({ head: 'b'.repeat(40), commits: [historyCommit], nextOffset: null }),
    commitLocal: async () => ({ ...pending, pushRequested: false, error: null }),
    previewLocalPush: async () => pushPreview,
    pushLocalCommits: async () => ({ ...pushPreview, url: pending.url, pushed: true, error: null, persistenceWarning: null }),
    setRolePaths: async (id, paths) => ({ ...state, revision: state.revision + 1, projects: state.projects.map(p => p.project.id === id ? { ...p, project: { ...p.project, rolePaths: paths } } : p) }),
    pickRoleFolder: async () => 'packages/ui',
    publish: async () => ({ ...pending, pushed: true, error: null }),
    retryPush: async () => ({ ...pending, pushed: true, error: null }),
    pull: async () => 'b'.repeat(40),
    checkUpdates: async () => state,
    setMonitor: async () => state,
    setNotifications: async () => state,
    markRead: async (ids: string[]) => {
      state = { ...state, revision: state.revision + 1, messages: state.messages.map(item => ids.includes(item.id) ? { ...item, read: true } : item) };
      return state;
    },
    openCommit: async () => {},
    exit: async () => {},
  };
  return { client, state };
}
async function mounted(overrides: Partial<AppSnapshot> = {}) {
  const result = fixture(overrides);
  render(<App client={result.client} />);
  await screen.findByText('team/project');
  return result;
}
function draft() {
  fireEvent.change(screen.getByLabelText('页面或模块'), { target: { value: '商品页' } });
  fireEvent.change(screen.getByLabelText('变更说明'), { target: { value: '修复筛选' } });
}

it('browser has no native client and disables local operations without fabricated project data', () => {
  expect(getClient()).toBeNull();
  render(<App client={null} />);
  expect(screen.getByText(/需要桌面端/)).toBeVisible();
  expect(screen.getByRole('button', { name: '登记项目' })).toBeDisabled();
  expect(screen.getByRole('button', { name: '检查变更' })).toBeDisabled();
  expect(screen.queryByText('team/project')).not.toBeInTheDocument();
});

it('publish requires real preview, nonempty valid title and change files', async () => {
  await mounted();
  draft();
  expect(screen.getByRole('button', { name: '提交并推送' })).toBeDisabled();
  fireEvent.click(screen.getByRole('button', { name: '检查变更' }));
  await screen.findByText('apps/web/product.tsx');
  expect(screen.getByRole('button', { name: '提交并推送' })).toBeEnabled();
  fireEvent.change(screen.getByLabelText('页面或模块'), { target: { value: 'bad - module' } });
  expect(screen.getByRole('button', { name: '提交并推送' })).toBeDisabled();
});
it('shows repository branch outside changes and exact diff', async () => {
  await mounted();
  expect(screen.getByText('main')).toBeVisible();
  fireEvent.click(screen.getByRole('button', { name: '检查变更' }));
  await screen.findByText('apps/web/product.tsx');
  expect(screen.getByText(/其他身份.*2/)).toBeVisible();
  expect(screen.getByText(/-old filter/)).toHaveTextContent('+fixed filter');
});
it('switching identity invalidates preview and recomputes controlled title', async () => {
  await mounted();
  draft();
  fireEvent.click(screen.getByRole('button', { name: '检查变更' }));
  await screen.findByText('apps/web/product.tsx');
  fireEvent.click(screen.getByRole('button', { name: '后端 backend' }));
  await screen.findByText('feat(backend): 商品页 - 修复筛选');
  expect(screen.getByRole('button', { name: '提交并推送' })).toBeDisabled();
  fireEvent.change(screen.getByLabelText('提交类型'), { target: { value: 'fix' } });
  expect(screen.getByText('fix(backend): 商品页 - 修复筛选')).toBeVisible();
});
it('failed durable receipt offers retry and preserves its real SHA', async () => {
  await mounted({ projects: [{ project, cursor: null, lastError: null, pending, lastReceipt: null, lastLocalPush: null }] });
  expect(screen.getByText('aaaaaaaaaaaa')).toBeVisible();
  fireEvent.click(screen.getByRole('button', { name: '重试推送' }));
  await screen.findAllByText('交付已推送');
});
it('unparsed messages disclose unknown identity and marking read persists view', async () => {
  await mounted();
  expect(screen.getByText('身份未标注')).toBeVisible();
  fireEvent.click(screen.getByRole('button', { name: '标记已读' }));
  await screen.findByText('已读');
  fireEvent.change(screen.getByLabelText('消息状态'), { target: { value: 'unread' } });
  expect(screen.queryByText('ordinary external commit')).not.toBeInTheDocument();
});
it('rewritten history and missing metadata are honest', async () => {
  await mounted({ messages: [{ ...message, rewritten: true, author: '', committedAt: '' }] });
  expect(screen.getByText('远端历史已变化')).toBeVisible();
  expect(screen.getByText('元数据待补全')).toBeVisible();
  expect(screen.queryByText('A Developer')).not.toBeInTheDocument();
});
it('publish failure retains draft and locks selection while preview is pending', async () => {
  const f = fixture();
  let finish!: (value: ChangePreview) => void;
  f.client.preview = () => new Promise(resolve => { finish = resolve; }) as ReturnType<typeof f.client.preview>;
  f.client.publish = async () => {
    throw { code: 'changed', message: '文件已变化，请重新检查' };
  };
  render(<App client={f.client}/>);
  await screen.findByText('team/project');
  draft();
  fireEvent.click(screen.getByRole('button', { name: '检查变更' }));
  expect(screen.getByRole('button', { name: '后端 backend' })).toBeDisabled();
  finish({ projectId: 'p1', role: 'frontend', head: 'b'.repeat(40), fingerprint: 'fp1', files: [{ path: 'apps/web/product.tsx', status: 'M' }], outsideCount: 0, diff: 'diff' });
  await screen.findByText('apps/web/product.tsx');
  fireEvent.click(screen.getByRole('button', { name: '提交并推送' }));
  await screen.findByText(/文件已变化/);
  expect(screen.getByLabelText('页面或模块')).toHaveValue('商品页');
  expect(screen.getByLabelText('变更说明')).toHaveValue('修复筛选');
  expect(screen.getByRole('button', { name: '提交并推送' })).toBeDisabled();
});
it('standard title counts Unicode characters and rejects control characters', async () => {
  await mounted();
  draft();
  fireEvent.click(screen.getByRole('button', { name: '检查变更' }));
  await screen.findByText('apps/web/product.tsx');
  fireEvent.change(screen.getByLabelText('变更说明'), { target: { value: '😀'.repeat(130) } });
  expect(screen.getByRole('button', { name: '提交并推送' })).toBeEnabled();
  fireEvent.change(screen.getByLabelText('变更说明'), { target: { value: '😀'.repeat(160) } });
  expect(screen.getByRole('button', { name: '提交并推送' })).toBeDisabled();
  fireEvent.change(screen.getByLabelText('变更说明'), { target: { value: 'bad\nsummary' } });
  expect(screen.getByRole('button', { name: '提交并推送' })).toBeDisabled();
});
it('subscription is cleaned up even if registration resolves after unmount', async () => { const f = fixture(); const dispose = vi.fn(); let ready!: (value: () => void) => void; f.client.subscribe = () => new Promise(resolve => { ready = resolve; }); const view = render(<App client={f.client}/>); await waitFor(() => expect(ready).toBeDefined()); view.unmount(); ready(dispose); await waitFor(() => expect(dispose).toHaveBeenCalledOnce()); });
it('pending delivery cannot be removed before recovery', async () => { await mounted({ projects: [{ project, cursor: null, lastError: null, pending, lastReceipt: null, lastLocalPush: null }] }); fireEvent.click(screen.getByText(/身份目录映射/)); expect(screen.getByRole('button', { name: /移除项目登记/ })).toBeDisabled(); });
it.each(['markRead', 'refresh'] as const)('a newer subscription snapshot survives an older %s response', async (source) => {
  const f = fixture();
  let emit!: (state: AppSnapshot) => void;
  let resolveStale!: (state: AppSnapshot) => void;
  const delayedSnapshot = () => new Promise<AppSnapshot>(resolve => { resolveStale = resolve; });
  f.client.subscribe = async (handler) => { emit = handler; return () => { }; };
  if (source === 'markRead') {
    f.client.markRead = delayedSnapshot;
  }
  else {
    let snapshots = 0;
    f.client.snapshot = () => snapshots++ === 0 ? Promise.resolve(f.state) : delayedSnapshot();
  }
  render(<App client={f.client}/>);
  await screen.findByText('team/project');
  if (source === 'markRead') {
    fireEvent.click(screen.getByRole('button', { name: '标记已读' }));
  }
  else {
    draft();
    fireEvent.click(screen.getByRole('button', { name: '检查变更' }));
    await screen.findByText('apps/web/product.tsx');
    fireEvent.click(screen.getByRole('button', { name: '提交并推送' }));
  }
  await waitFor(() => expect(resolveStale).toBeDefined());
  const newer: AppSnapshot = {
    ...f.state,
    revision: 3,
    messages: [{ ...message, read: true }, { ...message, id: 'm2', sha: 'd'.repeat(40), title: 'new background commit' }],
    projects: [{ ...f.state.projects[0], project: { ...project, monitorEnabled: false }, lastError: '检查远端失败，请稍后重试' }],
    systemNotifications: true,
  };
  await act(async () => emit(newer));
  expect(screen.getByText('new background commit')).toBeVisible();
  await act(async () => resolveStale({ ...f.state, messages: [{ ...message, read: true
      }] }));
  await waitFor(() => expect(screen.getByRole('button', { name: '检查变更' })).toBeEnabled());
  expect(screen.getByText('new background commit')).toBeVisible();
  expect(screen.getByRole('checkbox', { name: '系统通知' })).toBeChecked();
  expect(screen.getByRole('checkbox', { name: '后台监控' })).not.toBeChecked();
  expect(screen.getByText('检查远端失败，请稍后重试')).toBeVisible();
});
it('final review keeps authoritative newer events despite late older native emits', async () => {
  const f = fixture();
  let emit!: (value: AppSnapshot) => void;
  f.client.subscribe = async (handler) => { emit = handler; return () => { }; };
  render(<App client={f.client}/>);
  await screen.findByText('team/project');
  await act(async () => emit({ ...f.state, revision: 12, systemNotifications: true, messages: [{ ...message, read: true, title: 'newer persisted delivery' }] }));
  await act(async () => emit({ ...f.state, revision: 11 }));
  expect(screen.getByText('newer persisted delivery')).toBeVisible();
  expect(screen.getByRole('checkbox', { name: '系统通知' })).toBeChecked();
  expect(screen.queryByRole('button', { name: '标记已读' })).not.toBeInTheDocument();
});
it('final review reconciles failed receipt only with matching positive delivery proof', async () => {
  const f = fixture();
  let emit!: (value: AppSnapshot) => void;
  f.client.subscribe = async handler => {
    emit = handler;
    return () => {};
  };
  f.client.publish = async () => pending;
  render(<App client={f.client} />);
  await screen.findByText('team/project');
  draft();
  fireEvent.click(screen.getByRole('button', { name: '检查变更' }));
  await screen.findByText('apps/web/product.tsx');
  fireEvent.click(screen.getByRole('button', { name: '提交并推送' }));
  await screen.findByText('待重试交付');
  await act(async () => emit({ ...f.state, revision: 2 }));
  expect(screen.getByRole('button', { name: '重试推送' })).toBeVisible();

  const proof = { ...message, sha: pending.commit, title: pending.message, own: true, read: true };
  let revision = 3;
  for (const mismatch of [
    { own: false }, { rewritten: true }, { projectId: 'another-project' },
    { repository: 'other/repository' }, { branch: 'other' }, { sha: 'f'.repeat(40) },
  ]) {
    await act(async () => emit({ ...f.state, revision: revision++, messages: [{ ...proof, ...mismatch }] }));
    expect(screen.getByRole('button', { name: '重试推送' })).toBeVisible();
  }
  await act(async () => emit({ ...f.state, revision, messages: [proof] }));
  expect(screen.queryByText('待重试交付')).not.toBeInTheDocument();
  expect(screen.queryByText(pending.error!)).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: '重试推送' })).not.toBeInTheDocument();
});

it('known retry success remains visible when saving the updated Store snapshot fails', async () => {
  const f = fixture({ projects: [{ project, cursor: null, lastError: null, pending, lastReceipt: null, lastLocalPush: null }] });
  f.client.retryPush = async () => ({ ...pending, pushed: true, error: null, persistenceWarning: '应用状态保存失败，已知交付成功' });
  render(<App client={f.client} />);
  await screen.findByText('team/project');
  fireEvent.click(screen.getByRole('button', { name: '重试推送' }));
  await screen.findAllByText('交付已推送');
  expect(screen.queryByRole('button', { name: '重试推送' })).not.toBeInTheDocument();
  expect(screen.getByText('应用状态保存失败，已知交付成功')).toBeVisible();
  expect(screen.getByText('aaaaaaaaaaaa')).toBeVisible();
});

it('local commit does not call publish or existing push', async () => {
 const f = await mounted(); const local = vi.spyOn(f.client, 'commitLocal'); const publish = vi.spyOn(f.client, 'publish'); const push = vi.spyOn(f.client, 'pushLocalCommits');
 draft(); fireEvent.click(screen.getByRole('button', { name: '检查变更' })); await screen.findByText('apps/web/product.tsx');
 fireEvent.click(screen.getByRole('button', { name: '仅提交到本地' })); await waitFor(() => expect(local).toHaveBeenCalledTimes(1));
 expect(publish).not.toHaveBeenCalled(); expect(push).not.toHaveBeenCalled(); expect(screen.getByText('本地已提交，未推送')).toBeVisible(); expect(screen.queryByRole('button', {name:'重试推送'})).not.toBeInTheDocument();
});
it('history displays original metadata and fills standard title without writing Git', async () => {
 const f = await mounted(); const publish = vi.spyOn(f.client,'publish'); const local=vi.spyOn(f.client,'commitLocal');
 fireEvent.click(screen.getByRole('button',{name:'读取本地历史'})); await screen.findByText('原始正文');
 expect(screen.getByText(historyCommit.sha)).toBeVisible(); expect(screen.getByText(/Real Author/)).toBeVisible();
 fireEvent.click(screen.getByRole('button',{name:'填充提交说明'})); expect(screen.getByLabelText('提交类型')).toHaveValue('fix'); expect(screen.getByLabelText('页面或模块')).toHaveValue('商品页'); expect(screen.getByLabelText('变更说明')).toHaveValue('修复筛选');
 expect(publish).not.toHaveBeenCalled(); expect(local).not.toHaveBeenCalled(); expect(screen.getByText(historyCommit.sha)).toBeVisible();
});
it('ordinary history title remains original and requires a user module', async () => {
 const f=fixture(); f.client.localHistory=async()=>({head:historyCommit.sha,commits:[{...historyCommit,title:'Original ordinary title',parsed:null}],nextOffset:null}); render(<App client={f.client}/>); await screen.findByText('team/project');
 fireEvent.click(screen.getByRole('button',{name:'读取本地历史'})); await screen.findByText('Original ordinary title'); fireEvent.click(screen.getByRole('button',{name:'填充提交说明'}));
 expect(screen.getByLabelText('页面或模块')).toHaveValue(''); expect(screen.getByLabelText('变更说明')).toHaveValue('Original ordinary title'); expect(screen.getByLabelText('提交类型')).toHaveValue('feat');
});
it('history pagination binds HEAD and clears stale pages on head change', async () => {
 const f=fixture(); const read=vi.fn().mockResolvedValueOnce({head:historyCommit.sha,commits:[historyCommit],nextOffset:50}).mockRejectedValueOnce({code:'head_changed',message:'HEAD 已变化，请重新读取第一页历史'}); f.client.localHistory=read; render(<App client={f.client}/>); await screen.findByText('team/project');
 fireEvent.click(screen.getByRole('button',{name:'读取本地历史'})); await screen.findByText('原始正文'); fireEvent.click(screen.getByRole('button',{name:'加载更多历史'})); await screen.findByRole('alert');
 expect(read).toHaveBeenLastCalledWith('p1',50,historyCommit.sha); expect(screen.queryByText('原始正文')).not.toBeInTheDocument();
});
it('complete push range must be displayed before explicit push and passes only its fingerprint', async () => {
 const f=await mounted(); const push=vi.spyOn(f.client,'pushLocalCommits'); expect(screen.getByRole('button',{name:'推送这些提交'})).toBeDisabled(); expect(push).not.toHaveBeenCalled();
 fireEvent.click(screen.getByRole('button',{name:'检查待推送提交'})); await screen.findByText('ordinary title'); expect(screen.getAllByText(historyCommit.sha)).toHaveLength(2); expect(screen.getByText('f'.repeat(40))).toBeVisible(); expect(screen.getByText('目标 HEAD')).toBeVisible(); expect(push).not.toHaveBeenCalled();
 fireEvent.click(screen.getByRole('button',{name:'推送这些提交'})); await waitFor(()=>expect(push).toHaveBeenCalledWith('p1','range-fp'));
});
it('empty push range cannot be pushed', async () => { const f=fixture(); f.client.previewLocalPush=async()=>({...pushPreview,commits:[]}); render(<App client={f.client}/>); await screen.findByText('team/project'); fireEvent.click(screen.getByRole('button',{name:'检查待推送提交'})); await screen.findByText('没有待推送提交'); expect(screen.getByRole('button',{name:'推送这些提交'})).toBeDisabled(); });
it('history fill invalidates file and range previews', async () => {
 await mounted(); draft(); fireEvent.click(screen.getByRole('button',{name:'检查变更'})); await screen.findByText('apps/web/product.tsx'); fireEvent.click(screen.getByRole('button',{name:'检查待推送提交'})); await screen.findByText('ordinary title');
 fireEvent.click(screen.getByRole('button',{name:'读取本地历史'})); await screen.findByText('原始正文'); fireEvent.click(screen.getByRole('button',{name:'填充提交说明'})); expect(screen.getByRole('button',{name:'推送这些提交'})).toBeDisabled(); expect(screen.getByRole('button',{name:'仅提交到本地'})).toBeDisabled();
});
it('project lastReceipt success clears old failure even when message belongs to another clone', async()=> {
 await mounted({ projects:[{project,cursor:null,lastError:null,pending,lastReceipt:{...pending,pushed:true,error:null},lastLocalPush:null}],messages:[{...message,projectId:'clone',own:true,sha:pending.commit}] }); expect(screen.queryByText('待重试交付')).not.toBeInTheDocument(); expect(screen.queryByRole('button',{name:'重试推送'})).not.toBeInTheDocument(); expect(screen.getByText('交付已推送')).toBeVisible();
});
it('wrong-project receipt cannot acknowledge pending', async()=>{ await mounted({projects:[{project,cursor:null,lastError:null,pending,lastReceipt:{...pending,projectId:'other',pushed:true,error:null},lastLocalPush:null}]}); expect(screen.getByRole('button',{name:'重试推送'})).toBeVisible(); });
it('settings support multiple folders picker cancel default drafts and explicit save', async()=> {
 const f=await mounted(); const save=vi.spyOn(f.client,'setRolePaths'); fireEvent.click(screen.getByText(/身份目录映射/)); fireEvent.click(screen.getByRole('button',{name:'编辑身份目录'})); fireEvent.change(screen.getByLabelText('前端目录'),{target:{value:'apps/web\npackages/ui'}}); expect(save).not.toHaveBeenCalled(); fireEvent.click(screen.getByRole('button',{name:'恢复默认草稿'})); expect(screen.getByLabelText('前端目录')).toHaveValue('frontend'); expect(save).not.toHaveBeenCalled(); fireEvent.click(screen.getByRole('button',{name:'选择前端目录'})); await waitFor(()=>expect(screen.getByLabelText('前端目录')).toHaveValue('frontend\npackages/ui')); fireEvent.click(screen.getByRole('button',{name:'取消目录编辑'})); expect(save).not.toHaveBeenCalled();
 fireEvent.click(screen.getByRole('button',{name:'编辑身份目录'})); fireEvent.change(screen.getByLabelText('前端目录'),{target:{value:'apps/web\npackages/ui'}}); fireEvent.click(screen.getByRole('button',{name:'保存身份目录'})); await waitFor(()=>expect(save).toHaveBeenCalledWith('p1',expect.objectContaining({frontend:['apps/web','packages/ui']})));
});
it('settings Store failure preserves draft for same-mapping retry', async()=> { const f=fixture(); const save=vi.fn().mockRejectedValueOnce({message:'配置已写入，应用状态保存失败，请保存同一映射重试'}).mockImplementation(f.client.setRolePaths); f.client.setRolePaths=save; render(<App client={f.client}/>); await screen.findByText('team/project'); fireEvent.click(screen.getByText(/身份目录映射/)); fireEvent.click(screen.getByRole('button',{name:'编辑身份目录'})); fireEvent.change(screen.getByLabelText('前端目录'),{target:{value:'apps/web\npackages/ui'}}); fireEvent.click(screen.getByRole('button',{name:'保存身份目录'})); await screen.findByRole('alert'); expect(screen.getByLabelText('前端目录')).toHaveValue('apps/web\npackages/ui'); fireEvent.click(screen.getByRole('button',{name:'保存身份目录'})); await waitFor(()=>expect(save).toHaveBeenCalledTimes(2)); });
it('old project late history cannot contaminate current selection', async()=> {
 const f=fixture(); let emit!:(s:AppSnapshot)=>void; let finish!:(v:Awaited<ReturnType<CompanionClient['localHistory']>>)=>void; f.client.subscribe=async h=>{emit=h;return()=>{}}; f.client.localHistory=()=>new Promise(r=>{finish=r}); render(<App client={f.client}/>); await screen.findByText('team/project'); fireEvent.click(screen.getByRole('button',{name:'读取本地历史'}));
 await act(async()=>emit({...f.state,revision:4,selectedProject:'p2',projects:[...f.state.projects,{...f.state.projects[0],project:{...project,id:'p2',repository:'other/project'}}]})); await screen.findByText('other/project'); await act(async()=>finish({head:historyCommit.sha,commits:[historyCommit],nextOffset:null})); expect(screen.queryByText('原始正文')).not.toBeInTheDocument();
});

it('late role-folder selection does not apply to another project', async () => {
  const f = fixture(); let emit!: (s: AppSnapshot) => void; let finish!: (s: string | null) => void;
  f.client.subscribe = async handler => { emit = handler; return () => {}; };
  f.client.pickRoleFolder = () => new Promise(resolve => { finish = resolve; });
  render(<App client={f.client} />); await screen.findByText('team/project');
  fireEvent.click(screen.getByText(/身份目录映射/)); fireEvent.click(screen.getByRole('button', { name: '编辑身份目录' }));
  fireEvent.click(screen.getByRole('button', { name: '选择前端目录' }));
  await act(async () => emit({ ...f.state, revision: 4, selectedProject: 'p2', projects: [...f.state.projects, { ...f.state.projects[0], project: { ...project, id: 'p2', repository: 'other/project' } }] }));
  await act(async () => finish('stale/folder'));
  expect(screen.queryByLabelText('前端目录')).not.toBeInTheDocument(); expect(screen.queryByText('stale/folder')).not.toBeInTheDocument();
});
it('late preview HEAD and role responses cannot re-enable a previous project push', async () => {
  const f = fixture(); let emit!: (s: AppSnapshot) => void; let finish!: (v: typeof pushPreview) => void;
  f.client.subscribe = async handler => { emit = handler; return () => {}; };
  f.client.previewLocalPush = () => new Promise(resolve => { finish = resolve; });
  render(<App client={f.client} />); await screen.findByText('team/project');
  fireEvent.click(screen.getByRole('button', { name: '检查待推送提交' }));
  await act(async () => emit({ ...f.state, revision: 4, selectedRole: 'backend' }));
  await act(async () => finish(pushPreview));
  expect(screen.queryByText('ordinary title')).not.toBeInTheDocument(); expect(screen.getByRole('button', { name: '推送这些提交' })).toBeDisabled();
});
it('failed range proof stays actionable and prevents settings edits', async () => {
  await mounted({ projects: [{ project, cursor: null, lastError: null, pending: null, lastReceipt: null, lastLocalPush: { ...pushPreview, pushed: false, error: '范围失败', persistenceWarning: null, url: pending.url } }] });
  expect(screen.getByText('范围失败')).toBeVisible(); expect(screen.getByRole('button', { name: '仅提交到本地' })).toBeDisabled();
  fireEvent.click(screen.getByText(/身份目录映射/)); expect(screen.getByRole('button', { name: '编辑身份目录' })).toBeDisabled();
});
it('browser disables history, local-only and range writes', () => {
  render(<App client={null} />);
  for (const name of ['读取本地历史', '仅提交到本地', '检查待推送提交', '推送这些提交']) expect(screen.getByRole('button', { name })).toBeDisabled();
});

it('new range HEAD proof invalidates a late preview even without a role receipt', async () => {
  const f = fixture(); let emit!: (s: AppSnapshot) => void; let finish!: (v: typeof pushPreview) => void;
  f.client.subscribe = async handler => { emit = handler; return () => {}; };
  f.client.previewLocalPush = () => new Promise(resolve => { finish = resolve; });
  render(<App client={f.client} />); await screen.findByText('team/project');
  fireEvent.click(screen.getByRole('button', { name: '检查待推送提交' }));
  await act(async () => emit({ ...f.state, revision: 4, projects: [{ ...f.state.projects[0], lastLocalPush: { ...pushPreview, head: 'a'.repeat(40), pushed: true, error: null, persistenceWarning: null, url: pending.url } }] }));
  await act(async () => finish(pushPreview));
  expect(screen.queryByText('ordinary title')).not.toBeInTheDocument(); expect(screen.getByRole('button', { name: '推送这些提交' })).toBeDisabled();
});

it('settings editing invalidates both file and push previews before saving', async () => {
  await mounted(); draft(); fireEvent.click(screen.getByRole('button', { name: '检查变更' })); await screen.findByText('apps/web/product.tsx');
  fireEvent.click(screen.getByRole('button', { name: '检查待推送提交' })); await screen.findByText('ordinary title');
  expect(screen.getByRole('button', { name: '推送这些提交' })).toBeEnabled();
  fireEvent.click(screen.getByText(/身份目录映射/)); fireEvent.click(screen.getByRole('button', { name: '编辑身份目录' }));
  expect(screen.getByRole('button', { name: '仅提交到本地' })).toBeDisabled(); expect(screen.getByRole('button', { name: '推送这些提交' })).toBeDisabled();
  expect(screen.queryByText('apps/web/product.tsx')).not.toBeInTheDocument(); expect(screen.queryByText('ordinary title')).not.toBeInTheDocument();
});

it.each(['covered', 'uncovered', 'wrong-project', 'failed'] as const)('known range %s outcome respects exact role SHA and keeps persistence warning', async outcome => {
  const f = fixture({ projects: [{ project, cursor: null, lastError: null, pending, lastReceipt: null, lastLocalPush: null }] });
  f.client.previewLocalPush = async () => pushPreview;
  f.client.pushLocalCommits = async () => ({
    projectId: outcome === 'wrong-project' ? 'another-project' : 'p1',
    head: historyCommit.sha, remoteHead: 'b'.repeat(40), url: pending.url,
    commits: [{ ...historyCommit, sha: outcome === 'uncovered' ? historyCommit.sha : pending.commit }],
    pushed: outcome !== 'failed', error: outcome === 'failed' ? '范围失败' : null,
    persistenceWarning: '已知范围结果，应用状态保存失败',
  });
  render(<App client={f.client} />); await screen.findByText('team/project');
  fireEvent.click(screen.getByRole('button', { name: '检查待推送提交' })); await screen.findByText('ordinary title');
  fireEvent.click(screen.getByRole('button', { name: '推送这些提交' }));
  await waitFor(() => expect(screen.getByRole('button', { name: '检查待推送提交' })).toBeEnabled());
  if (outcome === 'covered') {
    expect(screen.queryByRole('button', { name: '重试推送' })).not.toBeInTheDocument();
    expect(screen.queryByText('待重试交付')).not.toBeInTheDocument(); expect(screen.queryByText(pending.error!)).not.toBeInTheDocument();
    expect(screen.getByText('交付已推送')).toBeVisible();
  } else { expect(screen.getByRole('button', { name: '重试推送' })).toBeVisible(); }
  if (outcome !== 'wrong-project') expect(screen.getByText('已知范围结果，应用状态保存失败')).toBeVisible();
});

it.each([
  '已知完整范围成功，但应用状态保存失败；请恢复权限',
  '状态已保存，但界面刷新失败，请重新打开主窗口',
])('same-HEAD saved range success preserves fresh exact warning: %s', async warning => {
  const covered = { ...historyCommit, sha: pending.commit };
  const saved = { ...pushPreview, commits: [covered], pushed: true, error: null, persistenceWarning: null, url: pending.url };
  const f = fixture({ projects: [{ project, cursor: null, lastError: null, pending, lastReceipt: null, lastLocalPush: saved }] });
  f.client.pushLocalCommits = async () => ({ ...saved, persistenceWarning: warning });
  render(<App client={f.client} />); await screen.findByText('team/project');
  fireEvent.click(screen.getByRole('button', { name: '检查待推送提交' })); await screen.findByText('ordinary title');
  fireEvent.click(screen.getByRole('button', { name: '推送这些提交' }));
  await waitFor(() => expect(screen.getByRole('button', { name: '检查待推送提交' })).toBeEnabled());
  expect(screen.getByText(warning)).toBeVisible();
  expect(screen.queryByRole('button', { name: '重试推送' })).not.toBeInTheDocument();
  expect(screen.queryByText('待重试交付')).not.toBeInTheDocument();
  expect(screen.queryByText(pending.error!)).not.toBeInTheDocument();
  expect(screen.getByText('交付已推送')).toBeVisible();
  expect(screen.getAllByText('完整范围已推送').length).toBeGreaterThan(0);
});
it('same-HEAD durable range success remains monotonic with fresh warning and metadata', async () => {
  const saved = { ...pushPreview, commits: [{ ...historyCommit, sha: pending.commit }], pushed: true, error: null, persistenceWarning: null, url: pending.url };
  const warning = '新的范围状态保存警告';
  const f = fixture({ projects: [{ project, cursor: null, lastError: null, pending, lastReceipt: null, lastLocalPush: saved }] });
  f.client.pushLocalCommits = async () => ({ ...saved, commits: pushPreview.commits, pushed: false, error: '迟到失败不能覆盖已知成功', persistenceWarning: warning });
  render(<App client={f.client} />); await screen.findByText('team/project');
  fireEvent.click(screen.getByRole('button', { name: '检查待推送提交' })); await screen.findByText('ordinary title');
  fireEvent.click(screen.getByRole('button', { name: '推送这些提交' }));
  await waitFor(() => expect(screen.getByRole('button', { name: '检查待推送提交' })).toBeEnabled());
  expect(screen.getByText(warning)).toBeVisible();
  expect(screen.getByText('2 条提交 · 原 SHA 保留')).toBeVisible();
  expect(screen.queryByText('迟到失败不能覆盖已知成功')).not.toBeInTheDocument();
  expect(screen.queryByText('已有提交已保留，范围推送未完成')).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: '重试推送' })).not.toBeInTheDocument();
});

it('same-HEAD verified fresh success without warning resolves the previous warning', async () => {
  const previousWarning = '旧范围持久化警告';
  const saved = { ...pushPreview, pushed: true, error: null, persistenceWarning: previousWarning, url: pending.url };
  const f = fixture({ projects: [{ project, cursor: null, lastError: null, pending: null, lastReceipt: null, lastLocalPush: saved }] });
  f.client.pushLocalCommits = async () => ({ ...saved, persistenceWarning: null });
  render(<App client={f.client} />); await screen.findByText('team/project'); expect(screen.getByText(previousWarning)).toBeVisible();
  fireEvent.click(screen.getByRole('button', { name: '检查待推送提交' })); await screen.findByText('ordinary title');
  fireEvent.click(screen.getByRole('button', { name: '推送这些提交' }));
  await waitFor(() => expect(screen.getByRole('button', { name: '检查待推送提交' })).toBeEnabled());
  expect(screen.queryByText(previousWarning)).not.toBeInTheDocument(); expect(screen.getAllByText('完整范围已推送').length).toBeGreaterThan(0);
});
it('different-HEAD saved success cannot replace fresh failed range outcome or warning', async () => {
  const saved = { ...pushPreview, pushed: true, error: null, persistenceWarning: null, url: pending.url };
  const f = fixture({ projects: [{ project, cursor: null, lastError: null, pending, lastReceipt: null, lastLocalPush: saved }] });
  f.client.pushLocalCommits = async () => ({ ...saved, head: 'f'.repeat(40), pushed: false, error: '新 HEAD 范围失败', persistenceWarning: '新 HEAD 持久化警告' });
  render(<App client={f.client} />); await screen.findByText('team/project');
  fireEvent.click(screen.getByRole('button', { name: '检查待推送提交' })); await screen.findByText('ordinary title');
  fireEvent.click(screen.getByRole('button', { name: '推送这些提交' }));
  await waitFor(() => expect(screen.getByRole('button', { name: '检查待推送提交' })).toBeEnabled());
  expect(screen.getByText('新 HEAD 范围失败')).toBeVisible(); expect(screen.getByText('新 HEAD 持久化警告')).toBeVisible();
  expect(screen.getByRole('button', { name: '重试推送' })).toBeVisible();
});
