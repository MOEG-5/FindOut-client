#!/usr/bin/env python3
"""Real Linux desktop quota flows, private X11/bus/keychain and local HTTP only."""
import json
import os
from pathlib import Path
import subprocess as sp
import sys
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

ROOT = Path(__file__).resolve().parent.parent
if os.environ.get('FINDOUT_ISOLATED_QUOTA_TEST') != '1':
    with tempfile.TemporaryDirectory(prefix='findout-quota-') as temp:
        env = dict(os.environ, FINDOUT_ISOLATED_QUOTA_TEST='1', SLINT_SCALE_FACTOR='1')
        for key, directory in [('XDG_DATA_HOME','data'),('XDG_CONFIG_HOME','config'),('XDG_STATE_HOME','state'),('XDG_CACHE_HOME','cache'),('XDG_RUNTIME_DIR','runtime')]:
            path = Path(temp)/directory; path.mkdir(mode=0o700); env[key] = str(path)
        for key in ['WAYLAND_DISPLAY','WAYLAND_SOCKET','GNOME_KEYRING_CONTROL','SSH_AUTH_SOCK','DBUS_SESSION_BUS_ADDRESS','DISPLAY','XAUTHORITY']:
            env.pop(key, None)
        raise SystemExit(sp.call(['dbus-run-session', f'--config-file={ROOT}/tests/dbus-session.conf', '--',
            'xvfb-run','-a','-s','-screen 0 1920x1080x24 -nolisten tcp',sys.executable,__file__,*sys.argv[1:]],env=env))

out = Path(sys.argv[1] if len(sys.argv)>1 else ROOT/'target/quota-desktop')
out.mkdir(parents=True,exist_ok=True)
(out/'result.json').unlink(missing_ok=True)
state = {'status':200,'quota':(37,12,'2026-09-22T00:00:00.000Z'),'activation':0,'queries':0}
token = 'synthetic-desktop-token-'+'x'*40
class Backend(BaseHTTPRequestHandler):
    def log_message(self,*args): pass
    def do_POST(self):
        body=json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        if self.path=='/v1/activate':
            assert body['activation_key']=='desktop-test-key'
            state['activation']+=1; status=200; payload={'device_token':token}; quota=None
        elif self.path=='/v1/query':
            assert self.headers.get('Authorization')=='Bearer '+token
            state['queries']+=1; status=state['status']; quota=state['quota']
            payload={'answer':'Synthetic answer','searched':False} if status==200 else {'message':'Please try again later'}
        else:
            self.send_error(404);return
        data=json.dumps(payload).encode(); self.send_response(status)
        self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(data)))
        if quota:
            for name,value in zip(['Limit','Remaining','Reset'],quota):self.send_header('X-FindOut-Daily-'+name,str(value))
        self.end_headers();self.wfile.write(data)
server=ThreadingHTTPServer(('127.0.0.1',0),Backend)
threading.Thread(target=server.serve_forever,daemon=True).start()
env=dict(os.environ,FINDOUT_API_ORIGIN=f'http://127.0.0.1:{server.server_port}')
app=None; wm=None; keyring=None; logs=[]
def command(*args):return sp.check_output(args,text=True,stderr=sp.DEVNULL).strip()
def wait_for(predicate,description):
    until=time.monotonic()+15
    while time.monotonic()<until:
        if app and app.poll() is not None:raise AssertionError('Desktop exited; inspect app log')
        try:
            if predicate():return
        except sp.CalledProcessError:pass
        time.sleep(.2)
    raise AssertionError('Timed out: '+description)
def start():
    global app,window
    log=open(out/f'app-{len(logs)}.log','w');logs.append(log)
    app=sp.Popen([str(ROOT/'target/debug/findout-client')],env=env,stdout=log,stderr=log)
    time.sleep(1)
    command('xdotool','key','--clearmodifiers','super+space')
    def visible():
        global window
        ids=command('xdotool','search','--onlyvisible','--pid',str(app.pid)).splitlines()
        if ids:window=ids[-1];return True
    wait_for(visible,'popup visible');command('xdotool','windowfocus','--sync',window)
