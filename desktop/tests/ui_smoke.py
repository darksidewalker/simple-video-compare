"""Optional UI contract smoke test: run with Playwright installed and `npm run dev` running.
All native responses below are explicit test fixtures, not runtime fallback data.
Usage: python tests/ui_smoke.py [http://127.0.0.1:5173]
"""
import json
import os
import sys
from pathlib import Path
from playwright.sync_api import sync_playwright, expect

URL = sys.argv[1] if len(sys.argv) > 1 else 'http://127.0.0.1:5173'
ARTIFACTS = Path(os.environ.get('TMPDIR', str(Path.home() / '.hermes/cache/scratch'))) / 'dasiwa-desktop-verification'
ARTIFACTS.mkdir(parents=True, exist_ok=True)
FIXTURE = """
import { mockIPC, mockWindows } from '/node_modules/@tauri-apps/api/mocks.js';
import { emit } from '/node_modules/@tauri-apps/api/event.js';
window.isTauri = true;
mockWindows('main');
window.fixture = {
  calls: [], completed: [], active: 0, maxActive: 0, controlDelay: 0, statusDelay: 0, failControl: false,
  pick: ['/fixture/original.mkv', '/fixture/upscaled.mkv'], failStatus: false,
  status: { paths: [], duration: 60, time: 0, paused: true, mode: 'side', amount: 0.5, audio: 'A', volume: 80, running: false }
};
window.emitFixtureDrop = payload => emit('tauri://drag-drop', payload);
mockIPC(async (command, args) => {
  const f = window.fixture;
  f.calls.push({command, args});
  if (command === 'runtime_status') return {supported: false};
  if (command === 'native_status') {
    f.maxActive = Math.max(f.maxActive, ++f.active);
    const status = structuredClone(f.status), fail = f.failStatus;
    try {
      if (f.statusDelay) await new Promise(resolve => setTimeout(resolve, f.statusDelay));
      if (fail) throw 'fixture: mpv disconnected';
      return status;
    } finally { f.active--; }
  }
  if (command === 'native_pick') return f.pick;
  if (command === 'native_probe') return { format: { filename: args.path, duration: '60.0' }, streams: [{ index: 0, codec_type: 'video', codec_name: 'hevc', width: 3840, height: 2160, pix_fmt: 'yuv420p10le', color_transfer: 'smpte2084' }] };
  if (command === 'native_open') { f.status.paths = args.paths; f.status.running = true; return structuredClone(f.status); }
  if (command === 'native_control') {
    f.maxActive = Math.max(f.maxActive, ++f.active);
    try {
      if (f.controlDelay) await new Promise(resolve => setTimeout(resolve, f.controlDelay));
      if (f.failControl) { f.failControl = false; throw 'fixture: control failed'; }
      if (args.action === 'playPause') f.status.paused = !f.status.paused;
      if (args.action === 'seek') f.status.time = args.value;
      if (['mode', 'amount', 'audio', 'volume'].includes(args.action)) f.status[args.action] = args.value;
      if (args.action === 'stop') f.status.running = false;
      f.completed.push({...args});
      return structuredClone(f.status);
    } finally { f.active--; }
  }
  if (command === 'plugin:window|scale_factor') return 2;
  if (command === 'plugin:window|close') return null;
  throw 'Unexpected command: ' + command;
}, { shouldMockEvents: true });
"""

