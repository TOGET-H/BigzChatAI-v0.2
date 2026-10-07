import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';
export type Role = 'product' | 'frontend' | 'backend' | 'testing' | 'skills';
export type CommitKind = 'feat' | 'fix' | 'docs' | 'test' | 'refactor' | 'chore';
export interface SyncError {
  code: string;
  message: string;
}
export interface Project {
  id: string;
  name: string;
  root: string;
  repository: string;
  remoteUrl: string;
  branch: string;
  rolePaths: Record<Role, string[]>;
  monitorEnabled: boolean;
}
export interface AddProjectInput {
  root: string;
  name: string;
  branch: string;
  createTemplate: boolean;
}
export interface FileChange {
  path: string;
  status: string;
}
export interface ChangePreview {
  projectId: string;
  role: Role;
  head: string;
  fingerprint: string;
  files: FileChange[];
  outsideCount: number;
  diff: string;
}
export interface PublishInput {
  projectId: string;
  role: Role;
  kind: string;
  module: string;
  summary: string;
  fingerprint: string;
}
export interface PublishReceipt {
  persistenceWarning: string | null;
  projectId: string;
  role: Role;
  commit: string;
  parent: string;
  message: string;
  url: string;
  pushed: boolean;
  pushRequested: boolean;
  error: string | null;
}
export interface LocalCommit {
  sha: string; parents: string[]; title: string; body: string; author: string; committedAt: string; parsed: ParsedCommit | null;
}
export interface LocalHistoryPage { head: string; commits: LocalCommit[]; nextOffset: number | null; }
export interface LocalPushPreview { projectId: string; head: string; remoteHead: string | null; fingerprint: string; commits: LocalCommit[]; }
export interface LocalPushReceipt { projectId: string; head: string; remoteHead: string | null; url: string; commits: LocalCommit[]; pushed: boolean; error: string | null; persistenceWarning: string | null; }
export interface ParsedCommit {
  kind: string;
  role: Role;
  module: string;
  summary: string;
}
export interface RemoteCommit {
  sha: string;
  title: string;
  author: string;
  committedAt: string;
}
export interface RemoteSnapshot {
  head: string;
  commits: RemoteCommit[];
  rewritten: boolean;
}
export interface UpdateMessage {
  own: boolean;
  id: string;
  projectId: string;
  repository: string;
  branch: string;
  sha: string;
  title: string;
  author: string;
  committedAt: string;
  url: string;
  parsed: ParsedCommit | null;
  read: boolean;
  rewritten: boolean;
}
export interface ProjectState {
  project: Project;
  cursor: string | null;
  lastError: string | null;
  pending: PublishReceipt | null;
  lastReceipt: PublishReceipt | null;
  lastLocalPush: LocalPushReceipt | null;
}
export interface AppSnapshot {
  revision: number;
  projects: ProjectState[];
  messages: UpdateMessage[];
  selectedProject: string | null;
  selectedRole: Role;
  systemNotifications: boolean;
}
export interface CompanionClient {
  snapshot(): Promise<AppSnapshot>;
  subscribe(handler: (state: AppSnapshot) => void): Promise<() => void>;
  pickFolder(): Promise<string | null>;
  addProject(input: AddProjectInput): Promise<AppSnapshot>;
  removeProject(projectId: string): Promise<AppSnapshot>;
  select(projectId: string | null, role: Role): Promise<AppSnapshot>;
  preview(projectId: string, role: Role): Promise<ChangePreview>;
  localHistory(projectId: string, offset?: number, expectedHead?: string | null): Promise<LocalHistoryPage>;
  commitLocal(input: PublishInput): Promise<PublishReceipt>;
  previewLocalPush(projectId: string): Promise<LocalPushPreview>;
  pushLocalCommits(projectId: string, fingerprint: string): Promise<LocalPushReceipt>;
  setRolePaths(projectId: string, rolePaths: Record<Role, string[]>): Promise<AppSnapshot>;
  pickRoleFolder(projectId: string): Promise<string | null>;
  publish(input: PublishInput): Promise<PublishReceipt>;
  retryPush(projectId: string): Promise<PublishReceipt>;
  pull(projectId: string): Promise<string>;
  checkUpdates(projectId?: string | null): Promise<AppSnapshot>;
  setMonitor(projectId: string, enabled: boolean): Promise<AppSnapshot>;
  setNotifications(enabled: boolean): Promise<AppSnapshot>;
  markRead(ids: string[]): Promise<AppSnapshot>;
  openCommit(repository: string, sha: string): Promise<void>;
  exit(): Promise<void>;
}
const nativeClient: CompanionClient = {
  snapshot: () => invoke('gitcollab_snapshot'),
  subscribe: handler => listen<AppSnapshot>('gitcollab://changed', event => handler(event.payload)),
  pickFolder: async () => {
    const path = await open({ directory: true, multiple: false, title: '选择 Git 仓库根目录' });
    return typeof path === 'string' ? path : null;
  },
  addProject: input => invoke('gitcollab_add_project', { input }),
  removeProject: projectId => invoke('gitcollab_remove_project', { projectId }),
  select: (projectId, role) => invoke('gitcollab_select', { projectId, role }),
  preview: (projectId, role) => invoke('gitcollab_preview', { projectId, role }),
  localHistory: (projectId, offset = 0, expectedHead = null) => invoke('gitcollab_local_history', { projectId, offset, expectedHead }),
  commitLocal: input => invoke('gitcollab_commit_local', { input }),
  previewLocalPush: projectId => invoke('gitcollab_preview_local_push', { projectId }),
  pushLocalCommits: (projectId, fingerprint) => invoke('gitcollab_push_local_commits', { projectId, fingerprint }),
  setRolePaths: (projectId, rolePaths) => invoke('gitcollab_set_role_paths', { projectId, rolePaths }),
  pickRoleFolder: projectId => invoke('gitcollab_pick_role_folder', { projectId }),
  publish: input => invoke('gitcollab_publish', { input }),
  retryPush: projectId => invoke('gitcollab_retry_push', { projectId }),
  pull: projectId => invoke('gitcollab_pull', { projectId }),
  checkUpdates: (projectId = null) => invoke('gitcollab_check_updates', { projectId }),
  setMonitor: (projectId, enabled) => invoke('gitcollab_set_monitor', { projectId, enabled }),
  setNotifications: enabled => invoke('gitcollab_set_notifications', { enabled }),
  markRead: ids => invoke('gitcollab_mark_read', { ids }),
  openCommit: (repository, sha) => invoke('gitcollab_open_commit', { repository, sha }),
  exit: () => invoke('gitcollab_exit'),
};
export function getClient(): CompanionClient | null {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window ? nativeClient : null;
}
