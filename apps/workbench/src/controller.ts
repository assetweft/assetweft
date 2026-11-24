import type { WorkbenchBridge } from './bridge';
import type { FeedbackRequest, ImportRequest, LibrarySession, Preview, Role, Validation, VersionDetail, VersionList, VersionRow } from './types';

export interface WorkbenchView {
  session(value: LibrarySession | null): void;
  list(value: VersionList | null, query: string): void;
  selection(value: VersionRow | null): void;
  detail(value: VersionDetail): void;
  preview(value: Preview): void;
  validation(value: Validation): void;
  error(area: 'catalog' | 'detail' | 'preview' | 'validation', message: string): void;
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
    this.view.session(session);
    this.view.selection(null);
    this.view.list(null, '');
    if (session) await this.search('');
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
    const session = await this.bridge.openLibrary(path, create);
    await this.setSession(session);
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
