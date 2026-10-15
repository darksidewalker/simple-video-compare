"""Backend-readiness UI fixtures; not native-backend evidence."""
import sys
from playwright.sync_api import sync_playwright, expect

URL = sys.argv[1] if len(sys.argv) > 1 else 'http://127.0.0.1:5173'
FIXTURE = """
import { mockIPC, mockWindows } from '/node_modules/@tauri-apps/api/mocks.js';
window.isTauri = true;
mockWindows('main');
window.fixture = {backendCalls: 0, ready: false};
mockIPC(async (command) => {
  if (command === 'backend_check') {
    fixture.backendCalls++;
    return {ready: fixture.ready, checks: [
      {name: 'libmpv', ok: fixture.ready, path: fixture.ready ? 'C:/fixture/libmpv-2.dll' : null, detail: fixture.ready ? 'Library API loaded' : 'Configured library cannot load', hint: fixture.ready ? '' : 'Click Download Windows runtime or correct DAWVC_LIBMPV.'},
      {name: 'ffprobe', ok: true, path: 'C:/fixture/ffprobe.exe', detail: 'ffprobe fixture-version', hint: ''},
      {name: 'Native video pane', ok: true, path: null, detail: 'Native window ready', hint: ''}
    ]};
  }
  if (command === 'runtime_status') return {supported:false};
  if (command === 'native_status') return {paths:[],duration:0,time:0,paused:true,mode:'side',amount:.5,audio:'A',volume:100,running:false};
  throw 'Unexpected command: '+command;
}, {shouldMockEvents:true});
"""
with sync_playwright() as p:
    browser = p.chromium.launch(headless=True)
    page = browser.new_page(viewport={'width':1200,'height':700})
    errors = []
    page.on('pageerror', lambda error: errors.append(str(error)))
    def inject(route):
        response = route.fetch()
        route.fulfill(response=response, body=FIXTURE+'\n'+response.text())
    page.route('**/src/main.tsx*', inject)
    page.goto(URL, wait_until='networkidle')
    runtime = page.get_by_role('button', name='Runtime', exact=True)
    expect(runtime).to_have_attribute('data-backend-state','missing')
    assert page.evaluate('fixture.backendCalls') == 1, 'Check once on startup, not a probe loop'
    runtime.click()
    expect(page.get_by_role('region',name='Backend checks')).to_be_visible()
    expect(page.get_by_text('Configured library cannot load',exact=True)).to_be_visible()
    expect(page.get_by_text('Click Download Windows runtime or correct DAWVC_LIBMPV.',exact=True)).to_be_visible()
    expect(page.get_by_text('C:/fixture/ffprobe.exe',exact=True)).to_be_visible()
    page.evaluate('fixture.ready = true')
    page.get_by_role('button',name='Recheck backend',exact=True).click()
    expect(runtime).to_have_attribute('data-backend-state','ready')
    expect(page.get_by_text('C:/fixture/libmpv-2.dll',exact=True)).to_be_visible()
    expect(page.get_by_text('All required backend components are ready.',exact=True)).to_be_visible()
    assert page.locator('.backend-indicator').inner_text() == '✓'
    assert not errors, errors
    browser.close()
print('PASS: startup check, missing component/hint, found paths, explicit recheck and green readiness marker (fixture IPC).')
