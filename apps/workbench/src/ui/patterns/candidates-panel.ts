import type { CandidateAttempt, CandidateHead, CandidateItem, CandidateOverview, InitHeadRequest, ProposeCandidateRequest } from '../../types';
import { button, createStatusBadge, el, facts, integerInput, labelField, makeNotice } from '../primitives';
import { actionDialog, textInput } from './dialogs';

const FIXTURE_BOUND = 16_777_216;

interface CandidateActions {
  initialize(req: InitHeadRequest): Promise<void>;
  propose(req: ProposeCandidateRequest): Promise<void>;
  attempt(id: string): Promise<void>;
}

function contractFields(form: HTMLFormElement) {
  const bytes = integerInput(1, FIXTURE_BOUND, FIXTURE_BOUND);
  const quality = textInput(false);
  form.append(labelField('Maximum component bytes', bytes), labelField('Required quality checks (comma separated)', quality));
  return () => ({
    maxComponentBytes: bytes.valueAsNumber,
    requiredQuality: quality.value.split(',').map((value) => value.trim()).filter(Boolean),
  });
}

function initializeDialog(initialize: CandidateActions['initialize']) {
  actionDialog('Initialize head', (form) => {
    const version = textInput();
    form.append(el('p', 'Initialize a managed TEST_FIXTURE version with TEST_FIXTURE_GLB_STRUCTURE_V1.'),
      labelField('Base version ID', version));
    const contract = contractFields(form);
    return () => initialize({ versionId: version.value, ...contract() });
  });
}

function proposeDialog(propose: CandidateActions['propose']) {
  actionDialog('Propose candidate', (form) => {
    const id = textInput();
    const base = textInput();
    const proposed = textInput();
    const revision = integerInput(0);
    const editable = el('select');
    editable.multiple = true;
    editable.required = true;
    for (const role of ['mesh', 'materials', 'rig', 'motion', 'vfx', 'audio', 'dialogue']) {
      editable.append(el('option', role));
    }
    form.append(labelField('Candidate ID', id), labelField('Base version ID', base),
      labelField('Proposed version ID', proposed), labelField('Expected head revision', revision),
      labelField('Editable roles (select all that apply)', editable),
      el('p', 'BYTE lock checks every role outside the selected editable set. Semantic locks are NOT_RUN.'));
    const contract = contractFields(form);
    return () => propose({
      id: id.value, baseVersion: base.value, candidateVersion: proposed.value,
      expectedRevision: revision.valueAsNumber, editable: Array.from(editable.selectedOptions, (option) => option.value),
      lockMode: 'BYTE', ...contract(),
    });
  });
}

function attemptDetails(attempt: CandidateAttempt) {
  const details = el('div', undefined, 'stack compact');
  details.append(createStatusBadge(attempt.decision));
  if (attempt.reason) details.append(el('p', attempt.reason));
  if (attempt.currentRevision !== null) details.append(el('p', `Revision at attempt: ${attempt.currentRevision}`));
  if (attempt.decision === 'CONFLICT') {
    details.append(el('p', 'The stale candidate remains available. The accepted head was preserved.', 'workspace-warning'));
  }
  if (attempt.report) {
    const disclosure = el('details');
    disclosure.append(el('summary', attempt.report.scope));
    const checks = el('ul');
    for (const component of attempt.report.components) {
      checks.append(el('li', `${component.role}: format ${component.format_status}, role ${component.role_status}`));
    }
    disclosure.append(checks, el('p', `NOT_RUN: ${attempt.report.not_run.join(', ')}`, 'muted'));
    details.append(disclosure);
  }
  return details;
}

function candidateRow(item: CandidateItem, attempt: CandidateActions['attempt']) {
  const row = el('li', undefined, 'stack compact');
  row.append(el('h3', item.id, 'mono'), facts([
    ['Asset', item.assetId], ['Base → proposed', `${item.baseVersion} → ${item.candidateVersion}`],
    ['Expected revision', item.expectedRevision], ['Editable roles', item.editable.join(', ')], ['Lock mode', item.lockMode],
  ]));
  if (item.attempt) {
    row.append(attemptDetails(item.attempt));
  } else {
    const actions = el('div', undefined, 'actions');
    actions.append(button('Attempt accept', () => actionDialog('Attempt accept', (form) => {
      form.append(el('p', `Check ${item.id} and advance the head only if the current base and revision still match.`));
      return () => attempt(item.id);
    })));
    row.append(actions);
  }
  return row;
}

export function candidatesPanel(target: HTMLElement, actions: CandidateActions) {
  const status = makeNotice();
  const proposal = makeNotice();
  const result = el('div');
  result.setAttribute('aria-live', 'polite');
  const content = el('div', undefined, 'stack');
  let overview: CandidateOverview | null = null;
  let initialized: CandidateHead | null = null;
  let latestProposal: CandidateItem | null = null;
  target.append(el('h2', 'Incremental Candidates'), status, proposal, result, content);

  function render() {
    content.replaceChildren();
    if (!overview) return;
    const heads = el('section', undefined, 'workspace-card stack');
    const headBar = el('div', undefined, 'section-bar');
    headBar.append(el('h3', 'Asset Heads'), button('Initialize head', () => initializeDialog(actions.initialize)));
    heads.append(headBar);
    const shown = overview.heads;
    const headList = el('ul', undefined, 'records');
    for (const head of shown) {
      const item = el('li');
      item.append(facts([
        ['Asset', head.assetId], ['Accepted version', head.versionId], ['Revision', head.revision], ['Contract', head.contractId],
      ]));
      headList.append(item);
    }
    heads.append(shown.length ? headList : el('p', 'No heads found for the listed candidates.', 'muted'));
    if (initialized && !shown.some((head) => head.assetId === initialized?.assetId)) {
      heads.append(el('p', 'Initialization receipt', 'muted'), facts([
        ['Asset', initialized.assetId], ['Initialized version', initialized.versionId],
        ['Revision at initialization', initialized.revision], ['Contract', initialized.contractId],
      ]));
    }

    const candidates = el('section', undefined, 'workspace-card stack');
    const candidateBar = el('div', undefined, 'section-bar');
    candidateBar.append(el('h3', 'Candidate Proposals & Accept Attempts'),
      button('Propose candidate', () => proposeDialog(actions.propose)));
    const listed = [...overview.candidates];
    if (latestProposal && !listed.some((item) => item.id === latestProposal?.id)) listed.push(latestProposal);
    const list = el('ul', undefined, 'records');
    for (const item of listed) list.append(candidateRow(item, actions.attempt));
    candidates.append(candidateBar, el('p',
      'Structural acceptance checks the frozen candidate against the current head and byte locks. Version feedback in the Catalog Inspector is independent.',
      'muted'), listed.length ? list : el('p', 'No candidate proposals.', 'muted'));
    content.append(heads, candidates);
  }

  return {
    render(value: CandidateOverview | null) {
      overview = value;
      status.textContent = '';
      render();
    },
    error(message: string) { status.textContent = message; },
    initializedHead(value: CandidateHead | null) {
      initialized = value;
      render();
    },
    proposedCandidate(value: CandidateItem | null) {
      latestProposal = value;
      proposal.textContent = value ? `Candidate ${value.id} staged.` : '';
      render();
    },
    attempt(value: CandidateAttempt | null) {
      result.replaceChildren();
      if (!value) return;
      if (latestProposal?.id === value.candidateId) latestProposal = { ...latestProposal, attempt: value };
      result.append(el('h3', `Attempt for ${value.candidateId}`), attemptDetails(value));
    },
  };
}
