"""Optional real Linux/WebKitGTK smoke test, without mocked native responses.
Launch the built app with WEBKIT_INSPECTOR_HTTP_SERVER=127.0.0.1:9238,
then: uv run --no-project --with websocket-client python desktop/tests/native_smoke.py A B
The test opens the sources and quits this dedicated app instance on success.
"""
import json
import re
import sys
import time
from urllib.request import urlopen
from websocket import create_connection

assert len(sys.argv) == 3, 'Pass the two real source paths A and B'
inspector = '127.0.0.1:9238'
with urlopen(f'http://{inspector}', timeout=10) as response:
    match = re.search(r"/socket/[^'\"]+/WebPage", response.read().decode())
    assert match is not None, 'No inspectable desktop window found'
    endpoint = match.group()
ws = create_connection(f'ws://{inspector}{endpoint}', timeout=60)
target = json.loads(ws.recv())['params']['targetInfo']['targetId']
seq = 0


def evaluate(expression):
    global seq
    seq += 1
    payload = json.dumps({'id': seq, 'method': 'Runtime.evaluate', 'params': {'expression': expression, 'returnByValue': True}})
    ws.send(json.dumps({'id': seq + 10000, 'method': 'Target.sendMessageToTarget', 'params': {'targetId': target, 'message': payload}}))
    while True:
        response = json.loads(ws.recv())
        if response.get('method') == 'Target.dispatchMessageFromTarget':
            response = json.loads(response['params']['message'])
        if response.get('id') == seq:
            if 'error' in response:
                raise RuntimeError(response)
            result = response['result']
            if result.get('wasThrown'):
                raise RuntimeError(result)
            return result['result'].get('value')


initial = evaluate('document.body.innerText')
assert 'Simple Video Compare' in initial, initial
assert 'Native operation failed' not in initial, initial
script = '''window.__verification = null; (async () => {
try {
const invoke = window.__TAURI__.core.invoke;
const states = [];
for (const paths of [['', PATHS[1]], [PATHS[0], ''], PATHS]) {
  for (let i = 0; i < 2; i++) {
    const input = document.querySelector(i === 0 ? '#path-A' : '#path-B');
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(input, paths[i]);
    input.dispatchEvent(new Event('input', {bubbles: true}));
  }
  await new Promise(resolve => setTimeout(resolve, 100));
  document.querySelector('form').requestSubmit();
  await new Promise(resolve => setTimeout(resolve, 100));
  const status = await invoke('native_status');
  if (!status.running || JSON.stringify(status.paths) !== JSON.stringify(paths)) throw 'UI did not open the expected source slots';
  if (!paths[0] && status.audio !== 'B') throw 'B-only audio was assigned to A';
  if (!paths[1] && status.audio !== 'A') throw 'A-only audio was assigned to B';
  states.push(status);
}
if (document.querySelector('[role="alert"]')) throw document.querySelector('[role="alert"]').innerText;
const scale = await invoke('plugin:window|scale_factor');
if (!(scale > 0)) throw 'Invalid display scale';
for (const mode of ['slider', 'blend', 'diff', 'side']) states.push(await invoke('native_control', { action: 'mode', value: mode }));
states.push(await invoke('native_control', { action: 'seek', value: 2 }));
states.push(await invoke('native_control', { action: 'playPause', value: null }));
await new Promise(resolve => setTimeout(resolve, 1500));
states.push(await invoke('native_status'));
window.__verification = {states, text: document.body.innerText};
} catch (e) { window.__verification = {error: String(e)}; }
})()'''.replace('PATHS', json.dumps(sys.argv[1:]))
evaluate(script)
for _ in range(120):
    result = evaluate('window.__verification')
    if result is not None:
        break
    time.sleep(.5)
else:
    raise RuntimeError('verification timeout')
assert 'error' not in result, result
assert all(state['running'] for state in result['states'])
assert result['states'][-1]['time'] > 2, result
assert 'Playing' in result['text'], result
assert 'Native operation failed' not in result['text'], result
assert evaluate('document.querySelector(".mix-row").getBoundingClientRect().bottom <= innerHeight'), 'Audio/volume controls are clipped'
print(json.dumps(result['states'][-1], indent=2))
evaluate('setTimeout(() => [...document.querySelectorAll("button")].find(button => button.textContent === "Quit").click(), 100)')
ws.close()
print('PASS: real UI source loading, event-listen permission, scale, all modes, seek and advancing playback; Quit requested.')
