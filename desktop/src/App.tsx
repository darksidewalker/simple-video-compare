import { useEffect, useRef, useState } from 'react';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { dropTarget, formatTime, pollStatus, selectPaths } from './model';
import type { NativeStatus, Paths } from './model';
import RuntimeSetup from './RuntimeSetup';
import { BackendDetails, useBackendCheck } from './BackendStatus';


const message = (error: unknown) => error instanceof Error ? error.message : String(error);
const modes = [['side', 'Side'], ['slider', 'Slider'], ['blend', 'Blend'], ['diff', 'Diff']] as const;

function Range({ label, value, max, step, disabled, commit }: { label: string; value: number; max: number; step: number; disabled: boolean; commit: (value: number) => void }) {
  const [draft, setDraft] = useState(value);
  const dirty = useRef(false);
  useEffect(() => { if (!dirty.current) setDraft(value); }, [value]);
  function finish() {
    if (!dirty.current) return;
    dirty.current = false;
  }
  return <label className="range">{label}<input aria-label={label} type="range" min="0" max={max} step={step} value={draft} disabled={disabled} onChange={event => { dirty.current = true; const next = Number(event.target.value); setDraft(next); commit(next); }} onPointerUp={finish} onPointerCancel={finish} onKeyUp={finish} onBlur={finish} /></label>;
}

