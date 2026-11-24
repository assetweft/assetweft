import './styles/tokens.css';
import './styles/layout.css';
import './styles/components.css';
import { bridge } from './bridge';
import { WorkbenchController } from './controller';
import type { LibrarySession } from './types';
import { assetList } from './ui/patterns/asset-list';
import { libraryDialogs } from './ui/patterns/dialogs';
import { inspectorPanel } from './ui/patterns/inspector-panel';
import { previewStage } from './ui/patterns/preview-stage';
import { makeNotice } from './ui/primitives';

let session: LibrarySession | null = null;
const libraryLabel = document.querySelector<HTMLElement>('#library-label')!;
const importButton = document.querySelector<HTMLButtonElement>('#import-file')!;
const importStatus = makeNotice();
libraryLabel.after(importStatus);
const dialogs = libraryDialogs({
  open: (path, create) => controller.open(path, create),
  importFile: async (request) => {
    const result = await controller.import(request);
    importStatus.textContent = `Imported ${result.versionId}.`;
  },
});
const catalog = assetList(document.getElementById('assets')!, {
  search: (query) => { void controller.search(query); },
  select: (row) => { void controller.select(row); },
  importFile: dialogs.importFile,
});
const stage = previewStage(document.getElementById('preview')!);
const inspector = inspectorPanel(document.getElementById('inspector')!, {
  validate: (role) => controller.validate(role),
  feedback: (request) => controller.feedback(request),
});
const controller = new WorkbenchController(bridge, {
  session(value) {
    session = value;
    importStatus.textContent = '';
    const label = value ? `SQLite v${value.schemaVersion} · ${value.dbPath}` : 'No local library open';
    libraryLabel.textContent = label;
    libraryLabel.title = label;
    importButton.disabled = !value;
    catalog.session(Boolean(value));
  },
  list: catalog.render,
  selection(row) {
    catalog.select(row?.versionId);
    stage.selection(row);
    inspector.selection(row);
  },
  detail: inspector.render,
  preview: (preview) => { void stage.render(preview); },
  validation: inspector.validation,
  error(area, message) {
    if (area === 'catalog') catalog.error(message);
    else if (area === 'preview') stage.error(message);
    else inspector.error(message, area === 'validation');
  },
});
document.getElementById('open-library')!.addEventListener('click', dialogs.open);
importButton.addEventListener('click', dialogs.importFile);
document.getElementById('capabilities')!.addEventListener('click', () => dialogs.capabilities(session));
void controller.start();
