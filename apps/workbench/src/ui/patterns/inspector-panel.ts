import type { Decision, FeedbackRequest, Role, Validation, VersionDetail, VersionRow } from '../../types';
import { errorMessage } from '../../controller';
import { attachAccessibleFormValidation, button, createStatusBadge, el, labelField, makeNotice } from '../primitives';

function section(title: string) {
  const node = el('section');
  node.append(el('h3', title));
  return node;
}

function facts(entries: [string, string | HTMLElement][]) {
  const node = el('dl', undefined, 'facts');
  for (const [key, value] of entries) {
    const item = el('dd');
    item.append(value);
    node.append(el('dt', key), item);
  }
  return node;
}

function renderValidation(report: Validation) {
  const list = el('ul', undefined, 'records');
  for (const check of report.checks) {
    const item = el('li');
    item.append(createStatusBadge(check.status), ' ', el('span', check.id, 'mono'), el('p', check.detail, 'muted'));
    list.append(item);
  }
  const container = el('div', undefined, 'stack compact');
  container.append(facts([
    ['Format', createStatusBadge(report.formatStatus)],
    ['Role', createStatusBadge(report.role.status)],
    ['Requirement', report.role.requirement],
    ['Present', report.role.present.join(', ') || 'None'],
    ['Missing', report.role.missing.join(', ') || 'None'],
    ['Units', report.units],
    ['Bytes read', el('span', report.bytesRead.toLocaleString(), 'mono')],
  ]), list, el('p', 'Not run', 'muted'));
  const omitted = el('ul', undefined, 'records mono');
  for (const item of report.notRun) omitted.append(el('li', item));
  container.append(omitted);
  return container;
}

export function inspectorPanel(host: HTMLElement, actions: {
  validate(role: Role): Promise<void>;
  feedback(request: FeedbackRequest): Promise<void>;
}) {
  const title = el('h2', 'Version inspector');
  const body = el('div');
  const notice = makeNotice();
  host.append(title, notice, body);

  let versionId: string | undefined;
  let validationOutput: HTMLElement | undefined;
  let validationNotice: HTMLElement | undefined;
  let history: HTMLElement | undefined;

  const renderHistory = (detail: VersionDetail) => {
    const timeline = el('ol', undefined, 'records');
    for (const record of detail.history) {
      const item = el('li');
      item.append(el('span', `#${record.sequence}`, 'mono'), ' ', createStatusBadge(record.decision),
        el('p', record.feedbackId, 'mono muted'));
      if (record.reason) item.append(el('p', record.reason));
      timeline.append(item);
    }
    history?.replaceChildren(timeline);
    if (!detail.history.length) history?.append(el('p', 'No feedback recorded.', 'muted'));
    if (detail.history.length === 20) history?.append(el('p', 'Showing the latest 20 records.', 'muted'));
  };

  return {
    selection(row: VersionRow | null) {
      versionId = undefined;
      validationOutput = undefined;
      validationNotice = undefined;
      history = undefined;
      body.replaceChildren();
      notice.textContent = row ? 'Loading version details…' : 'Select a version to inspect its metadata and history.';
    },
    error(message: string, isValidation: boolean) {
      if (isValidation && validationNotice) validationNotice.textContent = message;
      else notice.textContent = message;
    },
    validation(report: Validation) {
      if (validationNotice) validationNotice.textContent = 'Core validation returned a report.';
      validationOutput?.replaceChildren(renderValidation(report));
    },
    render(detail: VersionDetail) {
      notice.textContent = '';
      if (versionId === detail.versionId) {
        renderHistory(detail);
        return;
      }
      versionId = detail.versionId;
      const storage = section('Version and storage');
      const digest = el('div', undefined, 'stack compact');
      digest.append(el('span', detail.contentHash, 'mono'), button('Copy SHA-256', () => {
        void (async () => {
          try {
            await navigator.clipboard.writeText(detail.contentHash);
            if (versionId === detail.versionId) notice.textContent = 'SHA-256 copied.';
          } catch {
            if (versionId === detail.versionId) notice.textContent = 'Clipboard unavailable. Select the hash to copy it.';
          }
        })();
      }));
      storage.append(facts([
        ['Asset ID', detail.assetId],
        ['Version ID', el('span', detail.versionId, 'mono')],
        ['SHA-256', digest],
        ['File availability', createStatusBadge(detail.availability)],
        ['Byte length', el('span', detail.byteLength?.toLocaleString() ?? 'Unknown', 'mono')],
        ['Source class', detail.sourceClass],
        ['Source label', detail.source],
        ['License', detail.licenseId],
        ['Usage scope', detail.usageScope],
        ['Candidate head', detail.current ?
          `Revision ${detail.current.revision} · ${detail.current.contract.contract_id} · ${detail.current.version_id}` : 'None'],
      ]));

      const references = section('Component references');
      if (detail.references.length) references.append(facts(detail.references.map((ref) =>
        [ref.role, el('span', ref.componentVersionId, 'mono')])));
      else references.append(el('p', 'None declared', 'muted'));

      const validation = section('Core validation');
      const role = el('select');
      for (const [value, name] of [
        ['static-mesh', 'Static mesh'], ['rigged-character', 'Rigged character'], ['animated-character', 'Animated character'],
      ]) {
        const option = el('option', name);
        option.value = value;
        role.append(option);
      }
      const output = el('div');
      const progress = makeNotice();
      validationNotice = progress;
      validationOutput = output;
      const validate = button('Run Core validate', () => {
        output.replaceChildren();
        progress.textContent = 'Running Core validation…';
        void actions.validate(role.value as Role);
      });
      validate.disabled = detail.availability !== 'AVAILABLE';
      if (validate.disabled) progress.textContent = 'Validation requires available managed bytes.';
      validation.append(labelField('Required role', role), validate, progress, output);

      const feedback = section('Version feedback');
      const form = el('form', undefined, 'stack compact');
      const decision = el('select');
      for (const label of ['ACCEPT', 'REJECT']) decision.append(el('option', label));
      const reason = el('textarea');
      reason.rows = 2;
      reason.addEventListener('input', () => {
        const over = new TextEncoder().encode(reason.value.trim()).length > 2048;
        reason.setCustomValidity(over ? 'Use at most 2048 UTF-8 bytes.' : '');
      });
      const id = el('input');
      id.placeholder = 'Auto-generated if blank';
      const submit = el('button', 'Save decision', 'primary');
      submit.type = 'submit';
      const status = makeNotice();
      form.append(labelField('Decision', decision), labelField('Reason (optional)', reason),
        labelField('Feedback ID (optional)', id), submit, status);
      attachAccessibleFormValidation(form);
      form.addEventListener('submit', async (event) => {
        event.preventDefault();
        if (submit.disabled || !form.reportValidity()) return;
        submit.disabled = true;
        status.textContent = 'Saving decision…';
        try {
          await actions.feedback({
            assetId: detail.assetId, versionId: detail.versionId, decision: decision.value as Decision,
            feedbackId: id.value.trim() ? id.value : crypto.randomUUID(), reason: reason.value.trim() || null,
          });
          form.reset();
          status.textContent = 'Decision saved.';
        } catch (error) {
          status.textContent = errorMessage(error);
        } finally {
          submit.disabled = false;
        }
      });
      history = el('div');
      feedback.append(form, el('h3', 'Version history'), history);
      renderHistory(detail);
      body.replaceChildren(storage, references, validation, feedback);
    },
  };
}
