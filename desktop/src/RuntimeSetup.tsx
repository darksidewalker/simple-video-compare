import { useEffect, useRef, useState } from 'react';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { pollStatus } from './model';

type RuntimeStatus = {
  supported: boolean; installed: boolean; installedVersion: string | null;
  availableVersion: string; updateAvailable: boolean; busy: boolean; progress: number; message: string;
};
const message = (error: unknown) => error instanceof Error ? error.message : String(error);

export default function RuntimeSetup({ expanded, disabled, onBusyChange }: {
  expanded: boolean; disabled: boolean; onBusyChange: (busy: boolean) => void;
}) {
  const [status, setStatus] = useState<RuntimeStatus | null>(null);
  const [error, setError] = useState('');
  const [installing, setInstalling] = useState(false);
  const active = useRef(false);

  async function check() {
    setError('');
    try { setStatus(await invoke<RuntimeStatus>('runtime_status')); }
    catch (failure) { setError(message(failure)); }
  }
  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    void invoke<RuntimeStatus>('runtime_status').then(next => {
      if (!disposed) setStatus(next);
    }).catch(failure => { if (!disposed) setError(message(failure)); });
    return () => { disposed = true; };
  }, []);
  useEffect(() => {
    if (!installing) return;
    return pollStatus(() => invoke<RuntimeStatus>('runtime_status'), next => {
      if (active.current) setStatus(next);
    }, failure => { if (active.current) setError(message(failure)); });
  }, [installing]);

  async function install() {
    if (active.current || disabled) return;
    active.current = true;
    setInstalling(true);
    onBusyChange(true);
    setError('');
    try {
      const next = await invoke<RuntimeStatus>('runtime_install');
      active.current = false;
      setStatus(next);
    } catch (failure) {
      active.current = false;
      setError(message(failure));
      try { setStatus(await invoke<RuntimeStatus>('runtime_status')); }
      catch { setStatus(previous => previous && { ...previous, busy: false }); }
    }
    finally {
      active.current = false;
      setInstalling(false);
      onBusyChange(false);
    }
  }

  if (!isTauri() || (!expanded && !error && (!status?.supported || (status.installed && !status.updateAvailable && !installing && !status.message)))) return null;
  if (status && !status.supported) return null;
  const working = installing || !!status?.busy;
  return <section className="runtime-download panel" aria-label="Windows runtime download">
    <div className="section-head"><h2>Windows runtime</h2><div className="top-actions">
      <button type="button" disabled={working} onClick={() => void check()}>Check runtime</button>
      {status && (!status.installed || status.updateAvailable || working) && <button type="button" className="primary" disabled={disabled || working} onClick={() => void install()}>{working ? 'Downloading runtime…' : status.installed ? 'Update Windows runtime' : 'Download Windows runtime'}</button>}
    </div></div>
    {status && <p>{status.installed ? `Installed: ${status.installedVersion}. ` : 'App-managed runtime is not installed. '}{`Included catalog: ${status.availableVersion}. `}Download libmpv and ffprobe from GitHub into this app's LocalAppData folder. No administrator access or PATH changes. Existing manual installations remain usable.</p>}
    {disabled && !working && <p>Stop playback and wait for any current operation before installing.</p>}
    {working && <progress aria-label="Runtime download progress" max={100} value={status?.progress ?? 0} />}
    {status?.message && !error && <p role="status">{status.message}</p>}
    {error && <p className="probe-error" role="alert">{error}</p>}
    <small>Downloads run only after you click. SHA-256 verified; new versions activate on the next open. Update checks use the catalog included with this app, not unreviewed upstream nightlies.</small>
  </section>;
}
