import { spawn } from 'node:child_process';
import fs from 'node:fs';

const CHROME = '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const PORT = 9335;
const chrome = spawn(CHROME, [
  '--headless=new', `--remote-debugging-port=${PORT}`,
  '--disable-gpu-blocklist', '--enable-unsafe-webgpu', '--use-angle=metal',
  '--no-sandbox', '--user-data-dir=/tmp/dg-chrome-profile3',
  '--window-size=1400,950', 'about:blank',
], { stdio: 'ignore' });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

let wsUrl;
for (let i = 0; i < 40; i++) {
  try {
    const list = await (await fetch(`http://127.0.0.1:${PORT}/json/list`)).json();
    const page = list.find((t) => t.type === 'page');
    if (page) { wsUrl = page.webSocketDebuggerUrl; break; }
  } catch {}
  await sleep(250);
}
const ws = new WebSocket(wsUrl);
await new Promise((r) => { ws.onopen = r; });
let id = 0;
const pending = new Map();
const send = (method, params = {}) => new Promise((res, rej) => {
  const mid = ++id;
  pending.set(mid, { res, rej });
  ws.send(JSON.stringify({ id: mid, method, params }));
});
const errors = [];
ws.onmessage = (ev) => {
  const m = JSON.parse(ev.data);
  if (m.id && pending.has(m.id)) {
    const { res, rej } = pending.get(m.id);
    pending.delete(m.id);
    m.error ? rej(new Error(JSON.stringify(m.error))) : res(m.result);
  }
  if (m.method === 'Runtime.exceptionThrown') {
    errors.push(m.params.exceptionDetails.exception?.description || m.params.exceptionDetails.text);
  }
};
await send('Runtime.enable');
await send('Page.enable');
await send('Page.navigate', { url: process.argv[2] || 'http://localhost:8741/index.html' });
await sleep(3500); // 等首帧渲染

const evalJs = async (expr) => (await send('Runtime.evaluate', { expression: expr, awaitPromise: true, returnByValue: true })).result.value;
const shot = async (name) => {
  const s = await send('Page.captureScreenshot', { format: 'png' });
  fs.writeFileSync(name, Buffer.from(s.data, 'base64'));
};

// 1) 爆炸视图 60%。
await evalJs(`(() => {
  const el = document.getElementById('explode');
  el.value = 60; el.dispatchEvent(new Event('input', { bubbles: true }));
})()`);
await sleep(600);
await shot('/tmp/dg-explode.png');

// 2) 逐层拆装 70%。
await evalJs(`(() => {
  const el = document.getElementById('disassemble');
  el.value = 70; el.dispatchEvent(new Event('input', { bubbles: true }));
})()`);
await sleep(600);
await shot('/tmp/dg-disassemble.png');

// 3) 切换彩画。
await evalJs(`(() => {
  const el = document.getElementById('materialStyle');
  el.value = 'caihua'; el.dispatchEvent(new Event('change', { bubbles: true }));
})()`);
await sleep(1200); // 防抖 + 重建
await shot('/tmp/dg-caihua.png');

// 4) 切八铺作看复杂朵。
await evalJs(`(() => {
  const el = document.getElementById('bracketSet');
  el.value = 'ba'; el.dispatchEvent(new Event('change', { bubbles: true }));
  const d = document.getElementById('disassemble'); d.value = 0; d.dispatchEvent(new Event('input'));
  const e = document.getElementById('explode'); e.value = 0; e.dispatchEvent(new Event('input'));
})()`);
await sleep(1200);
await shot('/tmp/dg-bapu.png');

const stats = await evalJs(`document.getElementById('statRebuild').textContent`);
console.log('rebuild stat:', stats);
console.log('exceptions:', errors.length ? errors.join('\n') : 'none');
ws.close(); chrome.kill();
process.exit(errors.length ? 1 : 0);
