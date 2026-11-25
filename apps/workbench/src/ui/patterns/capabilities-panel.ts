import { errorMessage } from '../../controller';
import { CAPABILITY_MAX_BYTES } from '../../types';
import type { CapabilityOverview, CapabilityRunResult, CapabilitySpec, RunCapabilityRequest } from '../../types';
import { attachAccessibleFormValidation, button, createStatusBadge, el, integerInput, labelField, makeNotice } from '../primitives';

export function capabilitiesPanel(
  root: HTMLElement,
  run: (request: RunCapabilityRequest) => Promise<CapabilityRunResult>,
) {
  const notice = makeNotice();
  const list = el('div', undefined, 'stack capability-list');
  const detail = el('section', undefined, 'stack capability-detail');
  const grid = el('div', undefined, 'capability-grid');
  grid.append(list, detail);
  root.append(el('h2', 'Capabilities'), notice, grid);
  notice.hidden = true;
  grid.hidden = true;

  function show(spec: CapabilitySpec) {
    const heading = el('div', undefined, 'capability-heading');
    heading.append(el('h3', spec.label), createStatusBadge(spec.status));
    detail.replaceChildren(heading);
    if (spec.taskCode) detail.append(el('p', spec.taskCode, 'muted mono'));

    const runButton = el('button', 'Run inspection', 'primary');
    runButton.type = 'submit';
    if (!spec.implemented) {
      runButton.disabled = true;
      detail.append(el('p', spec.reason ?? ''), el('p', 'No parameters available while the runtime is NOT_RUN.', 'muted'), runButton);
      return;
    }

    const form = el('form', undefined, 'stack');
    const fields = el('fieldset', undefined, 'stack capability-fields');
    const primary = el('input');
    primary.required = true;
    const secondary = el('input');
    secondary.required = spec.secondaryRequired;
    secondary.disabled = !spec.allowedParams.includes('secondaryPath');
    const secondaryField = labelField(
      spec.secondaryRequired ? spec.secondaryLabel! : `${spec.secondaryLabel ?? 'Secondary path'} (optional)`,
      secondary,
    );
    secondaryField.hidden = secondary.disabled;
    const bytes = integerInput(1, CAPABILITY_MAX_BYTES, CAPABILITY_MAX_BYTES);
    fields.append(labelField(spec.primaryLabel!, primary), secondaryField, labelField('Maximum bytes per file', bytes), runButton);
    const status = makeNotice();
    const result = el('div', undefined, 'stack');
    form.append(fields, status, result);
    const boundaries = el('ul', undefined, 'muted capability-boundaries');
    boundaries.append(...spec.notRun.map((boundary) => el('li', boundary)));
    detail.append(boundaries, form);
    attachAccessibleFormValidation(form);

    fields.addEventListener('input', () => {
      result.replaceChildren();
      status.textContent = '';
    });

    form.addEventListener('submit', async (event) => {
      event.preventDefault();
      if (fields.disabled) return;
      const request: RunCapabilityRequest = {
        capabilityId: spec.id,
        primaryPath: primary.value,
        maxBytes: bytes.valueAsNumber,
      };
      if (!secondary.disabled && secondary.value.trim()) request.secondaryPath = secondary.value;
      fields.disabled = true;
      result.replaceChildren();
      status.textContent = 'Running local inspection…';
      try {
        const receipt = await run(request);
        if (!form.isConnected) return;
        status.textContent = `Inspection ${receipt.status}`;
        result.append(createStatusBadge(receipt.status));
        if (receipt.violations.length) {
          const violations = el('ul');
          violations.append(...receipt.violations.map((violation) => el('li', violation)));
          result.append(violations);
        }
        if (receipt.digest) result.append(el('p', `Digest ${receipt.digest}`, 'mono muted'));
        if (receipt.report !== null) {
          const report = el('details');
          report.append(el('summary', 'Inspection report'), el('pre', JSON.stringify(receipt.report, null, 2), 'capability-report mono'));
          result.append(report);
        }
      } catch (error) {
        if (form.isConnected) status.textContent = errorMessage(error);
      } finally {
        fields.disabled = false;
      }
    });
  }

  return {
    render(overview: CapabilityOverview | null) {
      notice.hidden = !!overview;
      notice.textContent = overview ? '' : 'Loading capabilities…';
      grid.hidden = !overview;
      list.replaceChildren();
      detail.replaceChildren();
      if (!overview) return;

      const choices: HTMLButtonElement[] = [];
      for (const [title, specs] of [
        ['Implemented', overview.implemented],
        ['Unimplemented', overview.unimplemented],
      ] as const) {
        const group = el('section', undefined, 'stack compact');
        const heading = el('h3', title);
        const entries = el('ul', undefined, 'records');
        for (const spec of specs) {
          const row = el('li');
          const choice = button(spec.label, () => {
            for (const other of choices) other.setAttribute('aria-pressed', String(other === choice));
            show(spec);
          });
          choice.className = 'capability-choice';
          choice.setAttribute('aria-pressed', 'false');
          choice.append(createStatusBadge(spec.status));
          choices.push(choice);
          row.append(choice);
          entries.append(row);
        }
        group.append(heading, entries);
        list.append(group);
      }
      choices[0]?.click();
    },
    error(message: string) {
      notice.hidden = false;
      notice.textContent = message;
    },
  };
}
