import assert from 'node:assert/strict';
import {test} from 'node:test';
import {saveEvidence} from './src/evidence.ts';

for (const next of ['empty', 'malformed', 'fixture B']) {
    test(`delayed Fixture A save while switching to ${next}`, async () => {
        let active = 1;
        let frame = 'fixture A';
        let persisted;
        let firstSnapshot;
        let staleReplies = 0;
        const canvas = {
            toBlob: done => done(new Blob([frame])),
            toDataURL: () => `data:image/png;base64,${btoa(frame)}`,
        };
        const {promise: release, resolve} = Promise.withResolvers();

        async function invoke(command, args) {
            if (command === 'observations') {
                const observation = JSON.parse(args.json);
                if (observation.caseName === 'fixture A') await release;
                persisted = {observation};
                return;
            }
            if (command === 'capture') {
                persisted.png = Array.from(args);
                return;
            }
            assert.equal(command, 'save_evidence');
            if (args.generation === 1) {
                firstSnapshot = Array.from(args.png);
                await release;
            }
            if (args.generation !== active) {
                staleReplies++;
                return {status: 'stale-rejected'};
            }
            persisted = {observation: JSON.parse(args.json), png: args.png};
            return {status: 'saved'};
        }

        const save = (generation, caseName) => saveEvidence(
            generation, {caseName}, canvas, invoke, () => active === generation,
        );
        const old = save(1, 'fixture A');
        active = 2;
        frame = '';
        persisted = undefined;
        if (next === 'fixture B') {
            frame = next;
            await save(2, next);
        }
        resolve();
        await old;

        const encode = value => Array.from(new TextEncoder().encode(value));
        assert.deepEqual(persisted, next === 'fixture B' ? {
            observation: {caseName: next, generation: 2}, png: encode(next),
        } : undefined);
        assert.deepEqual(firstSnapshot, encode('fixture A'));
        assert.equal(staleReplies, 1);
    });
}
