import type { ImportRequest, LibrarySession } from '../../types';
import { errorMessage } from '../../controller';
import { attachAccessibleFormValidation, button, createStatusBadge, el, labelField, makeNotice } from '../primitives';

function modal(title: string, id: string) {
  const dialog = el('dialog');
  dialog.setAttribute('closedby', 'any');
  const heading = el('h2', title);
  heading.id = id;
  dialog.setAttribute('aria-labelledby', heading.id);
  const bar = el('div', undefined, 'dialog-title');
  const close = button('Close', () => {
    if (dialog.getAttribute('closedby') === 'any') dialog.close();
  });
  bar.append(heading, close);
  dialog.append(bar);
  document.body.append(dialog);
  const live = document.getElementById('field-announcement');
  dialog.addEventListener('close', () => {
    dialog.returnValue = '';
    if (live?.parentElement === dialog) {
      live.textContent = '';
      document.body.append(live);
    }
  });
  dialog.addEventListener('cancel', (event) => {
    if (dialog.getAttribute('closedby') === 'none') event.preventDefault();
  });
  if (!('closedBy' in HTMLDialogElement.prototype)) {
    const outside = (event: PointerEvent) => {
      const rect = dialog.getBoundingClientRect();
      return event.target === dialog && (event.clientX < rect.left || event.clientX > rect.right ||
        event.clientY < rect.top || event.clientY > rect.bottom);
    };
    let startedOutside = false;
    dialog.addEventListener('pointerdown', (event) => { startedOutside = outside(event); });
    dialog.addEventListener('pointerup', (event) => {
      if (startedOutside && outside(event) && dialog.getAttribute('closedby') === 'any') dialog.close();
      startedOutside = false;
    });
  }
  return {
    dialog,
    show() {
      dialog.returnValue = '';
      if (live) dialog.append(live);
      dialog.showModal();
    },
  };
}

export function textInput(required = true) {
  const input = el('input');
  input.required = required;
  return input;
}

function submitForm(form: HTMLFormElement, text: string, run: () => Promise<void>, dialog: HTMLDialogElement) {
  const notice = makeNotice();
  const submit = el('button', text, 'primary');
  submit.type = 'submit';
  form.append(notice, submit);
  attachAccessibleFormValidation(form);
  form.addEventListener('reset', () => { notice.textContent = ''; });
  form.addEventListener('submit', async (event) => {
    event.preventDefault();
    if (submit.disabled) return;
    if (!form.reportValidity()) return;
    submit.disabled = true;
    dialog.setAttribute('closedby', 'none');
    dialog.querySelector<HTMLButtonElement>('.dialog-title button')!.disabled = true;
    notice.textContent = 'Working…';
    try {
      await run();
      dialog.close();
    } catch (error) {
      notice.textContent = errorMessage(error);
    } finally {
      submit.disabled = false;
      dialog.setAttribute('closedby', 'any');
      dialog.querySelector<HTMLButtonElement>('.dialog-title button')!.disabled = false;
    }
  });
}

export function libraryDialogs(actions: {
  open(path: string, create: boolean): Promise<void>;
  importFile(request: ImportRequest): Promise<void>;
}) {
  const open = modal('Open local library', 'open-title');
  const openForm = el('form', undefined, 'stack');
  const path = textInput();
  path.autofocus = true;
  path.placeholder = '/path/to/library.sqlite';
  const create = el('input');
  create.type = 'checkbox';
  const createLabel = el('label', undefined, 'checkbox');
  createLabel.append(create, el('span', ' Create or initialize this library'));
  openForm.append(labelField('SQLite file path', path), createLabel);
  submitForm(openForm, 'Open library', () => actions.open(path.value, create.checked), open.dialog);
  open.dialog.append(openForm);

  const importModal = modal('Import local file', 'import-title');
  const importForm = el('form', undefined, 'stack');
  const fields = {
    filePath: ['Local file path', textInput()],
    assetId: ['Asset ID', textInput()],
    versionId: ['Version ID', textInput()],
    sourceFormat: ['Source format', textInput()],
    licenseId: ['Declared license', textInput()],
    usageScope: ['Declared usage scope', textInput()],
  } as const;
  fields.filePath[1].autofocus = true;
  fields.sourceFormat[1].placeholder = 'GLB, PNG, WAV, or another format';
  fields.licenseId[1].defaultValue = 'UNKNOWN';
  fields.usageScope[1].defaultValue = 'UNKNOWN';
  for (const [label, input] of Object.values(fields)) importForm.append(labelField(label, input));
  submitForm(importForm, 'Import file', async () => {
    await actions.importFile({
      filePath: fields.filePath[1].value,
      assetId: fields.assetId[1].value,
      versionId: fields.versionId[1].value,
      sourceFormat: fields.sourceFormat[1].value,
      licenseId: fields.licenseId[1].value,
      usageScope: fields.usageScope[1].value,
    });
  }, importModal.dialog);
  importModal.dialog.append(importForm);

  const scope = modal('Capabilities and scope', 'scope-title');
  const content = el('div', undefined, 'stack');
  scope.dialog.append(content);
  return {
    open: () => { openForm.reset(); open.show(); },
    importFile: () => { importForm.reset(); importModal.show(); },
    capabilities(session: LibrarySession | null) {
      content.replaceChildren();
      if (!session) content.append(el('p', 'Open a local library to see its capabilities.'));
      for (const capability of session?.capabilities ?? []) {
        const item = el('div', undefined, 'stack compact');
        item.append(el('h3', capability.id.replaceAll('_', ' ')), createStatusBadge(capability.status),
          el('p', capability.description, 'muted'));
        content.append(item);
      }
      scope.show();
    },
  };
}


let actionDialogId = 0;

export function actionDialog(title: string, build: (form: HTMLFormElement) => () => Promise<void>) {
  const trigger = document.activeElement;
  const modalView = modal(title, `action-title-${++actionDialogId}`);
  const form = el('form', undefined, 'stack');
  const run = build(form);
  modalView.dialog.append(form);
  submitForm(form, title, run, modalView.dialog);
  modalView.dialog.addEventListener('close', () => {
    modalView.dialog.remove();
    if (!trigger?.isConnected) document.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]')?.focus();
  }, { once: true });
  modalView.show();
}
