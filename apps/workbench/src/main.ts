import './styles/tokens.css';
import './styles/layout.css';
import './styles/components.css';
import { bridge } from './bridge';
import { WorkbenchController } from './controller';
import type { LibrarySession, WorkspaceMode } from './types';
import { budgetPanel } from './ui/patterns/budget-panel';
import { candidatesPanel } from './ui/patterns/candidates-panel';
import { capabilitiesPanel } from './ui/patterns/capabilities-panel';
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
const modes = ['catalog', 'budget', 'candidates', 'capabilities'] as const;
const tabs = modes.map((mode) => document.querySelector<HTMLButtonElement>(`#tab-${mode}`)!);
const panels = modes.map((mode) => document.getElementById(`${mode}-workspace`)!);
const budget = budgetPanel(panels[1], {
  approve: (req) => controller.budgetAction((adapter) => adapter.approveBudget(req)),
  reserve: (req) => controller.budgetAction((adapter) => adapter.reserveCall(req)),
  cancel: (id) => controller.budgetAction((adapter) => adapter.cancelCall(id)),
  reconcile: (req) => controller.budgetAction((adapter) => adapter.reconcileCall(req)),
  run: (req) => controller.runByoGpu(req),
});
const candidates = candidatesPanel(panels[2], {
  initialize: (req) => controller.initializeHead(req),
  propose: (req) => controller.proposeCandidate(req),
  attempt: (id) => controller.attemptCandidate(id),
});
const capabilities = capabilitiesPanel(panels[3], (req) => controller.runCapabilityCheck(req));
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
  budget(value) {
    budget.render(value);
    if (!value) budget.error(session ? 'Loading budget overview…' : 'Open a local library to view budgets.');
  },
  candidates(value) {
    candidates.render(value);
    if (!value) candidates.error(session ? 'Loading candidate overview…' : 'Open a local library to view candidates.');
  },
  capabilities: capabilities.render,
  run: budget.run,
  budgetReceipt: budget.receipt,
  initializedHead: candidates.initializedHead,
  proposedCandidate: candidates.proposedCandidate,
  attempt: candidates.attempt,
  error(area, message) {
    if (area === 'catalog') catalog.error(message);
    else if (area === 'budget') budget.error(message);
    else if (area === 'candidates') candidates.error(message);
    else if (area === 'capabilities') capabilities.error(message);
    else if (area === 'preview') stage.error(message);
    else inspector.error(message, area === 'validation');
  },
});
document.getElementById('open-library')!.addEventListener('click', dialogs.open);
importButton.addEventListener('click', dialogs.importFile);
document.getElementById('capabilities')!.addEventListener('click', () => dialogs.capabilities(session));
function activate(mode: WorkspaceMode) {
  for (const [index, tab] of tabs.entries()) {
    const selected = modes[index] === mode;
    tab.setAttribute('aria-selected', String(selected));
    tab.tabIndex = selected ? 0 : -1;
    panels[index].hidden = !selected;
  }
  void controller.setMode(mode);
}

for (const [index, tab] of tabs.entries()) {
  tab.addEventListener('click', () => activate(modes[index]));
  tab.addEventListener('keydown', (event) => {
    const next = {
      ArrowLeft: (index + tabs.length - 1) % tabs.length, ArrowRight: (index + 1) % tabs.length,
      Home: 0, End: tabs.length - 1,
    }[event.key];
    if (next === undefined) return;
    event.preventDefault();
    tabs[next].focus();
    activate(modes[next]);
  });
}
void controller.start();
