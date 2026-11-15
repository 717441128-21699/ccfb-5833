import { spawn } from 'node:child_process';
import fs from 'node:fs';

const CHROME = '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const PORT = 9334;
const chrome = spawn(CHROME, [
  '--headless=new', `--remote-debugging-port=${PORT}`,
  '--disable-gpu-blocklist', '--enable-unsafe-webgpu', '--use-angle=metal',
  '--no-sandbox', '--user-data-dir=/tmp/dg-chrome-profile2',
  'about:blank',
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
ws.onmessage = (ev) => {
  const m = JSON.parse(ev.data);
  if (m.id && pending.has(m.id)) {
    const { res, rej } = pending.get(m.id);
    pending.delete(m.id);
    m.error ? rej(new Error(JSON.stringify(m.error))) : res(m.result);
  }
};
await send('Page.enable');
await send('Page.navigate', { url: process.argv[2] });
let body = '';
for (let i = 0; i < 60; i++) {
  await sleep(500);
  const t = await send('Runtime.evaluate', { expression: 'document.title', returnByValue: true });
  if ((t.result.value || '').startsWith('DIAG')) {
    const b = await send('Runtime.evaluate', { expression: 'document.body.textContent', returnByValue: true });
    body = b.result.value;
    break;
  }
}
console.log(body);
ws.close(); chrome.kill();