export default function App() {
  const desktop = isTauri();
  const embedded = desktop && /Linux|Windows/.test(navigator.userAgent);
  useEffect(() => { document.documentElement.classList.toggle('embedded-native', embedded); }, [embedded]);
  const [paths, setPaths] = useState<Paths>(['', '']);
  const pathsRef = useRef(paths);
  const slots = useRef<(HTMLElement | null)[]>([]);
  const [status, setStatus] = useState<NativeStatus | null>(null);
  const [error, setError] = useState('');
  const [operationBusy, setBusy] = useState(false);
  const [runtimeBusy, setRuntimeBusy] = useState(false);
  const runtimeBusyRef = useRef(false);
  const backend = useBackendCheck(desktop, runtimeBusy);
  const busy = operationBusy || runtimeBusy;
  const busyRef = useRef(false);
  const [rangeBusy, setRangeBusy] = useState(false);
  const pendingRanges = useRef(new Map<string, number>());
  const statusRequest = useRef<Promise<NativeStatus> | null>(null);
  const epoch = useRef(0);
  const [pollFailed, setPollFailed] = useState(false);
  const [retry, setRetry] = useState(0);
  const details = status?.details ?? false;
  const [runtime, setRuntime] = useState(false);
  const [hidden, setHidden] = useState(false);
  const [hover, setHover] = useState<0 | 1 | null>(null);

  const running = !!status?.running && !pollFailed;

  function editPaths(next: Paths) { pathsRef.current = next; setPaths(next); }

  useEffect(() => {
    if (!desktop) return;
    return pollStatus(async () => {
      const version = epoch.current;
      if (busyRef.current) return null;
      const request = invoke<NativeStatus>('native_status');
      statusRequest.current = request;
      try { return { version, status: await request }; }
      catch (failure) { if (version === epoch.current) throw failure; return null; }
      finally { if (statusRequest.current === request) statusRequest.current = null; }
    }, result => {
      if (result && result.version === epoch.current && !busyRef.current) setStatus(result.status);
    }, failure => {
      setError(`Status polling stopped: ${message(failure)}`);
      setPollFailed(true);
    });
  }, [desktop, retry]);

  async function run(operation: () => Promise<void>, range = false) {
    if (!desktop) { setError('Desktop required. Launch the Tauri application to use local files and mpv.'); return; }
    if (busyRef.current || runtimeBusyRef.current) { setError('A native operation is still in progress. Try again when it finishes.'); return; }
    busyRef.current = true;
    epoch.current++;
    setBusy(true);
    setRangeBusy(range);
    setError('');
    try {
      // Let an older poll finish before sending a mutation; its epoch is already invalid.
      await statusRequest.current?.catch(() => {});
      await operation();
    }
    catch (failure) { setError(message(failure)); }
    finally { busyRef.current = false; setBusy(false); setRangeBusy(false); }
  }


  async function open(next: Paths) {
    if (!next.some(path => path.trim())) throw new Error('Choose at least one video before opening the viewer.');

    const result = await invoke<NativeStatus>('native_open', { paths: next });
    setStatus(result);
    setPollFailed(false);
    setRetry(value => value + 1);
  }

  async function select(selected: string[], target: 0 | 1) {
    const next = selectPaths(pathsRef.current, selected, target);
    if (!selected.length) return;
    editPaths(next);
    await open(next);
  }

  function control(action: string, value: string | number | boolean | null = null) {
    void run(async () => { setStatus(await invoke<NativeStatus>('native_control', { action, value })); });
  }

  function rangeControl(action: string, value: number) {
    if (runtimeBusyRef.current) return;
    if (busyRef.current && !rangeBusy) return;
    pendingRanges.current.set(action, value);
    if (busyRef.current) return;
    void run(async () => {
      while (pendingRanges.current.size) {
        const [action, value] = pendingRanges.current.entries().next().value!;
        pendingRanges.current.delete(action);
        try {
          const result = await invoke<NativeStatus>('native_control', { action, value });
          if (!pendingRanges.current.size) setStatus(result);
        } catch (failure) { setError(message(failure)); }
      }
    }, true);
  }

  useEffect(() => {
    if (!desktop) return;
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    const window = getCurrentWebviewWindow();
    void window.onDragDropEvent(event => {
      const payload = event.payload;
      if (payload.type === 'leave') { setHover(null); return; }
      // Read scale for every event so moving the window between monitors remains accurate.
      void window.scaleFactor().then(scale => {
        if (cancelled) return;
        const boxes = slots.current.map(slot => slot!.getBoundingClientRect());
        const target = dropTarget(payload.position, scale, boxes);
        setHover(payload.type === 'drop' ? null : target);
        if (payload.type === 'drop') {
          if (payload.paths.length === 1 && target === null) {
            setError('Drop a single file directly onto source A or B. Drop two files anywhere to fill both.');
          } else {
            void run(() => select(payload.paths, target ?? 0));
          }
        }
      }).catch(failure => { if (!cancelled) setError(`File drop failed: ${message(failure)}`); });
    }).then(off => { if (cancelled) off(); else unlisten = off; }).catch(failure => { if (!cancelled) setError(`Native file drop unavailable: ${message(failure)}`); });
    return () => { cancelled = true; unlisten?.(); };
  }, [desktop]);

  useEffect(() => {
    function keydown(event: KeyboardEvent) {
      if (!running || busyRef.current || runtimeBusyRef.current || event.altKey || event.ctrlKey || event.metaKey || event.repeat) return;
      if (event.target instanceof HTMLElement && event.target.closest('input, select, textarea, button, [contenteditable="true"]')) return;
      if (event.code === 'Space') { event.preventDefault(); control('playPause'); }
      if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') {
        event.preventDefault();
        if (event.shiftKey) control(event.key === 'ArrowRight' ? 'stepForward' : 'stepBackward');
        else control('seek', Math.max(0, Math.min(status!.duration, status!.time + (event.key === 'ArrowRight' ? 5 : -5))));
      }
    }
    window.addEventListener('keydown', keydown);
    return () => window.removeEventListener('keydown', keydown);
  }, [running, status]);

  return <main className="shell">
    <header className="topbar">
      <div className="identity"><p className="eyebrow">DaSiWa · Native comparison</p><h1>Simple Video Compare</h1></div>
      <div className="top-actions">
        {embedded && <span role="status">{running ? status?.paused ? 'Paused' : 'Playing' : 'No video loaded'}</span>}
        <button type="button" aria-label="Runtime" aria-expanded={runtime} data-backend-state={backend.checking ? 'checking' : backend.report?.ready ? 'ready' : 'missing'} title={backend.checking ? 'Checking backend' : backend.report?.ready ? 'Backend ready' : 'Backend components need attention'} onClick={() => setRuntime(!runtime)}><span aria-hidden="true" className={`backend-indicator ${backend.report?.ready && !backend.checking ? 'backend-ok' : 'backend-missing'}`}>{backend.checking ? '…' : backend.report?.ready ? '✓' : '⚠'}</span> Runtime</button>
        <button type="button" disabled={!desktop || busy} onClick={() => void run(() => getCurrentWebviewWindow().close())}>Quit</button>
      </div>
    </header>

    {!desktop && <section className="notice" role="status"><strong>Desktop required</strong><p>This browser preview cannot open local sources or control mpv. Launch the Tauri desktop application. No browser video or simulated playback is used.</p></section>}
    {error && <section className="error" role="alert"><strong>Native operation failed</strong><p>{error}</p><button type="button" disabled={!desktop || busy} onClick={() => { setError(''); setPollFailed(false); setRetry(value => value + 1); }}>Retry status</button><button type="button" onClick={() => setRuntime(true)}>Runtime help</button></section>}
    <RuntimeSetup expanded={runtime} disabled={busy || running} onBusyChange={value => { runtimeBusyRef.current = value; setRuntimeBusy(value); }} />
    {runtime && desktop && <BackendDetails backend={backend} disabled={runtimeBusy} />}
    {runtime && <section className="runtime panel" aria-label="Runtime instructions"><div className="section-head"><h2>Native runtime</h2><button type="button" onClick={() => setRuntime(false)}>Close runtime help</button></div><p>Playback is embedded in this application: <code>libmpv.so.2</code> on Linux and 64-bit <code>libmpv-2.dll</code> on Windows. Place the library beside the app or in its <code>lib/</code> directory, or set <code>DAWVC_LIBMPV</code> to its full path. Linux also searches the system library. No separate mpv executable or player window is used. All platforms require <code>ffprobe</code> (included with FFmpeg) beside the app, on PATH, or selected with <code>DAWVC_FFPROBE</code>. On Windows, use the download panel to install the app-managed runtime without copying files. Explicit <code>DAWVC_LIBMPV</code> and <code>DAWVC_FFPROBE</code> overrides take priority; otherwise the managed runtime is preferred over manual locations. Linux uses system packages.</p><ul><li>Ubuntu / Debian: <code>sudo apt install mpv ffmpeg</code></li><li>Arch / CachyOS: <code>sudo pacman -S mpv ffmpeg</code></li><li>Windows: use a 64-bit libmpv shared build and FFmpeg's <code>ffprobe.exe</code>; Microsoft Edge WebView2 Runtime is required for the controls.</li></ul><p>Restart the desktop application after changing PATH, then load the sources again. Current viewer: {status ? status.running ? 'running' : 'closed' : 'not connected'}{pollFailed ? ' · status unavailable' : ''}. Backend checks above verify library initialization, ffprobe and the native pane. Open a real file to verify codec support and decoding.</p></section>}

    <form className="sources" onSubmit={event => { event.preventDefault(); void run(() => open(pathsRef.current)); }}>
      {(['A', 'B'] as const).map((letter, index) => <section key={letter} ref={element => { slots.current[index] = element; }} className={`source panel source-${letter.toLowerCase()} ${hover === index ? 'dragging' : ''}`}>
        <div className="source-head"><span className={`badge badge-${letter.toLowerCase()}`}>{letter}</span><label htmlFor={`path-${letter}`}>{letter === 'A' ? 'Original / reference' : 'Upscaled / comparison'}</label><button type="button" disabled={!desktop || busy} onClick={() => void run(async () => select(await invoke<string[]>('native_pick'), index as 0 | 1))}>Browse {letter}</button></div>
        <input id={`path-${letter}`} spellCheck={false} autoComplete="off" disabled={!desktop || busy} placeholder="Drop a local file or paste its full path" value={paths[index]} onChange={event => { const next: Paths = [...pathsRef.current]; next[index] = event.target.value; editPaths(next); }} />
        <small>{status?.paths[index] ? `Loaded: ${status.paths[index]}` : 'No source loaded'}</small>
      </section>)}
      <div className="source-actions"><button className="primary" type="submit" disabled={!desktop || busy || !paths.some(path => path.trim())}>{busy ? 'Working…' : running ? 'Open / reopen native viewer' : 'Open native viewer'}</button><span>Two files fill A → B. Single files belong on their source strip.</span></div>
    </form>

    <section className="desk panel" aria-label="Native viewer controls">
      <div className="section-head"><div className="mode-tabs" role="group" aria-label="Comparison mode">{modes.map(([mode, label]) => <button key={mode} type="button" className={status?.mode === mode ? 'active' : ''} aria-pressed={status?.mode === mode} disabled={!running || busy || !status?.paths.every(path => path)} onClick={() => control('mode', mode)}>{label}</button>)}</div><div className="desk-actions"><button type="button" aria-expanded={!hidden} onClick={() => setHidden(!hidden)}>{hidden ? 'Show controls' : 'Hide controls'}</button><button type="button" aria-pressed={details} disabled={!running || busy} onClick={() => control('sourceDetails', !details)}>Source details</button></div></div>
      <div className="native-note"><div className="viewer-mark" aria-hidden="true"><span>A</span><span>B</span></div><div><h2>{embedded ? 'Native video above these controls' : running ? 'Comparison is in the mpv window' : 'A dedicated window for the pixels'}</h2><p>{running ? 'Use Open / reopen native viewer above to restart playback or reopen it after closing.' : 'Choose one video for playback or two videos for comparison, then open the native viewer.'} This control desk does not decode video. mpv plays the original files without browser codec limitations.</p></div><span className="viewer-state">{pollFailed ? 'Status unavailable' : status ? running ? status.paused ? 'Paused' : 'Playing' : 'Viewer closed' : desktop ? 'Connecting…' : 'Desktop only'}</span></div>
      {!hidden && <div className="transport">
        <div className="transport-row"><button className="primary" type="button" disabled={!running || busy} onClick={() => control('playPause')}>{status?.paused === false ? 'Pause' : 'Play'}</button><button type="button" disabled={!running || busy} onClick={() => control('stepBackward')} aria-label="Step backward one frame">← Frame</button><button type="button" disabled={!running || busy} onClick={() => control('stepForward')} aria-label="Step forward one frame">Frame →</button><button type="button" disabled={!running || busy} onClick={() => control('sync')}>Sync now</button><button type="button" disabled={!running || busy} onClick={() => control('fullscreen')}>Native fullscreen</button><button type="button" disabled={!running || busy} onClick={() => control('stop')}>Stop viewer</button></div>
        <div className="timeline"><Range label="Seek" value={status?.time ?? 0} max={status?.duration || 1} step={0.001} disabled={!running || (busy && !rangeBusy) || !status?.duration} commit={value => rangeControl('seek', value)} /><output className="time">{status ? `${formatTime(status.time)} / ${formatTime(status.duration)}` : '— / —'}</output></div>
        <div className="mix-row">{(status?.mode === 'slider' || status?.mode === 'blend') && <Range label={status.mode === 'slider' ? 'A/B split' : 'Blend amount'} value={status.amount} max={1} step={0.01} disabled={!running || (busy && !rangeBusy)} commit={value => rangeControl('amount', value)} />}<label className="audio">Audio <select disabled={!running || busy} value={status?.audio ?? 'mute'} onChange={event => control('audio', event.target.value)}><option value="A">A</option><option value="B">B</option><option value="mute">Mute</option></select></label><Range label="Volume" value={status?.volume ?? 0} max={100} step={1} disabled={!running || (busy && !rangeBusy)} commit={value => rangeControl('volume', value)} /><span className="volume">{status ? `${status.volume}%` : '—'}</span></div>
      </div>}
    </section>


    <footer><span>Native mpv · original sources · no browser proxy</span><span><kbd>Space</kbd> play / pause · <kbd>←</kbd><kbd>→</kbd> ±5s · <kbd>Shift + arrow</kbd> frame step <small>(while this desk has focus)</small></span></footer>
  </main>;
}
