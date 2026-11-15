// 轻量 CDP 驱动：启动 headless Chrome，打开测试页，收集控制台/异常/页面结果并截图。
// 仅用 Node 内置能力（fetch + WebSocket），无第三方依赖。
import { spawn } from 'node:child_process';
import fs from 'node:fs';

const CHROME = '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const URL = process.argv[2] || 'http://localhost:8741/test.html';
const SHOT = process.argv[3] || '/tmp/dg-test.png';
const PORT = 9333;

const chrome = spawn(CHROME, [
  '--headless=new',
  `--remote-debugging-port=${PORT}`,
  '--disable-gpu-blocklist',
  '--enable-unsafe-webgpu',
  '--use-angle=metal',
  '--no-sandbox',
  '--no-first-run',
  '--user-data-dir=/tmp/dg-chrome-profile',
  '--window-size=1280,900',
  'about:blank',
], { stdio: ['ignore', 'ignore', 'pipe'] });

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function getWsUrl() {
  for (let i = 0; i < 40; i++) {
    try {
      const r = await fetch(`http://127.0.0.1:${PORT}/json/list`);
      const list = await r.json();
      const page = list.find((t) => t.type === 'page' && t.webSocketDebuggerUrl);
      if (page) return page.webSocketDebuggerUrl;
    } catch {
      await sleep(250);
    }
  }
  throw new Error('Chrome DevTools 未就绪');
}

const wsUrl = await getWsUrl();
const ws = new WebSocket(wsUrl);
await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });

let id = 0;
const pending = new Map();
const consoleLogs = [];
const exceptions = [];

function send(method, params = {}) {
  return new Promise((resolve, reject) => {
    const mid = ++id;
    pending.set(mid, { resolve, reject });
    ws.send(JSON.stringify({ id: mid, method, params }));
  });
}

ws.onmessage = (ev) => {
  const msg = JSON.parse(ev.data);
  if (msg.id && pending.has(msg.id)) {
    const { resolve, reject } = pending.get(msg.id);
    pending.delete(msg.id);
    msg.error ? reject(new Error(JSON.stringify(msg.error))) : resolve(msg.result);
    return;
  }
  if (msg.method === 'Runtime.consoleAPICalled') {
    const text = msg.params.args.map((a) => a.value ?? a.description ?? a.type).join(' ');
    consoleLogs.push(`[${msg.params.type}] ${text}`);
  }
  if (msg.method === 'Runtime.exceptionThrown') {
    exceptions.push(msg.params.exceptionDetails.exception?.description || msg.params.exceptionDetails.text);
  }
};

await send('Runtime.enable');
await send('Page.enable');
await send('Log.enable');
await send('Page.navigate', { url: URL });

// 轮询页面标题（测试页完成时把 PASS/FAIL 写入 document.title）。
const deadline = Date.now() + 45000;
let status = 'TIMEOUT';
let dom = '';
while (Date.now() < deadline) {
  await sleep(500);
  const r = await send('Runtime.evaluate', { expression: 'document.title', returnByValue: true });
  status = r.result.value || 'RUNNING';
  if (status === 'PASS' || status === 'FAIL') {
    const d = await send('Runtime.evaluate', {
      expression: 'document.getElementById("result").textContent', returnByValue: true,
    });
    dom = d.result.value;
    break;
  }
}

// 截图（等一帧确保画面已上屏）。
await sleep(800);
const shot = await send('Page.captureScreenshot', { format: 'png' });
fs.writeFileSync(SHOT, Buffer.from(shot.data, 'base64'));

console.log('=== STATUS:', status, '===');
console.log(dom);
console.log('=== CONSOLE (' + consoleLogs.length + ') ===');
console.log(consoleLogs.join('\n') || '(none)');
console.log('=== EXCEPTIONS (' + exceptions.length + ') ===');
console.log(exceptions.join('\n') || '(none)');
console.log('=== SCREENSHOT:', SHOT, '===');

ws.close();
chrome.kill();
process.exit(status === 'PASS' && exceptions.length === 0 ? 0 : 1);
