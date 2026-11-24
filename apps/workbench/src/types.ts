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
