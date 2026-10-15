import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { selectPaths, dropTarget, formatTime, pollStatus } from './model.ts';

it('polls only after a read finishes and stops on error without a retry flood', async () => {
  const tasks: (() => void)[] = [];
  let calls = 0;
  let resolve!: (value: number) => void;
  const results: number[] = [];
  const failures: unknown[] = [];
  const stop = pollStatus(() => {
    calls++;
    return calls === 1 ? new Promise<number>(done => { resolve = done; }) : Promise.reject('mpv disconnected');
  }, value => results.push(value), error => failures.push(error), callback => { tasks.push(callback); return () => {}; });
  assert.equal(calls, 1);
  assert.equal(tasks.length, 0);
  resolve(42);
  await new Promise(done => setImmediate(done));
  assert.deepEqual(results, [42]);
  assert.equal(tasks.length, 1);
  tasks.shift()!();
  await new Promise(done => setImmediate(done));
  assert.equal(calls, 2);
  assert.deepEqual(failures, ['mpv disconnected']);
  assert.equal(tasks.length, 0);
  stop();
});

it('formats time without rounding into a nonexistent 60th second', () => {
  assert.equal(formatTime(59.999), '00:59.999');
  assert.equal(formatTime(3661.25), '1:01:01.250');
  assert.equal(formatTime(NaN), '—');
});

it('maps native physical drop coordinates to the correct CSS slot at 2× scale', () => {
  const boxes = [
    { left: 20, top: 40, right: 300, bottom: 110 },
    { left: 320, top: 40, right: 600, bottom: 110 },
  ];
  assert.equal(dropTarget({ x: 900, y: 140 }, 2, boxes), 1);
  assert.equal(dropTarget({ x: 100, y: 140 }, 2, boxes), 0);
  assert.equal(dropTarget({ x: 900, y: 500 }, 2, boxes), null);
});

describe('local source selection', () => {
  it('replaces only the target for one file and preserves literal Unicode/Windows paths', () => {
    assert.deepEqual(selectPaths(['/unchanged.mov', '/old.mov'], ['C:\\素材\\video 01.mkv'], 1), ['/unchanged.mov', 'C:\\素材\\video 01.mkv']);
    assert.deepEqual(selectPaths(['/old.mov', '/unchanged.mov'], ['/new.mov'], 0), ['/new.mov', '/unchanged.mov']);
  });
  it('keeps both sources unchanged when the native picker is cancelled', () => {
    const original: [string, string] = ['/a.mov', '/b.mov'];
    assert.equal(selectPaths(original, [], 0), original);
  });
  it('rejects excess or empty selections without mutating the originals', () => {
    const original: [string, string] = ['/a.mov', '/b.mov'];
    assert.throws(() => selectPaths(original, ['/1', '/2', '/3'], 0), /at most two/);
    assert.throws(() => selectPaths(original, ['  '], 0), /empty/);
    assert.deepEqual(original, ['/a.mov', '/b.mov']);
  });
  it('fills A/B in order for a two-file selection, regardless of target', () => {
    assert.deepEqual(selectPaths(['old A', 'old B'], ['/source.mov', '/upscaled.mkv'], 1), ['/source.mov', '/upscaled.mkv']);
  });
});
