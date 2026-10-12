#!/usr/bin/env node
// Visual reference for PDA-T-0041: replays the golden timeline of harness.mjs in the REAL
// design-system/reference/organism.html script inside headless Chrome and saves one
// 640x640 PNG per frame (text suppressed: the app draws labels with GPUI, not the raster).
//
//   node chrome_capture.mjs <out-dir> [frame ...]      default frames: 30 120 210 300 450 600
//   CHROME=/path/to/chrome node chrome_capture.mjs ...
//
// The Rust side (`cargo test -p app --release visual -- --ignored`) renders the same
// frames with tiny-skia and compares them with SSIM.

import fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const HTML = path.resolve(here, '../../../design-system/reference/organism.html');
const TOKENS = path.resolve(here, '../../../design-system/tokens.json');
const GOLDEN = path.resolve(here, '../tests/golden/organism.json');
const out = process.argv[2];
if (!out) { console.error('usage: chrome_capture.mjs <out-dir> [frame ...]'); process.exit(2); }
const frames = process.argv.slice(3).map(Number);
if (!frames.length) frames.push(30, 120, 210, 300, 450, 600);
const chrome = process.env.CHROME || 'google-chrome';
fs.mkdirSync(out, { recursive: true });

const golden = JSON.parse(fs.readFileSync(GOLDEN, 'utf8'));
const tokens = fs.readFileSync(TOKENS, 'utf8');
const ref = fs.readFileSync(HTML, 'utf8');
let src = /<script>([\s\S]*?)<\/script>/.exec(ref)[1];
src = src.slice(0, src.indexOf('(function loop(now)'));

const DRIVER = `
;globalThis.__h = (() => { let sn = 0; return {
  setState(s) { state = s; }, setMouse(m) { mouse = m; }, select(id) { sel = id; },
  addAgent(a) { agents.push(a); },
  spawn() { sn++; agents.push({ id: 'cell-' + sn, engine: ['pando','acp','external','cli'][sn % 4], status: 'working', isNew: true }); state = 'delegating'; },
  kill() { agents.pop(); }, useTokens(j) { T = j; },
  frame(dt) { S.t += dt; step(dt); draw(); } }; })();`;

const page = (frame) => `<!doctype html><html><body style="margin:0;background:#0F2A20">
<canvas id="c" width="640" height="640" style="display:block;width:640px;height:640px"></canvas>
<div id="states"></div><button id="spawn"></button><button id="kill"></button>
<script>
function mulberry32(a){return function(){a|=0;a=(a+0x6d2b79f5)|0;let t=Math.imul(a^(a>>>15),1|a);t=(t+Math.imul(t^(t>>>7),61|t))^t;return((t^(t>>>14))>>>0)/4294967296;};}
Math.random = mulberry32(${golden.seed});
window.addEventListener = () => {};
window.fetch = () => Promise.reject(new Error('offline'));
CanvasRenderingContext2D.prototype.fillText = function () {};
${src}
${DRIVER}
const h = __h; h.useTokens(${tokens});
h.addAgent({ id: 'docs-sync', engine: 'acp', status: 'paused' });
const timeline = ${JSON.stringify(golden.timeline)};
for (let f = 0; f <= ${frame}; f++) {
  for (const e of timeline.filter((e) => e.f === f)) {
    if (e.op === 'state') h.setState(e.value);
    else if (e.op === 'pointer') h.setMouse(e.value ? { x: e.value[0], y: e.value[1] } : null);
    else if (e.op === 'spawn') h.spawn();
    else if (e.op === 'kill') h.kill();
    else if (e.op === 'select') h.select(e.id);
  }
  h.frame(1 / 60);
}
</script></body></html>`;

for (const f of frames) {
  const html = path.join(out, `frame-${f}.html`);
  fs.writeFileSync(html, page(f));
  execFileSync(chrome, ['--headless=new', '--no-sandbox', '--disable-gpu', '--hide-scrollbars',
    '--force-device-scale-factor=1', '--window-size=640,640', `--screenshot=${path.join(out, `chrome-${f}.png`)}`,
    'file://' + html], { stdio: 'ignore' });
  console.log('captured frame', f);
}
