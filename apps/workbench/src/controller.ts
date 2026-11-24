import type { WorkbenchBridge } from './bridge';
import type {
  BudgetAccount, BudgetIntent, BudgetOverview, ByoGpuRun, CandidateAttempt, CandidateHead,
  CandidateItem, CandidateOverview, InitHeadRequest, ProposeCandidateRequest, RunByoGpuRequest, WorkspaceMode,
  FeedbackRequest, ImportRequest, LibrarySession, Preview, Role, Validation, VersionDetail, VersionList, VersionRow } from './types';

export interface WorkbenchView {
  session(value: LibrarySession | null): void;
  list(value: VersionList | null, query: string): void;
  selection(value: VersionRow | null): void;
  detail(value: VersionDetail): void;
  preview(value: Preview): void;
  validation(value: Validation): void;
  budget(value: BudgetOverview | null): void;
  candidates(value: CandidateOverview | null): void;
  run(value: ByoGpuRun | null): void;
  budgetReceipt(value: BudgetAccount | BudgetIntent | null): void;
  initializedHead(value: CandidateHead | null): void;
  proposedCandidate(value: CandidateItem | null): void;
  attempt(value: CandidateAttempt | null): void;
  error(area: 'catalog' | 'detail' | 'preview' | 'validation' | 'budget' | 'candidates', message: string): void;
}

export function errorMessage(error: unknown): string {
  if (error && typeof error === 'object' && 'message' in error) {
    const message = String(error.message);
    return 'code' in error ? `${String(error.code)} — ${message}` : message;
  }
  return String(error);
}

export class WorkbenchController {
  private sessionGen = 0;
  private selectionGen = 0;
  private listGen = 0;
  private validationGen = 0;
  private budgetGen = 0;
  private candidateGen = 0;
  private opening = false;
  private mode: WorkspaceMode = 'catalog';
  private session: LibrarySession | null = null;
  private selected: VersionRow | null = null;
  private query = '';

  constructor(private bridge: WorkbenchBridge, private view: WorkbenchView) {}

  private async setSession(session: LibrarySession | null) {
    this.sessionGen++;
    this.session = session;
    this.query = '';
    this.selected = null;
    this.selectionGen++;
    this.budgetGen++;
    this.candidateGen++;
    this.view.session(session);
    this.view.budget(null);
    this.view.candidates(null);
    this.view.run(null);
    this.view.budgetReceipt(null);
    this.view.initializedHead(null);
    this.view.proposedCandidate(null);
    this.view.attempt(null);
    this.view.selection(null);
    this.view.list(null, '');
    if (session) await Promise.all([this.search(''), this.refreshWorkspace()]);
  }

  async start() {
    const gen = this.sessionGen;
    try {
      const session = await this.bridge.bootstrap();
      if (gen === this.sessionGen) await this.setSession(session);
    } catch (error) {
      if (gen === this.sessionGen) this.view.error('catalog', errorMessage(error));
    }
  }

  async open(path: string, create: boolean) {
    if (this.opening) throw new Error('A library open is already in progress.');
    this.opening = true;
    this.sessionGen++;
    let session: LibrarySession | undefined;
    try {
      session = await this.bridge.openLibrary(path, create);
    } finally {
      this.opening = false;
      if (!session) void this.refreshWorkspace();
    }
    await this.setSession(session);
  }

  async setMode(mode: WorkspaceMode) {
    if (mode === this.mode) return;
    this.mode = mode;
    this.budgetGen++;
    this.candidateGen++;
    if (mode === 'budget') this.view.budget(null);
    if (mode === 'candidates') this.view.candidates(null);
    await this.refreshWorkspace();
  }

  private async refreshWorkspace() {
    if (!this.session || this.opening || this.mode === 'catalog') return;
    const mode = this.mode;
    const session = this.sessionGen;
    const gen = mode === 'budget' ? ++this.budgetGen : ++this.candidateGen;
    const current = () => session === this.sessionGen && mode === this.mode &&
      gen === (mode === 'budget' ? this.budgetGen : this.candidateGen);
    try {
      if (mode === 'budget') {
        const result = await this.bridge.budgetOverview(100);
        if (current()) this.view.budget(result);
      } else {
        const result = await this.bridge.candidateOverview(100);
        if (current()) this.view.candidates(result);
      }
    } catch (error) {
      if (current()) this.view.error(mode, errorMessage(error));
    }
  }

