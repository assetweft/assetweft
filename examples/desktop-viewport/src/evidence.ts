type Invoke = (command: string, args: Record<string, unknown>) => Promise<{status: 'saved' | 'stale-rejected'}>;

export async function saveEvidence(
    generation: number,
    observations: object,
    canvas: Pick<HTMLCanvasElement, 'toDataURL'>,
    invoke: Invoke,
    isCurrent: () => boolean,
) {
    if (!isCurrent()) return;
    const prefix = 'data:image/png;base64,';
    const snapshot = canvas.toDataURL('image/png');
    if (!snapshot.startsWith(prefix)) throw Error('PNG export failed');

    const result = await invoke('save_evidence', {
        generation,
        json: JSON.stringify({...observations, generation}, null, 2),
        png: Array.from(atob(snapshot.slice(prefix.length)), byte => byte.charCodeAt(0)),
    });
    if (!isCurrent()) return;
    if (result.status !== 'saved') throw Error('Evidence generation rejected');
}
