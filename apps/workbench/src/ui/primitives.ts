export function el<K extends keyof HTMLElementTagNameMap>(tag: K, text?: string, className?: string): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (text !== undefined) node.textContent = text;
  if (className) node.className = className;
  return node;
}

export function button(text: string, action: () => void) {
  const node = el('button', text);
  node.type = 'button';
  node.addEventListener('click', action);
  return node;
}

export function createStatusBadge(status: string) {
  const badge = el('span', status, 'badge');
  if (['PASS', 'ACCEPT', 'AVAILABLE', 'READY'].includes(status)) badge.dataset.tone = 'pass';
  if (['FAIL', 'REJECT', 'MISSING', 'CORRUPT', 'ERROR'].includes(status)) badge.dataset.tone = 'fail';
  if (['WARN', 'PARTIAL', 'METADATA_ONLY'].includes(status)) badge.dataset.tone = 'warn';
  return badge;
}

type Field = HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement;
let fieldId = 0;

export function labelField(labelText: string, input: Field) {
  const field = el('div', undefined, 'field');
  const label = el('label', labelText);
  const error = el('span', '', 'field-error');
  input.id = `field-${++fieldId}`;
  label.htmlFor = input.id;
  error.id = `${input.id}-error`;
  error.hidden = true;
  input.dataset.error = error.id;
  field.append(label, input, error);
  return field;
}

export function attachAccessibleFormValidation(form: HTMLFormElement) {
  const fields = Array.from(form.querySelectorAll<Field>('input, select, textarea'));
  const live = document.getElementById('field-announcement');
  const supportsUserInvalid = CSS.supports('selector(:user-invalid)');
  const touched = new Set<Field>();
  const sync = (input: Field, force = false) => {
    const old = input.getAttribute('aria-invalid');
    const invalid = !input.validity.valid && (force ||
      (supportsUserInvalid ? input.matches(':user-invalid') : touched.has(input)));
    input.setAttribute('aria-invalid', String(invalid));
    const error = input.dataset.error ? document.getElementById(input.dataset.error) : null;
    if (!error) return;
    const message = invalid ? input.validationMessage : '';
    const changed = old !== 'true' || error.textContent !== message;
    const prefix = `${input.labels?.[0]?.textContent}: `;
    error.hidden = !invalid;
    error.textContent = message;
    if (invalid) {
      input.setAttribute('aria-describedby', error.id);
      if (changed && live) live.textContent = prefix + message;
    } else {
      input.removeAttribute('aria-describedby');
      if (old === 'true' && live?.textContent?.startsWith(prefix)) live.textContent = '';
    }
  };

  for (const input of fields) {
    input.setAttribute('aria-invalid', 'false');
    for (const event of ['blur', 'input', 'change', 'invalid']) {
      input.addEventListener(event, () => {
        if (event === 'blur' || event === 'invalid') touched.add(input);
        sync(input, event === 'invalid');
      });
    }
  }
  form.addEventListener('reset', () => {
    touched.clear();
    if (live) live.textContent = '';
    requestAnimationFrame(() => {
      for (const input of fields) {
        input.setCustomValidity('');
        sync(input);
      }
    });
  });
}

export function makeNotice(text = '') {
  const node = el('p', text, 'muted');
  node.setAttribute('role', 'status');
  return node;
}
