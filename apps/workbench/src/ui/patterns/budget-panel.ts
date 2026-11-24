import type {
  Approval, ApproveBudgetRequest, BudgetAccount, BudgetIntent, BudgetOverview, BudgetUnit,
  ByoGpuRun, GpuProfile, ReconcileCallRequest, ReserveCallRequest, Resolution, RunByoGpuRequest, Unit,
} from '../../types';
import { button, createStatusBadge, el, facts, integerInput, labelField, makeNotice, selectInput } from '../primitives';
import { actionDialog, textInput } from './dialogs';

interface BudgetActions {
  approve(req: ApproveBudgetRequest): Promise<void>;
  reserve(req: ReserveCallRequest): Promise<void>;
  cancel(id: string): Promise<void>;
  reconcile(req: ReconcileCallRequest): Promise<void>;
  run(req: RunByoGpuRequest): Promise<void>;
}

const unitLabel = (unit: BudgetUnit) => unit.label === 'CurrencyMinor'
  ? `${unit.currency} minor units (scale ${unit.decimals})` : unit.label;
const requestUnit = (unit: BudgetUnit): Unit => unit.label === 'CurrencyMinor'
  ? { CurrencyMinor: { currency: unit.currency!, decimals: unit.decimals! } } : unit.label;

function approveDialog(approve: BudgetActions['approve']) {
  actionDialog('Approve budget', (form) => {
    const id = textInput();
    const source = textInput();
    const unit = selectInput(['Task', 'Token', 'Second', 'CurrencyMinor']);
    const approval = selectInput(['ExplicitBounded', 'ZeroAdditionalCost']);
    const capacity = integerInput(0);
    const currency = textInput();
    currency.pattern = '[A-Z]{3}';
    const decimals = integerInput(0, 9);
    const currencyField = labelField('Currency code', currency);
    const decimalsField = labelField('Minor-unit decimal places', decimals);
    const syncUnit = () => {
      const isCurrency = unit.value === 'CurrencyMinor';
      currencyField.hidden = decimalsField.hidden = !isCurrency;
      currency.disabled = decimals.disabled = !isCurrency;
    };
    syncUnit();
    unit.addEventListener('change', syncUnit);
    form.append(labelField('Account ID', id), labelField('Unit', unit), currencyField, decimalsField,
      labelField('Approval', approval), labelField('Capacity', capacity), labelField('Allowance source', source));
    return () => approve({
      id: id.value, source: source.value, capacity: capacity.valueAsNumber, approval: approval.value as Approval,
      unit: unit.value === 'CurrencyMinor'
        ? { CurrencyMinor: { currency: currency.value, decimals: decimals.valueAsNumber } }
        : unit.value as 'Task' | 'Token' | 'Second',
    });
  });
}

function reserveDialog(accounts: BudgetAccount[], reserve: BudgetActions['reserve']) {
  actionDialog('Reserve call', (form) => {
    const account = selectInput(accounts.map((row) => row.id));
    const id = textInput();
    const fingerprint = textInput();
    const source = textInput();
    const quote = integerInput(0);
    quote.required = false;
    const reservation = integerInput(0);
    form.append(labelField('Account ID', account), labelField('Intent ID', id),
      labelField('Input fingerprint', fingerprint), labelField('Quote source', source),
      labelField('Quote (blank means Unknown)', quote), labelField('Reservation', reservation));
    return () => reserve({
      id: id.value, account: account.value, inputFingerprint: fingerprint.value, source: source.value,
      unit: requestUnit(accounts[account.selectedIndex].unit),
      quote: quote.value === '' ? null : quote.valueAsNumber, reservation: reservation.valueAsNumber,
    });
  });
}

function reconcileDialog(intent: BudgetIntent, reconcile: BudgetActions['reconcile']) {
  actionDialog('Reconcile receipt', (form) => {
    const receiptId = textInput();
    const source = textInput();
    const resolution = selectInput(['Unresolved', 'Confirmed', 'AuthoritativeNoCharge']);
    const charge = integerInput(0);
    const chargeField = labelField('Confirmed charge', charge);
    const syncResolution = () => {
      chargeField.hidden = charge.disabled = resolution.value !== 'Confirmed';
    };
    syncResolution();
    resolution.addEventListener('change', syncResolution);
    form.append(el('p', `Intent ${intent.id}`, 'mono'), labelField('Receipt ID', receiptId),
      labelField('Provider evidence source', source), labelField('Resolution', resolution), chargeField);
    return () => reconcile({
      intentId: intent.id, receiptId: receiptId.value, source: source.value,
      resolution: resolution.value === 'Confirmed'
        ? { Confirmed: { charge: charge.valueAsNumber } } : resolution.value as Resolution,
    });
  });
}