def capture(name):
    path=out/(name+'.png');command('import','-window','root',str(path))
    geometry=dict(line.split('=',1) for line in command('xdotool','getwindowgeometry','--shell',window).splitlines())
    crop=f"{geometry['WIDTH']}x{geometry['HEIGHT']}+{geometry['X']}+{geometry['Y']}"
    ocr=out/'ocr.png'
    command('convert',str(path),'-crop',crop,'+repage','-resize','400%',str(ocr))
    text=command('tesseract',str(ocr),'stdout','--psm','11')
    status_crop=f"422x18+{int(geometry['X'])+24}+{int(geometry['Y'])+223}"
    command('magick',str(path),'-crop',status_crop,'+repage','-resize','300%',str(ocr))
    text+='\n'+command('tesseract',str(ocr),'stdout','--psm','7')
    (out/(name+'.txt')).write_text(text);return text
checks=[]
# At 8px, OCR can confuse 0/8 and year digits. Assert distinctive quota and
# day markers here; retained screenshots allow full reset/zero-value inspection.
def screen(name, required=(), forbidden=()):
    def matches():
        text=capture(name).lower()
        return all(s.lower() in text for s in required) and all(s.lower() not in text for s in forbidden)
    wait_for(matches,name);checks.append(name);print('PASS '+name,flush=True)
def enter(text):
    command('xdotool','key','--clearmodifiers','ctrl+a');command('xdotool','type','--clearmodifiers',text);command('xdotool','key','Return')
def ask():
    n=state['queries'];enter('Quota test');wait_for(lambda:state['queries']==n+1,'query reached backend')
try:
    # Explicit service process on a bus with activation disabled; storage is disposable.
    keyring=sp.Popen(['gnome-keyring-daemon','--foreground','--components=secrets','--unlock'],stdin=sp.PIPE,stdout=sp.DEVNULL,stderr=sp.DEVNULL)
    keyring.stdin.write(b'disposable-test-password\n');keyring.stdin.close()
    wm=sp.Popen(['xfwm4','--replace'],stdout=sp.DEVNULL,stderr=sp.DEVNULL)
    time.sleep(1);start()
    screen('01-before-activation',required=['A little space to find out'])
    enter('desktop-test-key');wait_for(lambda:state['activation']==1,'activation request')
    screen('02-activated',required=['FINDOUT'],forbidden=['A little space','activation required','left','37'])
    assert state['queries']==0
    ask();screen('03-current-quota',required=['12/37','-22'])
    state['status']=429;state['quota']=(37,0,'2026-09-22T00:00:00.000Z')
    ask();screen('04-exhausted',required=['daily limit reached','/37'],forbidden=['12/37'])
    app.terminate();app.wait(timeout=5);app=None
    state['status']=200;state['quota']=(9,4,'2026-09-23T00:00:00.000Z')
    start();screen('05-restored',required=['FINDOUT'],forbidden=['activation required','0/37','12/37'])
    assert state['activation']==1 and state['queries']==2
    ask();screen('06-changed-backend-limit',required=['4/9','-23'],forbidden=['37'])
    state['status']=429;state['quota']=None
    ask();screen('07-error-without-quota',required=['please try again later'],forbidden=['4/9','2026-09-23'])
    (out/'result.json').write_text(json.dumps({'passed':checks,'activations':state['activation'],'queries':state['queries'],'environment':'private Xvfb, D-Bus, Secret Service and XDG directories; real desktop process restart'},indent=2)+'\n')
finally:
    for p in [app,wm,keyring]:
        if p and p.poll() is None:
            p.terminate()
            try:p.wait(timeout=5)
            except sp.TimeoutExpired:p.kill();p.wait()
    server.shutdown();server.server_close()
    for log in logs:log.close()