  private async workspaceAction<T>(
    mode: 'budget' | 'candidates', run: () => Promise<T>, show: (value: T) => void,
  ) {
    if (!this.session || this.opening || this.mode !== mode) {
      throw new Error('Open a local library and return to the workspace before submitting.');
    }
    const session = this.sessionGen;
    const gen = mode === 'budget' ? ++this.budgetGen : ++this.candidateGen;
    const result = await run();
    if (session === this.sessionGen && mode === this.mode &&
        gen === (mode === 'budget' ? this.budgetGen : this.candidateGen)) {
      show(result);
      await this.refreshWorkspace();
    }
  }

  budgetAction(run: (bridge: WorkbenchBridge) => Promise<BudgetAccount | BudgetIntent>) {
    return this.workspaceAction('budget', () => run(this.bridge), (value) => this.view.budgetReceipt(value));
  }

  runByoGpu(request: RunByoGpuRequest) {
    return this.workspaceAction('budget', () => this.bridge.runByoGpu(request), (value) => this.view.run(value));
  }

  initializeHead(request: InitHeadRequest) {
    return this.workspaceAction('candidates', () => this.bridge.initializeCandidateHead(request), (value) => this.view.initializedHead(value));
  }

  proposeCandidate(request: ProposeCandidateRequest) {
    return this.workspaceAction('candidates', () => this.bridge.proposeCandidate(request), (value) => this.view.proposedCandidate(value));
  }

  attemptCandidate(id: string) {
    return this.workspaceAction('candidates', () => this.bridge.attemptAcceptCandidate(id), (value) => this.view.attempt(value));
  }

  async search(query: string) {
    if (!this.session) return;
    this.query = query;
    const listGen = ++this.listGen;
    const sessionGen = this.sessionGen;
    const current = () => listGen === this.listGen && sessionGen === this.sessionGen;
    try {
      const list = await this.bridge.listVersions(query, 100);
      if (current()) this.view.list(list, query);
    } catch (error) {
      if (current()) this.view.error('catalog', errorMessage(error));
    }
  }

  async select(row: VersionRow) {
    if (!this.session) return;
    const gen = ++this.selectionGen;
    this.selected = row;
    this.view.selection(row);
    const versionId = row.versionId;
    await Promise.all([
      this.bridge.getVersionDetail(versionId).then((detail) => {
        if (gen === this.selectionGen) this.view.detail(detail);
      }).catch((error) => {
        if (gen === this.selectionGen) this.view.error('detail', errorMessage(error));
      }),
      this.bridge.loadPreview(versionId).then((preview) => {
        if (gen === this.selectionGen) this.view.preview(preview);
      }).catch((error) => {
        if (gen === this.selectionGen) this.view.error('preview', errorMessage(error));
      }),
    ]);
  }

  async validate(role: Role) {
    const selected = this.selected;
    if (!selected) return;
    const selection = this.selectionGen;
    const validation = ++this.validationGen;
    const current = () => selection === this.selectionGen && validation === this.validationGen;
    try {
      const report = await this.bridge.validateVersion(selected.versionId, role);
      if (current()) this.view.validation(report);
    } catch (error) {
      if (current()) this.view.error('validation', errorMessage(error));
    }
  }

  async feedback(request: FeedbackRequest) {
    const selection = this.selectionGen;
    const session = this.sessionGen;
    if (this.selected?.versionId !== request.versionId || this.selected.assetId !== request.assetId) {
      throw new Error('Select the version again before submitting feedback.');
    }
    await this.bridge.appendFeedback(request);
    if (session !== this.sessionGen) return;
    await Promise.all([
      this.search(this.query),
      this.bridge.getVersionDetail(request.versionId).then((detail) => {
        if (selection === this.selectionGen) this.view.detail(detail);
      }).catch((error) => {
        if (selection === this.selectionGen) {
          this.view.error('detail', `Feedback saved. History refresh failed. ${errorMessage(error)}`);
        }
      }),
    ]);
  }

  async import(request: ImportRequest) {
    const gen = this.sessionGen;
    const result = await this.bridge.importLocalFile(request);
    if (gen === this.sessionGen) await this.search(this.query);
    return result;
  }
}
