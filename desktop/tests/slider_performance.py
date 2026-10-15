"""Optional native slider benchmark with two supplied original videos.
Launch a dedicated app with WEBKIT_INSPECTOR_HTTP_SERVER=127.0.0.1:9238
and DAWVC_RENDER_VERIFY=1, then:
uv run --no-project --with websocket-client python desktop/tests/slider_performance.py A B
The benchmark closes that app. Reported timings measure command round trips,
not display latency. GLAREA_SPLIT logs verify composed pixels/cache reuse.
"""
import json,re,time,sys,statistics
from urllib.request import urlopen
from websocket import create_connection
assert len(sys.argv) == 3, 'Pass the original source paths A and B'
html=urlopen('http://127.0.0.1:9238', timeout=10).read().decode()
endpoint=re.search(r"/socket/[^'\"]+/WebPage",html)
assert endpoint is not None, 'No inspectable desktop window found'
ws=create_connection('ws://127.0.0.1:9238'+endpoint.group(),timeout=60)
target=json.loads(ws.recv())['params']['targetInfo']['targetId'];seq=0
def evaluate(expression):
 global seq
 seq+=1
 payload=json.dumps({'id':seq,'method':'Runtime.evaluate','params':{'expression':expression,'returnByValue':True}})
 ws.send(json.dumps({'id':seq+10000,'method':'Target.sendMessageToTarget','params':{'targetId':target,'message':payload}}))
 while True:
  r=json.loads(ws.recv())
  if r.get('method')=='Target.dispatchMessageFromTarget':r=json.loads(r['params']['message'])
  if r.get('id')==seq:
   assert not r.get('error') and not r['result'].get('wasThrown'),r
   return r['result']['result'].get('value')
script='''window.__bench=null;(async()=>{try {
const invoke=window.__TAURI__.core.invoke;
const probes=await Promise.all(PATHS.map(path=>invoke('native_probe',{path})));
await invoke('native_open',{paths:PATHS});
await invoke('native_control',{action:'mode',value:'slider'});
await new Promise(r=>setTimeout(r,300));
const times=[];
for(let i=0;i<120;i++) {
 const begin=performance.now();
 await invoke('native_control',{action:'amount',value:i%2?.2:.8});
 times.push(performance.now()-begin);
 await new Promise(r=>setTimeout(r,16));
}
let playing=await invoke('native_control',{action:'playPause',value:null});
for(let i=0;i<30;i++) {
 await invoke('native_control',{action:'amount',value:i/30});
 await new Promise(r=>setTimeout(r,16));
}
const final=await invoke('native_status');
await invoke('native_control',{action:'playPause',value:null});
await invoke('native_control',{action:'seek',value:1});
await invoke('native_control',{action:'amount',value:.8});
await new Promise(r=>setTimeout(r,100));
for(const mode of ['side','blend','diff','slider']) await invoke('native_control',{action:'mode',value:mode});
await invoke('native_control',{action:'amount',value:.2});
await new Promise(r=>setTimeout(r,100));
window.__bench={times,playing,final,probes};
}catch(e){window.__bench={error:String(e)}}})()'''.replace('PATHS',json.dumps(sys.argv[1:3]))
evaluate(script)
r=None
for _ in range(12000):
 r=evaluate('window.__bench')
 if r is not None:break
 time.sleep(.01)
assert r and 'error' not in r,r
assert r['final']['time']>r['playing']['time'],r
samples=sorted(r.pop('times'))
print(json.dumps({'command_ms':{'median':statistics.median(samples),'p95':samples[int(len(samples)*.95)],'max':max(samples)},'playing_start':r['playing']['time'],'playing_end':r['final']['time'],'sources':[(p['streams'][0]['width'],p['streams'][0]['height']) for p in r['probes']]},indent=2))
evaluate('setTimeout(()=>window.__TAURI__.window.getCurrentWindow().close(),100)')
ws.close()