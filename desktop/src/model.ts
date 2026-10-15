export type Paths = [string, string];

export function pollStatus<T>(read: () => Promise<T>, receive: (status: T) => void, fail: (error: unknown) => void, schedule: (callback: () => void) => () => void = callback => {
  const timer = setTimeout(callback, 250);
  return () => clearTimeout(timer);
}): () => void {
  let stopped = false;
  let cancel: (() => void) | undefined;
  async function tick() {
    try {
      const status = await read();
      if (stopped) return;
      receive(status);
      cancel = schedule(() => { void tick(); });
    } catch (error) {
      if (!stopped) fail(error);
      stopped = true;
    }
  }
  void tick();
  return () => { stopped = true; cancel?.(); };
}

export type NativeStatus = { details?: boolean; paths: string[]; duration: number; time: number; paused: boolean; mode: 'side' | 'slider' | 'blend' | 'diff'; amount: number; audio: 'A' | 'B' | 'mute'; volume: number; running: boolean };

export function formatTime(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) return '—';
  const ms = Math.floor(seconds * 1000);
  const hours = Math.floor(ms / 3600000);
  const minutes = Math.floor(ms / 60000) % 60;
  const second = Math.floor(ms / 1000) % 60;
  return `${hours ? hours + ':' : ''}${String(minutes).padStart(2, '0')}:${String(second).padStart(2, '0')}.${String(ms % 1000).padStart(3, '0')}`;
}

type Box = { left: number; top: number; right: number; bottom: number };

export function dropTarget(position: { x: number; y: number }, scale: number, boxes: Box[]): 0 | 1 | null {
  if (!Number.isFinite(scale) || scale <= 0) throw new Error('Cannot determine window scale for file drop.');
  const x = position.x / scale;
  const y = position.y / scale;
  const index = boxes.findIndex(box => x >= box.left && x <= box.right && y >= box.top && y <= box.bottom);
  return index === 0 || index === 1 ? index : null;
}

export function selectPaths(current: Paths, selected: string[], target: 0 | 1): Paths {
  if (selected.length > 2) throw new Error('Select at most two video files.');
  if (!selected.length) return current;
  if (selected.some(path => typeof path !== 'string' || !path.trim())) throw new Error('A source path cannot be empty.');
  if (selected.length === 2) return [selected[0], selected[1]];
  const next: Paths = [...current];
  next[target] = selected[0];
  return next;
}
