"""Runtime download UI contract test; IPC responses are explicit fixtures, not download evidence.
Run with a Vite development server and Playwright installed.
"""
import sys
from playwright.sync_api import sync_playwright, expect

URL = sys.argv[1] if len(sys.argv) > 1 else 'http://127.0.0.1:5173'
FIXTURE = """
import { mockIPC, mockWindows } from '/node_modules/@tauri-apps/api/mocks.js';
import { emit } from '/node_modules/@tauri-apps/api/event.js';
window.emitFixtureDrop = payload => emit('tauri://drag-drop', payload);
window.isTauri = true;
mockWindows('main');
window.fixture = { installs: 0, checks: 0, failInstall: false, nativeCalls: [],
  runtime: { supported: true, installed: false, installedVersion: null, availableVersion: 'fixture-v2', updateAvailable: false, busy: false, progress: 0, message: '' },
  status: { paths: [], duration: 5, time: 0, paused: true, mode: 'side', amount: .5, audio: 'A', volume: 100, running: false }
};
mockIPC(async (command, args) => {
  const f = window.fixture;
  if (['native_open', 'native_probe', 'native_control'].includes(command)) f.nativeCalls.push({command, args});
  if (command === 'plugin:window|scale_factor') return 1;
  if (command === 'native_probe') return {format: {filename: args.path}, streams: []};
  if (command === 'native_open') { f.status.paths = args.paths; f.status.running = true; return structuredClone(f.status); }
  if (command === 'native_status') return structuredClone(f.status);
  if (command === 'runtime_status') { f.checks++; return structuredClone(f.runtime); }
  if (command === 'runtime_install') {
    f.installs++;
    f.runtime.busy = true; f.runtime.progress = 42; f.runtime.message = 'Downloading fixture';
    await new Promise(resolve => setTimeout(resolve, 900));
    f.runtime.busy = false;
    if (f.failInstall) { f.runtime.message = 'fixture checksum mismatch'; throw f.runtime.message; }
    Object.assign(f.runtime, {installed: true, installedVersion: 'fixture-v2', updateAvailable: false, progress: 100, message: 'Runtime installed. Reopen the viewer to use it.'});
    return structuredClone(f.runtime);
  }
  if (command === 'native_control') { if (args.action === 'stop') f.status.running = false; return structuredClone(f.status); }
  throw 'Unexpected command: ' + command;
}, {shouldMockEvents: true});
"""

with sync_playwright() as p:
    browser = p.chromium.launch(headless=True)
    page = browser.new_page(viewport={'width': 1200, 'height': 700})
    errors = []
    page.on('pageerror', lambda error: errors.append(str(error)))
    def inject(route):
        response = route.fetch()
        route.fulfill(response=response, body=FIXTURE + '\n' + response.text())
    page.route('**/src/main.tsx*', inject)
    page.goto(URL, wait_until='networkidle')
    download = page.get_by_role('button', name='Download Windows runtime', exact=True)
    expect(download).to_be_visible()
    assert page.evaluate('fixture.installs') == 0, 'Startup must not download anything'
    page.get_by_label('Original / reference', exact=True).fill('/fixture/existing.mkv')
    download.click()
    expect(page.get_by_role('progressbar')).to_be_visible()
    expect(page.get_by_role('button', name='Downloading runtime…')).to_be_disabled()
    expect(page.get_by_role('button', name='Browse A', exact=True)).to_be_disabled()
    expect(page.get_by_label('Volume', exact=True)).to_be_disabled()
    page.get_by_role('button', name='Source details', exact=True).click()
    page.evaluate("document.dispatchEvent(new KeyboardEvent('keydown', {code: 'Space', key: ' '}))")
    page.evaluate("emitFixtureDrop({paths: ['/fixture/a.mkv', '/fixture/b.mkv'], position: {x: 0, y: 0}})")
    page.wait_for_function("fixture.nativeCalls.length > 0 || document.body.innerText.includes('A native operation is still in progress.')")
    assert page.evaluate('fixture.runtime.busy'), 'Drop must be tested during the installer delay'
    assert page.evaluate('fixture.nativeCalls') == [], 'Installing must block native_open, native_probe and native_control'
    expect(page.get_by_text('A native operation is still in progress. Try again when it finishes.', exact=True)).to_be_visible()
    expect(page.get_by_text('Runtime installed. Reopen the viewer to use it.', exact=True)).to_be_visible(timeout=10000)
    assert page.evaluate('fixture.installs') == 1
    page.get_by_role('button', name='Source details', exact=True).click()
    page.get_by_role('button', name='Retry status', exact=True).click()
    expect(page.get_by_text('A native operation is still in progress. Try again when it finishes.', exact=True)).to_have_count(0)
    page.set_viewport_size({'width': 1200, 'height': 440})
    assert page.locator('.mix-row').bounding_box()['y'] + page.locator('.mix-row').bounding_box()['height'] <= 440, 'Runtime completion must not clip playback controls'
    page.set_viewport_size({'width': 1200, 'height': 700})
    page.get_by_role('button', name='Runtime', exact=True).click()
    expect(page.get_by_text('fixture-v2', exact=False).first).to_be_visible()
    page.evaluate("Object.assign(fixture.runtime, {installedVersion:'fixture-v1',updateAvailable:true,message:''})")
    page.get_by_role('button', name='Check runtime', exact=True).click()
    update = page.get_by_role('button', name='Update Windows runtime', exact=True)
    expect(update).to_be_enabled()
    page.evaluate('fixture.status.running = true')
    expect(update).to_be_disabled()
    page.get_by_role('button', name='Stop viewer', exact=True).click()
    expect(update).to_be_enabled()
    page.evaluate('fixture.failInstall = true')
    update.click()
    expect(page.get_by_text('fixture checksum mismatch', exact=True)).to_be_visible(timeout=10000)
    assert page.evaluate('fixture.runtime.installedVersion') == 'fixture-v1'
    expect(page.get_by_role('button', name='Update Windows runtime', exact=True)).to_be_enabled()
    expect(page.get_by_role('button', name='Browse A', exact=True)).to_be_enabled()
    page.evaluate('fixture.failInstall = false')
    update.click()
    expect(page.get_by_role('button', name='Downloading runtime…')).to_be_disabled()
    expect(page.get_by_text('Runtime installed. Reopen the viewer to use it.', exact=True)).to_be_visible(timeout=10000)
    assert page.evaluate('fixture.installs') == 3
    assert page.evaluate('fixture.runtime.installedVersion') == 'fixture-v2'
    expect(page.get_by_text('fixture checksum mismatch', exact=True)).to_have_count(0)
    expect(page.get_by_role('button', name='Browse A', exact=True)).to_be_enabled()
    page.evaluate("fixture.nativeCalls = []; emitFixtureDrop({paths: ['/fixture/a.mkv', '/fixture/b.mkv'], position: {x: 0, y: 0}})")
    expect(page.get_by_role('button', name='Stop viewer', exact=True)).to_be_enabled()
    assert page.evaluate("fixture.nativeCalls.map(call => call.command).sort()") == ['native_open', 'native_probe', 'native_probe']
    expect(page.get_by_label('Volume', exact=True)).to_be_enabled()
    assert not errors, errors
    browser.close()
print('PASS: explicit install consent, progress, installer drop guard, local checks, playback guard, update failure and successful retry with controls restored (fixture IPC).')
