export interface LibrarySession {
  dbPath: string;
  schemaVersion: number;
  mode: 'LOCAL_SQLITE_NO_LOGIN';
  capabilities: { id: string; status: string; description: string }[];
}
export type Availability = 'AVAILABLE' | 'METADATA_ONLY' | 'MISSING' | 'CORRUPT';
export type Decision = 'ACCEPT' | 'REJECT';
export type Role = 'static-mesh' | 'rigged-character' | 'animated-character';
export interface VersionRow {
  assetId: string;
  versionId: string;
  sourceFormat: string;
  availability: Availability;
  latestDecision: Decision | 'UNRATED';
}
export interface VersionList { matches: VersionRow[]; truncated: boolean }
export interface FeedbackRequest {
  assetId: string;
  versionId: string;
  decision: Decision;
  feedbackId: string;
  reason: string | null;
}
export interface FeedbackRecord extends FeedbackRequest { sequence: number }
export interface VersionDetail {
  assetId: string;
  versionId: string;
  sourceFormat: string;
  availability: Availability;
  contentHash: string;
  sourceClass: string;
  source: string;
  licenseId: string;
  usageScope: string;
  byteLength: number | null;
  references: { role: string; componentVersionId: string }[];
  current: { revision: number; version_id: string; contract: { contract_id: string } } | null;
  latestFeedback: FeedbackRecord | null;
  history: FeedbackRecord[];
}
export interface Validation {
  checkerVersion: string;
  fileSha256: string;
  bytesRead: number;
  format: string;
  formatStatus: string;
  role: { requirement: string; status: string; satisfied: boolean | null; present: string[]; missing: string[] };
  checks: { id: string; status: string; detail: string }[];
  notRun: string[];
  units: string;
  source: { class: string; declaredLicense: string | null; verified: boolean };
}
export type Preview = { versionId: string } & (
  | { kind: 'GLB'; byteLength: number; contentHash: string; glbBase64: string; validation: Validation; notice: string }
  | { kind: 'PNG'; dataUrl: string; previewKind: 'CORE_THUMBNAIL' | 'CAS_RAW'; note: string | null }
  | { kind: 'WAV'; dataUrl: string; byteLength: number }
  | { kind: 'UNAVAILABLE'; availability: Availability; reason: string }
  | { kind: 'UNSUPPORTED'; availability: Availability; sourceFormat: string; reason: string }
);
export interface ImportRequest {
  filePath: string;
  assetId: string;
  versionId: string;
  sourceFormat: string;
  licenseId: string;
  usageScope: string;
}
export interface ImportResult { versionId: string; contentHash: string; byteLength: number }

export type WorkspaceMode = 'catalog' | 'budget' | 'candidates';
export type Unit = 'Token' | 'Task' | 'Second' | { CurrencyMinor: { currency: string; decimals: number } };
export type Approval = 'ZeroAdditionalCost' | 'ExplicitBounded';
export type ExactCount = number | string;
export interface BudgetUnit { label: 'Token' | 'Task' | 'Second' | 'CurrencyMinor'; currency?: string; decimals?: number }
export interface ApproveBudgetRequest {
  id: string;
  unit: Unit;
  capacity: number;
  approval: Approval;
  source: string;
}
export interface BudgetAccount {
  id: string;
  unit: BudgetUnit;
  approval: 'ZERO_ADDITIONAL_COST' | 'EXPLICIT_BOUNDED';
  source: string;
  capacity: ExactCount;
  reserved: ExactCount;
  spent: ExactCount;
  available: ExactCount;
}
export interface ReserveCallRequest {
  id: string;
  account: string;
  inputFingerprint: string;
  unit: Unit;
  quote: number | null;
  reservation: number;
  source: string;
}
export type Resolution = 'Unresolved' | 'AuthoritativeNoCharge' | { Confirmed: { charge: number } };
export interface ReconcileCallRequest { intentId: string; receiptId: string; source: string; resolution: Resolution }
export interface BudgetReceipt {
  id: string;
  source: string;
  resolution: string | { Confirmed: { charge: ExactCount } };
  usage: { unit: BudgetUnit; amount: ExactCount | null; source: string }[];
}
export interface BudgetIntent {
  id: string;
  account: string;
  unit: BudgetUnit;
  quote: ExactCount | null;
  reservation: ExactCount;
  state: 'PREPARING' | 'ATTEMPTING' | 'UNKNOWN' | 'CONFIRMED' | 'RELEASED';
  cancelled: boolean;
  charge: ExactCount | null;
  receipts: BudgetReceipt[];
  warning: string | null;
}
export interface GpuProfile {
  id: string;
  models: { id: string; upstream: string; license: string; artifact: string; notes: string }[];
  external_runtime: string;
  auth_env_vars: string[];
  auth_config: string[];
  cash_cost: null;
  is_private?: boolean;
  accelerators?: string[];
}
export interface BudgetOverview { accounts: BudgetAccount[]; intents: BudgetIntent[]; profiles: GpuProfile[] }
export interface RunByoGpuRequest {
  budgetId: string;
  intentId: string;
  profile: string;
  model: string;
  outDir: string;
  image: string | null;
  prompt: string | null;
  bundle: string | null;
  adapterBin: string | null;
  allowRemoteUpload: boolean;
  maxBytes: number;
}
export interface ByoGpuRun {
  status: 'SUCCEEDED' | 'UNKNOWN' | 'RELEASED';
  profile: string;
  model: string;
  intentId: string;
  invoked: boolean;
  artifactFile: string | null;
  artifactSha256: string | null;
  cashCost: null;
  message: string;
}
export interface CandidateHead { assetId: string; versionId: string; revision: number; contractId: string }
export interface InitHeadRequest { versionId: string; maxComponentBytes: number; requiredQuality: string[] }
export interface ProposeCandidateRequest {
  id: string;
  baseVersion: string;
  candidateVersion: string;
  expectedRevision: number;
  editable: string[];
  lockMode: 'BYTE' | 'SEMANTIC';
  maxComponentBytes: number;
  requiredQuality: string[];
}
export interface CandidateAttempt {
  candidateId: string;
  decision: 'ACCEPTED' | 'REJECTED' | 'CONFLICT' | 'NOT_RUN';
  reason: string | null;
  currentRevision: number | null;
  report: {
    scope: string;
    components: { role: string; format_status: string; role_status: string; not_run: string[] }[];
    not_run: string[];
  } | null;
}
export interface CandidateItem {
  id: string;
  assetId: string;
  baseVersion: string;
  candidateVersion: string;
  expectedRevision: number;
  editable: string[];
  lockMode: string;
  attempt: CandidateAttempt | null;
}
export interface CandidateOverview { heads: CandidateHead[]; candidates: CandidateItem[] }
