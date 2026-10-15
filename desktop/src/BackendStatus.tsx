import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

type BackendCheck = { name: string; ok: boolean; path: string | null; detail: string; hint: string };
type BackendReport = { ready: boolean; checks: BackendCheck[] };

export function useBackendCheck(desktop: boolean, installing: boolean) {
  const [report, setReport] = useState<BackendReport | null>(null);
  const [error, setError] = useState('');
  const [checking, setChecking] = useState(false);
  const active = useRef(false);
  const check = useCallback(async () => {
    if (active.current) return;
    active.current = true;
    setChecking(true);
    setError('');
    try { setReport(await invoke<BackendReport>('backend_check')); }
    catch (failure) { setReport(null); setError(String(failure)); }
    finally { active.current = false; setChecking(false); }
  }, []);
  useEffect(() => { if (desktop && !installing) void check(); }, [desktop, installing, check]);
  return { report, error, checking, check };
}

export function BackendDetails({ backend, disabled }: {
  backend: ReturnType<typeof useBackendCheck>; disabled: boolean;
}) {
  return <section className="backend-checks panel" aria-label="Backend checks">
    <div className="section-head"><h2>Backend checks</h2><button type="button" disabled={disabled || backend.checking} onClick={() => void backend.check()}>Recheck backend</button></div>
    <p role="status">{backend.checking ? 'Checking required components…' : backend.report?.ready ? 'All required backend components are ready.' : 'Some backend components are missing or unavailable.'}</p>
    {backend.error && <p className="probe-error" role="alert">{backend.error} — Recheck backend. If this persists, restart the app and verify its runtime installation.</p>}
    <ul>{backend.report?.checks.map(item => <li key={item.name}>
      <strong className={item.ok ? 'backend-ok' : 'backend-missing'}>{item.ok ? '✓' : '⚠'} {item.name}: {item.ok ? 'Found / ready' : 'Missing / unavailable'}</strong>
      {item.path && <code>{item.path}</code>}<p>{item.detail}</p>
      {!item.ok && item.hint && <p className="backend-hint">{item.hint}</p>}
    </li>)}</ul>
    <small>Local prerequisite checks only; no downloads or playback restarts. Codec support for a particular file is confirmed when it is opened.</small>
  </section>;
}
