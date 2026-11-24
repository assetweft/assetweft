import { invoke } from '@tauri-apps/api/core';
import type { FeedbackRecord, FeedbackRequest, ImportRequest, ImportResult, LibrarySession, Preview, Role, Validation, VersionDetail, VersionList } from './types';

export interface WorkbenchBridge {
  bootstrap(): Promise<LibrarySession | null>;
  openLibrary(dbPath: string, createIfMissing: boolean): Promise<LibrarySession>;
  listVersions(query: string, limit: number): Promise<VersionList>;
  getVersionDetail(versionId: string): Promise<VersionDetail>;
  loadPreview(versionId: string): Promise<Preview>;
  validateVersion(versionId: string, role: Role): Promise<Validation>;
  appendFeedback(req: FeedbackRequest): Promise<FeedbackRecord>;
  importLocalFile(req: ImportRequest): Promise<ImportResult>;
}

export const bridge: WorkbenchBridge = {
  bootstrap: () => invoke('workbench_bootstrap_state'),
  openLibrary: (dbPath, createIfMissing) => invoke('open_library', { dbPath, createIfMissing }),
  listVersions: (query, limit) => invoke('list_versions', { query, limit }),
  getVersionDetail: (versionId) => invoke('get_version_detail', { versionId }),
  loadPreview: (versionId) => invoke('load_preview', { versionId }),
  validateVersion: (versionId, role) => invoke('validate_version', { versionId, role }),
  appendFeedback: (req) => invoke('append_feedback', { req }),
  importLocalFile: (req) => invoke('import_local_file', { req }),
};
