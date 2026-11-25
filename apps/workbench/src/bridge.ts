import { invoke } from '@tauri-apps/api/core';
import type {
  CapabilityOverview, CapabilityRunResult, RunCapabilityRequest,
  ApproveBudgetRequest, BudgetAccount, BudgetIntent, BudgetOverview, ByoGpuRun, CandidateAttempt,
  CandidateHead, CandidateItem, CandidateOverview, InitHeadRequest, ProposeCandidateRequest,
  ReconcileCallRequest, ReserveCallRequest, RunByoGpuRequest,
  FeedbackRecord, FeedbackRequest, ImportRequest, ImportResult, LibrarySession, Preview, Role, Validation, VersionDetail, VersionList } from './types';

export interface WorkbenchBridge {
  capabilityOverview(): Promise<CapabilityOverview>;
  runCapabilityCheck(req: RunCapabilityRequest): Promise<CapabilityRunResult>;
  budgetOverview(limit: number): Promise<BudgetOverview>;
  approveBudget(req: ApproveBudgetRequest): Promise<BudgetAccount>;
  reserveCall(req: ReserveCallRequest): Promise<BudgetIntent>;
  cancelCall(intentId: string): Promise<BudgetIntent>;
  reconcileCall(req: ReconcileCallRequest): Promise<BudgetIntent>;
  runByoGpu(req: RunByoGpuRequest): Promise<ByoGpuRun>;
  candidateOverview(limit: number): Promise<CandidateOverview>;
  initializeCandidateHead(req: InitHeadRequest): Promise<CandidateHead>;
  proposeCandidate(req: ProposeCandidateRequest): Promise<CandidateItem>;
  attemptAcceptCandidate(candidateId: string): Promise<CandidateAttempt>;
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
  capabilityOverview: () => invoke('capability_overview'),
  runCapabilityCheck: (req) => invoke('run_capability_check', { req }),
  budgetOverview: (limit) => invoke('budget_overview', { limit }),
  approveBudget: (req) => invoke('approve_budget', { req }),
  reserveCall: (req) => invoke('reserve_call', { req }),
  cancelCall: (intentId) => invoke('cancel_call', { intentId }),
  reconcileCall: (req) => invoke('reconcile_call', { req }),
  runByoGpu: (req) => invoke('run_byo_gpu', { req }),
  candidateOverview: (limit) => invoke('candidate_overview', { limit }),
  initializeCandidateHead: (req) => invoke('initialize_candidate_head', { req }),
  proposeCandidate: (req) => invoke('propose_candidate', { req }),
  attemptAcceptCandidate: (candidateId) => invoke('attempt_accept_candidate', { candidateId }),
  bootstrap: () => invoke('workbench_bootstrap_state'),
  openLibrary: (dbPath, createIfMissing) => invoke('open_library', { dbPath, createIfMissing }),
  listVersions: (query, limit) => invoke('list_versions', { query, limit }),
  getVersionDetail: (versionId) => invoke('get_version_detail', { versionId }),
  loadPreview: (versionId) => invoke('load_preview', { versionId }),
  validateVersion: (versionId, role) => invoke('validate_version', { versionId, role }),
  appendFeedback: (req) => invoke('append_feedback', { req }),
  importLocalFile: (req) => invoke('import_local_file', { req }),
};