with sync_playwright() as playwright:
    browser = playwright.chromium.launch(headless=True)
    page = browser.new_page(viewport={'width': 1200, 'height': 850})
    errors = []
    page.on('pageerror', lambda error: errors.append(str(error)))
    page.goto(URL, wait_until='networkidle')
    expect(page.get_by_text('Desktop required', exact=True)).to_be_visible()
    expect(page.get_by_role('button', name='Browse A')).to_be_disabled()
    assert page.locator('video').count() == 0
    page.get_by_role('button', name='Hide controls').click()
    expect(page.get_by_role('button', name='Play', exact=True)).to_have_count(0)
    page.get_by_role('button', name='Show controls').click()
    page.get_by_role('button', name='Runtime', exact=True).click()
    expect(page.get_by_text('sudo apt install mpv ffmpeg', exact=True)).to_be_visible()
    page.screenshot(path=str(ARTIFACTS / 'browser-desktop-required.png'), full_page=True)
    page.set_viewport_size({'width': 375, 'height': 812})
    assert page.evaluate('document.documentElement.scrollWidth <= innerWidth')
    page.screenshot(path=str(ARTIFACTS / 'mobile-desktop-required.png'), full_page=True)
    assert not errors, errors

    def inject_fixture(route):
        response = route.fetch()
        route.fulfill(response=response, body=FIXTURE + '\n' + response.text())

    page.set_viewport_size({'width': 1200, 'height': 850})
    page.route('**/src/main.tsx*', inject_fixture)
    page.reload(wait_until='networkidle')
    page.evaluate("fixture.pick = ['/fixture/upscaled.mkv']")
    page.get_by_role('button', name='Browse B').click()
    expect(page.locator('#path-A')).to_have_value('')
    expect(page.locator('#path-B')).to_have_value('/fixture/upscaled.mkv')
    expect(page.get_by_role('button', name='Play', exact=True)).to_be_enabled()
    expect(page.get_by_role('button', name='Slider', exact=True)).to_be_disabled()
    expect(page.locator('.source-a small')).to_have_text('No source loaded')
    expect(page.locator('.source-b small')).to_contain_text('/fixture/upscaled.mkv')
    page.evaluate("fixture.pick = ['/fixture/original.mkv', '/fixture/upscaled.mkv']")
    page.get_by_role('button', name='Browse B').click()
    expect(page.locator('#path-A')).to_have_value('/fixture/original.mkv')
    expect(page.locator('#path-B')).to_have_value('/fixture/upscaled.mkv')
    expect(page.get_by_role('button', name='Play', exact=True)).to_be_enabled()
    page.get_by_role('button', name='Slider', exact=True).click()
    expect(page.get_by_role('slider', name='A/B split')).to_be_visible()
    split = page.get_by_role('slider', name='A/B split')
    split.focus()
    split.press('ArrowRight')
    page.wait_for_function("fixture.calls.some(c => c.command === 'native_control' && c.args.action === 'amount' && c.args.value === 0.51)")
    page.get_by_role('combobox', name='Audio').select_option('B')
    page.wait_for_function("fixture.status.audio === 'B'")
    page.locator('h1').click()
    page.keyboard.press('Space')
    expect(page.get_by_role('button', name='Pause', exact=True)).to_be_enabled()
    page.keyboard.press('ArrowRight')
    page.wait_for_function('fixture.status.time === 5')
    page.keyboard.press('Shift+ArrowLeft')
    page.wait_for_function("fixture.calls.some(c => c.command === 'native_control' && c.args.action === 'stepBackward' && c.args.value === null)")
    # A slow IPC call must not disable a held slider or lose its last queued value.
    page.evaluate('fixture.controlDelay = 160')

    def drag_live(label, action, field):
        slider = page.get_by_role('slider', name=label)
        slider.scroll_into_view_if_needed()
        box = slider.bounding_box()
        y = box['y'] + box['height'] / 2
        start = page.evaluate('fixture.completed.length')
        inputs = slider.evaluate('el => { el.fixtureInputs = 0; el.addEventListener("input", () => el.fixtureInputs++); return el.fixtureInputs; }')
        page.mouse.move(box['x'] + box['width'] * 0.15, y)
        page.mouse.down()
        try:
            for fraction in (0.25, 0.4, 0.55):
                page.mouse.move(box['x'] + box['width'] * fraction, y, steps=5)
                page.wait_for_timeout(210)
                expect(slider).to_be_enabled()
            intermediate = page.evaluate('start => fixture.completed.slice(start)', start)
            values = [call['value'] for call in intermediate if call['action'] == action]
            assert len(set(values)) >= 2, (label, values)
            # Burst updates arrive faster than IPC; only the newest pending value matters.
            page.mouse.move(box['x'] + box['width'] * 0.85, y, steps=15)
            final = float(slider.input_value())
            expect(page.get_by_role('button', name='Stop viewer')).to_be_disabled()
        finally:
            page.mouse.up()
        page.wait_for_function('([field, value]) => fixture.status[field] === value && fixture.active === 0', arg=[field, final])
        expect(page.get_by_role('button', name='Stop viewer')).to_be_enabled()
        expect(slider).to_have_value(str(int(final)) if final.is_integer() else str(final))
        completed = page.evaluate('start => fixture.completed.slice(start)', start)
        calls = [call for call in completed if call['action'] == action]
        assert calls[-1]['value'] == final, (label, calls, final)
        assert len(calls) < slider.evaluate('el => el.fixtureInputs') - inputs, (label, calls)
        assert page.evaluate('fixture.maxActive') == 1
        page.wait_for_timeout(300)
        expect(slider).to_have_value(str(int(final)) if final.is_integer() else str(final))

    drag_live('A/B split', 'amount', 'amount')
    drag_live('Seek', 'seek', 'time')
    drag_live('Volume', 'volume', 'volume')
    page.get_by_role('button', name='Blend', exact=True).click()
    expect(page.get_by_role('slider', name='Blend amount')).to_be_visible()
    drag_live('Blend amount', 'amount', 'amount')

    # Start a mutation while an old poll (including an old failure) is in flight.
    for fail in (False, True):
        page.evaluate('fail => { fixture.statusDelay = 450; fixture.failStatus = fail; }', fail)
        page.wait_for_function("fixture.active === 1 && fixture.calls.at(-1).command === 'native_status'")
        page.evaluate('fixture.failStatus = false')
        volume = page.get_by_role('slider', name='Volume')
        volume.focus()
        volume.press('ArrowLeft')
        final = float(volume.input_value())
        page.wait_for_function('value => fixture.status.volume === value', arg=final)
        page.evaluate('fixture.statusDelay = 0')
        expect(page.get_by_role('button', name='Stop viewer')).to_be_enabled()
        expect(volume).to_have_value(str(int(final)))
        expect(page.get_by_role('alert')).to_have_count(0)
        assert page.evaluate('fixture.maxActive') == 1

    # A failed intermediate request still drains the newest value and reports the error.
    page.evaluate('fixture.failControl = true')
    volume.focus()
    volume.press('ArrowLeft')
    volume.press('ArrowLeft')
    final = float(volume.input_value())
    page.wait_for_function('value => fixture.status.volume === value', arg=final)
    expect(page.get_by_role('alert')).to_contain_text('fixture: control failed')
    expect(page.get_by_role('button', name='Stop viewer')).to_be_enabled()
    page.get_by_role('button', name='Retry status').click()
    expect(page.get_by_role('alert')).to_have_count(0)
    page.evaluate('fixture.controlDelay = 0')
    page.get_by_role('button', name='Native fullscreen').click()
    page.get_by_role('button', name='Source details').click()
    expect(page.get_by_text('smpte2084', exact=True)).to_have_count(2)
    page.get_by_text('Raw ffprobe JSON', exact=True).first.click()
    expect(page.locator('pre').first).to_contain_text('yuv420p10le')
    page.evaluate("fixture.pick = []")
    page.get_by_role('button', name='Browse A').click()
    expect(page.locator('#path-A')).to_have_value('/fixture/original.mkv')
    box = page.locator('.source-b').bounding_box()
    page.evaluate("payload => emitFixtureDrop(payload)", {'paths': ['/fixture/dropped-b.mov'], 'position': {'x': (box['x'] + 40) * 2, 'y': (box['y'] + 35) * 2}})
    expect(page.locator('#path-A')).to_have_value('/fixture/original.mkv')
    expect(page.locator('#path-B')).to_have_value('/fixture/dropped-b.mov')
    page.wait_for_function("fixture.status.paths[1] === '/fixture/dropped-b.mov'")
    page.screenshot(path=str(ARTIFACTS / 'native-contract-fixture.png'), full_page=True)
    page.evaluate('fixture.failStatus = true')
    expect(page.get_by_role('alert')).to_contain_text('Status polling stopped')
    count = page.evaluate("fixture.calls.filter(c => c.command === 'native_status').length")
    page.wait_for_timeout(800)
    assert page.evaluate("fixture.calls.filter(c => c.command === 'native_status').length") == count
    page.evaluate('fixture.failStatus = false')
    page.get_by_role('button', name='Retry status').click()
    expect(page.get_by_role('button', name='Pause', exact=True)).to_be_enabled()
    page.get_by_role('button', name='Quit', exact=True).click()
    page.wait_for_function("fixture.calls.some(c => c.command === 'plugin:window|close')")
    assert not errors, errors
    (ARTIFACTS / 'ipc-calls.json').write_text(json.dumps(page.evaluate('fixture.calls'), indent=2))
    browser.close()
print(f'PASS: browser guard, responsive rendering, native fixture commands, live coalesced ranges, serialized polling/control errors, 2× B-drop targeting, source JSON, polling failure/retry, quit. Artifacts: {ARTIFACTS}')
