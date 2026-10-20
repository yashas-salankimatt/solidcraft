// Browser check of the `instance` web build in headless Chrome over CDP (see
// docs/agent-handoff-instance-portal.md §7): a desktop pointer (touch mode stays off, shortcut
// presets work with real keys) and an emulated touch tablet (touch mode on, taps, flyouts,
// selection bar, pinch zoom). Node 22+ and Google Chrome (macOS path below; change CHROME).
// Usage: node docs/agent-handoff-instance-web-check.mjs http://127.0.0.1:8793/ /tmp/sc-web-out
import { spawn } from 'node:child_process';
import { mkdtempSync, readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const CHROME = process.env.CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const url = process.argv[2] || 'http://127.0.0.1:8793/';
const out = process.argv[3] || '/tmp/sc-web/out';
mkdirSync(out, { recursive: true });
const profile = mkdtempSync(join(tmpdir(), 'sc-chrome-'));
const chrome = spawn(CHROME, [
  '--headless=new', `--user-data-dir=${profile}`, '--remote-debugging-port=0', '--no-first-run',
  '--no-default-browser-check', '--enable-unsafe-webgpu', 'about:blank',
], { stdio: 'ignore' });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const results = [];
const check = (ok, what, detail = '') => {
  results.push({ ok: !!ok, what, detail });
  console.log(`${ok ? 'PASS' : 'FAIL'} ${what}${detail ? ' — ' + detail : ''}`);
};

let port;
for (let i = 0; i < 300 && !port; i++) {
  try { port = parseInt(readFileSync(join(profile, 'DevToolsActivePort'), 'utf8').split('\n')[0], 10); } catch { await sleep(100); }
}
const list = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
const ws = new WebSocket(list.find((t) => t.type === 'page').webSocketDebuggerUrl);
await new Promise((r) => (ws.onopen = r));
let next = 0;
const pending = new Map();
const logs = [];
ws.onmessage = (e) => {
  const m = JSON.parse(e.data);
  if (m.id && pending.has(m.id)) {
    const { res, rej } = pending.get(m.id);
    pending.delete(m.id);
    m.error ? rej(new Error(JSON.stringify(m.error))) : res(m.result);
  } else if (m.method === 'Page.javascriptDialogOpening') {
    // The unsaved design's "leave this page?" prompt: leave.
    logs.push('dialog: ' + m.params.type);
    ws.send(JSON.stringify({ id: ++next, method: 'Page.handleJavaScriptDialog', params: { accept: true } }));
  } else if (m.method === 'Runtime.exceptionThrown') {
    logs.push('exception: ' + (m.params.exceptionDetails.exception?.description || m.params.exceptionDetails.text));
  } else if (m.method === 'Runtime.consoleAPICalled' && ['error', 'warning'].includes(m.params.type)) {
    logs.push(m.params.type + ': ' + m.params.args.map((a) => a.value ?? a.description).join(' '));
  }
};
const call = (method, params = {}) => new Promise((res, rej) => {
  const id = ++next;
  pending.set(id, { res, rej });
  ws.send(JSON.stringify({ id, method, params }));
});
const evaluate = async (expression) => {
  const r = await call('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
  if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description || r.exceptionDetails.text);
  return r.result.value;
};
const req = (method, params = {}) => evaluate(`window.solidcraft.request(${JSON.stringify(method)}, ${JSON.stringify(params)})`);
const exec = (command, params = {}) => evaluate(`window.solidcraft.execute(${JSON.stringify(command)}, ${JSON.stringify(params)})`);
const frames = (n = 10) => evaluate(`new Promise(r => { let k = ${n}; const f = () => (--k > 0 ? requestAnimationFrame(f) : r(true)); requestAnimationFrame(f); })`);
const shot = async (name) => {
  const { data } = await call('Page.captureScreenshot', { format: 'png' });
  writeFileSync(join(out, name), Buffer.from(data, 'base64'));
};
const load = async () => {
  await call('Page.navigate', { url });
  for (let i = 0; i < 600; i++) {
    const ready = await evaluate(`!!window.solidcraft && !document.getElementById('solidcraft_loading')`).catch(() => false);
    if (ready) return true;
    await sleep(200);
  }
  return false;
};
const handles = async (prefix) => (await req('ui.handles', { prefix })) ?? [];
const names = (hs) => hs.map((h) => h.name);
// Points to CSS pixels: the UI scale, times 1.2 in touch mode (deviceScaleFactor is 1).
const css = async (touch) => {
  const p = await req('ui.prefs', {});
  return (p.ui_scale ?? 1) * (touch ? 1.2 : 1);
};

try {
  await call('Runtime.enable');
  await call('Page.enable');

  // ---- Desktop pointer ----
  await call('Emulation.setDeviceMetricsOverride', { width: 1280, height: 860, deviceScaleFactor: 1, mobile: false });
  check(await load(), 'desktop: the app loads');
  check(await evaluate(`!!navigator.gpu`), 'desktop: WebGPU available');
  await req('ui.home', { open: false });
  await frames(10);
  const inspect = await req('ui.inspect', {});
  check(inspect.renderer === 'gpu', 'desktop: viewport renders on the GPU', inspect.renderer);
  const prefs = await req('ui.prefs', {});
  check(prefs.touch_mode === 'auto', 'desktop: touch mode defaults to automatic', prefs.touch_mode);
  check((await handles('touch:')).length === 0, 'desktop: touch mode stays off with a mouse');
  check(!(JSON.stringify(await evaluate('window.solidcraft.commands()')).includes('geometry.provider')), 'desktop: no geometry provider commands');

  // Presets through the command bridge.
  for (const [preset, nav] of [['solidworks', 'solidworks'], ['onshape', 'onshape'], ['fusion', 'fusion']]) {
    const r = await req('ui.shortcut', { preset });
    const p = await req('ui.prefs', {});
    check(r.preset === preset && p.nav === nav, `preset ${preset}: keys and mouse`, `reply ${JSON.stringify(r)} nav ${p.nav}`);
  }

  // SolidWorks style: Cmd/Ctrl+1 is the front view, typed on the real keyboard.
  await req('ui.shortcut', { preset: 'solidworks' });
  await exec('solid.box', { length: 40, width: 30, height: 20 });
  await req('ui.camera', { yaw: 0.9, pitch: 0.6 });
  await frames(5);
  const vp = (await req('ui.inspect', {})).viewport;
  const [cx, cy] = [(vp[0] + vp[2] / 2), (vp[1] + vp[3] / 2)];
  await call('Input.dispatchMouseEvent', { type: 'mouseMoved', x: cx, y: cy });
  const isMac = await evaluate(`/Mac/.test(navigator.platform)`);
  const mod = isMac ? 4 : 2; // Meta on macOS, Ctrl elsewhere
  await call('Input.dispatchKeyEvent', { type: 'rawKeyDown', modifiers: mod, key: '1', code: 'Digit1', windowsVirtualKeyCode: 49 });
  await call('Input.dispatchKeyEvent', { type: 'keyUp', modifiers: mod, key: '1', code: 'Digit1', windowsVirtualKeyCode: 49 });
  await frames(60);
  const cam = (await req('ui.camera', {}));
  check(Math.abs(cam.yaw) < 1e-3 && Math.abs(cam.pitch) < 1e-3, `solidworks: ${isMac ? 'Cmd' : 'Ctrl'}+1 turns to the front view`, `yaw ${cam.yaw?.toFixed?.(3)} pitch ${cam.pitch?.toFixed?.(3)}`);
  await req('ui.prefs', { open: 'Keyboard Shortcuts' });
  await frames(10);
  await shot('desktop-shortcut-presets.png');
  await req('ui.prefs', { button: 'cancel' });
  await req('ui.shortcut', { preset: 'solidcraft' });

  // ---- Touch tablet: a fresh start (no recovery offer from the desktop phase's design) ----
  await call('Page.navigate', { url: 'about:blank' });
  await sleep(1500);
  await call('Storage.clearDataForOrigin', { origin: new URL(url).origin, storageTypes: 'all' });
  await call('Emulation.setDeviceMetricsOverride', { width: 1180, height: 820, deviceScaleFactor: 1, mobile: false });
  await call('Emulation.setTouchEmulationEnabled', { enabled: true, maxTouchPoints: 5 });
  check(await load(), 'touch: the app loads');
  check(await evaluate(`matchMedia('(pointer: coarse)').matches`), 'touch: the page sees a coarse pointer');
  await req('ui.home', { open: false });
  await frames(10);
  let touch = names(await handles('touch:'));
  check(['touch:tab', 'touch:undo', 'touch:redo'].every((n) => touch.includes(n)) && touch.some((n) => n.startsWith('touch:panel:')),
    'touch: the tool column is shown', touch.slice(0, 8).join(', '));
  const scale = await css(true);
  const at = async (name) => {
    const h = (await handles(name)).find((x) => x.name === name);
    return h && { x: h.x * scale, y: h.y * scale };
  };
  const tap = async (p) => {
    await call('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ x: p.x, y: p.y, id: 1 }] });
    await frames(2);
    await call('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
    await frames(6);
  };

  await exec('solid.box', { length: 40, width: 30, height: 20 });
  await req('ui.camera', { fit: true });
  await frames(20);
  const tv = (await req('ui.inspect', {})).viewport;
  const centre = { x: (tv[0] + tv[2] / 2) * scale, y: (tv[1] + tv[3] / 2) * scale };
  await tap(centre);
  const sel = names(await handles('touch:sel:'));
  check(sel.length > 1 && sel.includes('touch:sel:more'), 'touch: a tap on the box selects it and shows the selection bar', sel.join(', '));
  await shot('touch-selection-bar.png');

  // A panel opens its tools; a tap on one starts it.
  const panel = touch.find((n) => n.startsWith('touch:panel:'));
  await tap(await at(panel));
  const rows = names(await handles('touch:row:'));
  check(rows.length > 0, `touch: ${panel} opens its tools`, rows.slice(0, 5).join(', '));
  await shot('touch-flyout.png');
  if (rows.length) {
    await tap(await at(rows[0]));
    // A tool, its dialog, or (Create Sketch with a face selected) the command done at once.
    const { tool, dialog } = await req('ui.inspect', {});
    const sketch = await exec('sketch.inspect', {}).catch((e) => null);
    check(tool || dialog || sketch?.name, `touch: tapping ${rows[0]} starts it`,
      `tool ${tool} dialog ${dialog ? dialog.split(' ')[3] : null} open sketch ${sketch?.name}`);
    await req('ui.key', { key: 'Escape' });
    await frames(5);
  }

  // Two fingers pinch apart: the view zooms in.
  const before = (await req('ui.camera', {})).distance;
  const pts = (d) => [{ x: centre.x - d, y: centre.y, id: 1 }, { x: centre.x + d, y: centre.y, id: 2 }];
  await call('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: pts(40) });
  await frames(2);
  for (let d = 50; d <= 160; d += 10) {
    await call('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: pts(d) });
    await frames(1);
  }
  await call('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
  await frames(5);
  const after = (await req('ui.camera', {})).distance;
  check(after < before * 0.8, 'touch: a pinch zooms in', `distance ${before?.toFixed?.(1)} → ${after?.toFixed?.(1)}`);
  await shot('touch-after-pinch.png');
} catch (e) {
  check(false, 'script', e.stack || String(e));
} finally {
  if (logs.length) console.log('page log:\n  ' + logs.slice(0, 20).join('\n  '));
  writeFileSync(join(out, 'results.json'), JSON.stringify({ results, logs }, null, 2));
  const failed = results.filter((r) => !r.ok).length;
  console.log(`${results.length - failed}/${results.length} passed`);
  ws.close();
  chrome.kill();
  process.exit(failed ? 1 : 0);
}