function runDialog(profiles: GpuProfile[], run: BudgetActions['run']) {
  actionDialog('Run BYO GPU', (form) => {
    const profile = selectInput(profiles.map((entry) => entry.id));
    const model = el('select');
    const syncModels = () => {
      model.replaceChildren(...profiles[profile.selectedIndex].models.map((entry) => el('option', entry.id)));
    };
    syncModels();
    profile.addEventListener('change', syncModels);
    const budget = textInput();
    const intent = textInput();
    const outDir = textInput();
    const image = textInput(false);
    const bundle = textInput(false);
    const prompt = el('textarea');
    const adapter = textInput(false);
    const maxBytes = integerInput(1, 16_777_216, 16_777_216);
    const allow = el('input');
    allow.type = 'checkbox';
    allow.checked = false;
    const allowLabel = el('label', undefined, 'checkbox');
    allowLabel.append(allow, el('span', 'Allow remote upload of local input file (required only when sending a local image/bundle)'));
    form.append(el('p', 'Runs your configured local executable. It can make remote requests using your configured credentials.'),
      labelField('Task budget ID', budget), labelField('Intent ID', intent), labelField('Profile', profile),
      labelField('Model', model), labelField('Output directory', outDir), labelField('Local image path', image),
      labelField('Private Kaggle bundle path', bundle), labelField('Prompt', prompt),
      labelField('Trusted adapter executable (optional with configured default)', adapter),
      labelField('File byte limit', maxBytes), allowLabel);
    return () => run({
      budgetId: budget.value, intentId: intent.value, profile: profile.value, model: model.value,
      outDir: outDir.value, image: image.value || null, bundle: bundle.value || null, prompt: prompt.value || null,
      adapterBin: adapter.value || null, allowRemoteUpload: allow.checked, maxBytes: maxBytes.valueAsNumber,
    });
  });
}

function accountTable(accounts: BudgetAccount[]) {
  if (!accounts.length) return el('p', 'No budget accounts.', 'muted');
  const table = el('table', undefined, 'workspace-table');
  table.setAttribute('aria-label', 'Budget Accounts');
  const headers = table.createTHead().insertRow();
  for (const name of ['Account', 'Unit', 'Approval', 'Capacity', 'Reserved', 'Spent', 'Available']) {
    const header = el('th', name);
    header.scope = 'col';
    headers.append(header);
  }
  const body = table.createTBody();
  for (const account of accounts) {
    const row = body.insertRow();
    const header = el('th', account.id, 'mono');
    header.scope = 'row';
    header.append(el('p', account.source, 'muted'));
    row.append(header);
    row.insertCell().textContent = unitLabel(account.unit);
    row.insertCell().append(createStatusBadge(account.approval));
    for (const count of [account.capacity, account.reserved, account.spent, account.available]) {
      const cell = row.insertCell();
      cell.className = 'mono quantity';
      cell.textContent = String(count);
    }
  }
  const scroll = el('div', undefined, 'table-scroll');
  scroll.append(table);
  return scroll;
}

function intentRow(intent: BudgetIntent, actions: BudgetActions) {
  const row = el('li', undefined, 'stack compact');
  const heading = el('div', undefined, 'section-bar');
  heading.append(el('h3', intent.id, 'mono'), createStatusBadge(intent.state));
  row.append(heading, facts([
    ['Account', intent.account], ['Unit', unitLabel(intent.unit)], ['Quote', intent.quote ?? 'Unknown'],
    ['Reservation', intent.reservation], ['Charge', intent.charge ?? 'Unknown'],
    ['Cancelled locally', intent.cancelled ? 'Yes' : 'No'],
  ]));
  if (intent.warning) row.append(el('p', intent.warning, 'workspace-warning'));
  const receipts = el('details');
  receipts.append(el('summary', `Receipts (${intent.receipts.length})`));
  const list = el('ul', undefined, 'records');
  for (const receipt of intent.receipts) {
    const resolution = typeof receipt.resolution === 'string'
      ? receipt.resolution : `Confirmed · Charge ${receipt.resolution.Confirmed.charge}`;
    const item = el('li', `${receipt.id} · ${resolution} · ${receipt.source}`);
    for (const usage of receipt.usage) {
      item.append(el('p', `${unitLabel(usage.unit)} · ${usage.amount ?? 'Unknown'} · ${usage.source}`, 'muted'));
    }
    list.append(item);
  }
  receipts.append(list);
  row.append(receipts);
  const buttons = el('div', undefined, 'actions');
  if (!intent.cancelled && ['PREPARING', 'ATTEMPTING', 'UNKNOWN'].includes(intent.state)) {
    buttons.append(button('Local cancel', () => actionDialog('Local cancel', (form) => {
      form.append(el('p', `Record a local cancel for ${intent.id}. This action makes no claim of remote stop or refund.`));
      return () => actions.cancel(intent.id);
    })));
  }
  if (['ATTEMPTING', 'UNKNOWN'].includes(intent.state)) {
    buttons.append(button('Reconcile receipt', () => reconcileDialog(intent, actions.reconcile)));
  }
  if (buttons.childElementCount) row.append(buttons);
  return row;
}

