import type { VersionList, VersionRow } from '../../types';
import { button, createStatusBadge, el, makeNotice } from '../primitives';

export function assetList(host: HTMLElement, callbacks: {
  search(query: string): void;
  select(row: VersionRow): void;
  importFile(): void;
}) {
  const heading = el('h2', 'Catalog');
  const search = el('input');
  search.type = 'search';
  search.placeholder = 'Asset or version ID';
  search.setAttribute('aria-label', 'Search asset or version ID');
  search.disabled = true;
  search.addEventListener('input', () => callbacks.search(search.value));
  const notice = makeNotice();
  const list = el('ul', undefined, 'version-list');
  const emptyAction = button('Import local file', callbacks.importFile);
  const header = el('div', undefined, 'stack compact');
  const empty = el('div');
  empty.append(emptyAction);
  empty.hidden = true;
  header.append(heading, search, notice, empty);
  host.append(header, list);

  let selected: string | undefined;
  let rows: { id: string; node: HTMLButtonElement }[] = [];

  function select(id: string | undefined) {
    selected = id;
    for (const row of rows) {
      if (row.id === selected) row.node.setAttribute('aria-current', 'true');
      else row.node.removeAttribute('aria-current');
    }
  }

  return {
    session(active: boolean) {
      search.disabled = !active;
      search.value = '';
    },
    select,
    error(message: string) {
      notice.textContent = message;
      list.replaceChildren();
      empty.hidden = true;
      rows = [];
    },
    render(result: VersionList | null, query: string) {
      if (search.value !== query) search.value = query;
      const focused = rows.find((row) => row.node === document.activeElement)?.id;
      empty.hidden = !result || result.matches.length > 0 || Boolean(query.trim());
      notice.textContent = !result ? (search.disabled ? 'Open a local library to view its catalog.' : 'Loading catalog…')
        : result.truncated ? 'Showing the first 100 versions. Narrow your search to see more.'
        : result.matches.length ? `${result.matches.length} versions`
        : query.trim() ? 'No versions match this search.' : 'No assets indexed yet.';
      rows = [];
      list.replaceChildren(...(result?.matches ?? []).map((row) => {
        const item = el('li');
        const control = button('', () => callbacks.select(row));
        control.className = 'version-row';
        const badges = el('span', undefined, 'actions');
        badges.append(createStatusBadge(row.availability), createStatusBadge(row.latestDecision));
        control.append(el('strong', row.assetId), el('span', row.versionId, 'mono muted'),
          el('span', row.sourceFormat, 'muted'), badges);
        rows.push({ id: row.versionId, node: control });
        item.append(control);
        return item;
      }));
      select(selected);
      if (focused) rows.find((row) => row.id === focused)?.node.focus();
    },
  };
}