function profileDetails(profile: GpuProfile) {
  const profileNode = el('li', undefined, 'stack compact');
  profileNode.append(el('h3', profile.id, 'mono'), el('p', profile.external_runtime),
    facts([
      ['Cash cost', 'Unknown'],
      ['Credential settings', [...profile.auth_env_vars, ...profile.auth_config].join(', ')],
      ...(profile.accelerators ? [['Accelerators', profile.accelerators.join(', ')] as [string, string]] : []),
      ...(profile.is_private ? [['Kernel visibility', 'Private'] as [string, string]] : []),
    ]));
  const models = el('ul');
  for (const model of profile.models) models.append(el('li',
    [model.id, model.upstream, model.license, model.artifact, model.notes].filter(Boolean).join(' · ')));
  profileNode.append(models);
  return profileNode;
}

export function budgetPanel(target: HTMLElement, actions: BudgetActions) {
  const status = makeNotice();
  const receipt = makeNotice();
  const result = el('div', undefined, 'stack compact');
  result.setAttribute('aria-live', 'polite');
  const content = el('div', undefined, 'stack');
  target.append(el('h2', 'Generation & Budgets'), status, receipt, result, content);

  return {
    render(value: BudgetOverview | null) {
      content.replaceChildren();
      status.textContent = '';
      if (!value) return;
      const accounts = el('section', undefined, 'workspace-card stack');
      const accountBar = el('div', undefined, 'section-bar');
      accountBar.append(el('h3', 'Budget Accounts'), button('Approve budget', () => approveDialog(actions.approve)));
      accounts.append(accountBar, accountTable(value.accounts));
      const intents = el('section', undefined, 'workspace-card stack');
      const intentBar = el('div', undefined, 'section-bar');
      const reserve = button('Reserve call', () => reserveDialog(value.accounts, actions.reserve));
      reserve.disabled = !value.accounts.length;
      intentBar.append(el('h3', 'Call Intents & Remote UNKNOWN'), reserve);
      intents.append(intentBar);
      const list = el('ul', undefined, 'records');
      list.append(...value.intents.map((intent) => intentRow(intent, actions)));
      intents.append(value.intents.length ? list : el('p', 'No call intents.', 'muted'));
      const runner = el('section', undefined, 'workspace-card stack');
      const runnerBar = el('div', undefined, 'section-bar');
      runnerBar.append(el('h3', 'BYO GPU Profiles & Local Opt-In Runner'),
        button('Run BYO GPU', () => runDialog(value.profiles, actions.run)));
      const profiles = el('ul', undefined, 'records');
      profiles.append(...value.profiles.map(profileDetails));
      runner.append(runnerBar, el('p', 'No paid provider calls run automatically; settled intents replay with invoked=false.'), profiles);
      content.append(accounts, intents, runner);
    },
    error(message: string) { status.textContent = message; },
    receipt(value: BudgetAccount | BudgetIntent | null) {
      receipt.textContent = value ? ('state' in value
        ? [value.id, value.state, value.warning].filter(Boolean).join(' · ')
        : `Allowance ${value.id} recorded.`) : '';
    },
    run(value: ByoGpuRun | null) {
      result.replaceChildren();
      if (!value) return;
      result.append(createStatusBadge(value.status), el('p', value.message),
        facts([['Intent', value.intentId], ['Profile / model', `${value.profile} / ${value.model}`],
          ['Runner invoked', String(value.invoked)], ['Cash cost', value.cashCost ?? 'Unknown'],
          ['Artifact', value.artifactFile ?? '—'], ['SHA-256', value.artifactSha256 ?? '—']]));
    },
  };
}
