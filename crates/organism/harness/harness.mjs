#!/usr/bin/env node
// Golden harness for the organism port (PDA-T-0031).
//
// Loads the <script> of design-system/reference/organism.html into a vm context with
// stubbed document/canvas/fetch/requestAnimationFrame, replaces Math.random with
// mulberry32(seed), replays a scripted timeline at a fixed dt of 1/60 and records, for
// selected frames, the simulation state and the canvas draw calls in a canonical form.
//
//   node harness.mjs            write ../tests/golden/organism.json
//   node harness.mjs --check    regenerate and fail if the committed golden differs
//
// No dependencies: Node >= 18.

import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const HTML = path.resolve(here, '../../../design-system/reference/organism.html');
const TOKENS = path.resolve(here, '../../../design-system/tokens.json');
const OUT = path.resolve(here, '../tests/golden/organism.json');

const SEED = 1;
const DT = 1 / 60;
const FRAMES = [0, 1, 30, 120, 210, 300, 450, 600];
const LAST = Math.max(...FRAMES);
const ROUND = 1e9;
const r9 = (x) => (typeof x === 'number' ? Math.round(x * ROUND) / ROUND : x);

// Same generator as crates/organism/src/rng.rs.
function mulberry32(a) {
  return function () {
    a |= 0;
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function parseColor(c) {
  if (c[0] === '#') {
    const n = parseInt(c.slice(1), 16);
    return [(n >> 16) & 255, (n >> 8) & 255, n & 255, 1];
  }
  const m = /^rgba\(([^)]*)\)$/.exec(c);
  if (!m) throw new Error('unsupported colour ' + c);
  const p = m[1].split(',').map(Number);
  return [p[0], p[1], p[2], p[3]];
}

// Recording 2D context: emits canonical events instead of pixels.
function makeCtx(events) {
  const st = { fillStyle: '#000000', strokeStyle: '#000000', lineWidth: 1, globalAlpha: 1, lineCap: 'butt',
    lineDashOffset: 0, font: '10px sans-serif', textAlign: 'start', dash: [] };
  const stack = [];
  let clip = false;
  let path = null;
  const col = (c) => { const k = parseColor(c); return [k[0], k[1], k[2], k[3] * st.globalAlpha]; };
  const geom = () => {
    if (!path) return null;
    if (path.arc) return { t: 'circle', c: path.arc.c, r: path.arc.r };
    if (path.curve) return { t: 'curve', pts: path.pts };
    return { t: 'poly', pts: path.pts, closed: path.closed };
  };
  const paint = (k) => {
    const g = geom();
    if (!g) return;
    const e = { k, g, color: col(k === 'fill' ? st.fillStyle : st.strokeStyle), clip };
    if (k === 'stroke') { e.w = st.lineWidth; e.dash = st.dash.slice(); e.off = st.lineDashOffset; e.cap = st.lineCap; }
    events.push(e);
  };
  const ctx = {
    setTransform() {}, clearRect() {},
    beginPath() { path = { pts: [], closed: false, arc: null, curve: false }; },
    moveTo(x, y) { path.pts.push([x, y]); },
    lineTo(x, y) { path.pts.push([x, y]); },
    quadraticCurveTo(cx, cy) { path.curve = true; path.pts.push([cx, cy]); },
    closePath() { path.closed = true; },
    arc(x, y, r) { path.arc = { c: [x, y], r }; },
    stroke() { paint('stroke'); }, fill() { paint('fill'); },
    save() { stack.push({ ...st, dash: st.dash.slice(), clip }); },
    restore() { const s = stack.pop(); Object.assign(st, s); clip = s.clip; },
    clip() { clip = true; },
    setLineDash(d) { st.dash = d.slice(); },
    fillText(text, x, y) {
      const m = /^(\d+) (\d+)px/.exec(st.font);
      events.push({ k: 'text', text, x, y, weight: Number(m[1]), size: Number(m[2]), color: col(st.fillStyle), align: st.textAlign });
    },
  };
  return new Proxy(ctx, {
    get: (t, p) => (p in t ? t[p] : st[p]),
    set: (t, p, v) => { st[p] = v; return true; },
  });
}

// What the appended driver exposes inside the reference script's scope.
const DRIVER = `
;globalThis.__h = (() => {
  let sn = 0;
  return {
    setState(s) { state = s; },
    setMouse(m) { mouse = m; },
    select(id) { sel = id; },
    addAgent(a) { agents.push(a); },
    spawn() { sn++; const a = { id: 'cell-' + sn, engine: ['pando', 'acp', 'external', 'cli'][sn % 4], status: 'working', isNew: true };
      agents.push(a); state = 'delegating'; return a; },
    kill() { agents.pop(); },
    useTokens(j) { T = j; },
    cellPos(id) { const c = S.cells[id]; return c ? { x: c.x, y: c.y } : null; },
    frame(dt) { S.t += dt; step(dt); draw(); },
    snap() {
      return {
        t: S.t, amp: S.amp, ampT: S.ampT, nx: S.nx, ny: S.ny, state, sel,
        waves: S.waves.map((w) => ({ r: w.r, life: w.life })),
        particles: S.particles.map((p) => ({ a: p.a, r: p.r, s: p.s, sz: p.sz })),
        cells: Object.values(S.cells).map((c) => ({ id: c.a.id, x: c.x, y: c.y, vx: c.vx, vy: c.vy, born: c.born,
          phase: c.phase, hover: c.hover, flow: c.flow.slice(), ang: c.ang })),
      };
    },
  };
})();
`;

function loadReference() {
  const html = fs.readFileSync(HTML, 'utf8');
  const m = /<script>([\s\S]*?)<\/script>/.exec(html);
  if (!m) throw new Error('no <script> in organism.html');
  let src = m[1];
  // Drop the rAF loop and the DOM interaction section; the harness drives frames itself.
  const cut = src.indexOf('(function loop(now)');
  if (cut < 0) throw new Error('reference layout changed: loop IIFE not found');
  src = src.slice(0, cut);
  return src;
}

function run() {
  const events = [];
  const canvas = {
    clientWidth: 640, width: 640, height: 640,
    getContext: () => makeCtx(events),
    addEventListener() {}, getBoundingClientRect: () => ({ left: 0, top: 0, width: 640, height: 640 }),
  };
  const sandbox = {
    document: { getElementById: () => canvas, createElement: () => ({ style: {}, appendChild() {} }) },
    addEventListener() {}, requestAnimationFrame() {}, devicePixelRatio: 1,
    performance: { now: () => 0 },
    fetch: () => Promise.reject(new Error('offline')), console,
  };
  const context = vm.createContext(sandbox);
  vm.runInContext('Math.random = __rand', Object.assign(context, { __rand: mulberry32(SEED) }));
  vm.runInContext(loadReference() + DRIVER, context, { filename: 'organism.html' });
  const h = context.__h;
  h.useTokens(JSON.parse(fs.readFileSync(TOKENS, 'utf8')));

  const timeline = [];
  const emit = (f, e) => timeline.push({ f, ...e });
  const initial = { id: 'docs-sync', engine: 'acp', status: 'paused' };
  h.addAgent(initial);

  const at = {};
  const on = (f, fn) => { (at[f] ||= []).push(fn); };
  on(0, () => emit(0, { op: 'state', value: 'idle' }));
  on(20, () => { h.setState('listening'); emit(20, { op: 'state', value: 'listening' }); });
  for (let f = 10; f < 60; f++) {
    on(f, () => { const p = [r9(320 + 150 * Math.cos(0.1 * f)), r9(320 + 150 * Math.sin(0.1 * f))];
      h.setMouse({ x: p[0], y: p[1] }); emit(f, { op: 'pointer', value: p }); });
  }
  on(90, () => { h.setState('thinking'); emit(90, { op: 'state', value: 'thinking' }); });
  for (const f of [100, 110]) {
    on(f, () => { const c = h.cellPos('refactor-gpui'); const p = [r9(c.x), r9(c.y)];
      h.setMouse({ x: p[0], y: p[1] }); emit(f, { op: 'pointer', value: p }); });
  }
  on(140, () => { h.setMouse(null); emit(140, { op: 'pointer', value: null }); });
  on(200, () => { const a = h.spawn(); emit(200, { op: 'spawn', agent: a }); emit(200, { op: 'state', value: 'delegating' }); });
  on(240, () => { h.setState('speaking'); emit(240, { op: 'state', value: 'speaking' }); });
  on(330, () => { const a = h.spawn(); emit(330, { op: 'spawn', agent: a }); emit(330, { op: 'state', value: 'delegating' }); });
  on(345, () => { h.setState('speaking'); emit(345, { op: 'state', value: 'speaking' }); });
  on(380, () => { h.kill(); emit(380, { op: 'kill' }); });
  on(420, () => { h.setState('idle'); emit(420, { op: 'state', value: 'idle' }); });
  on(500, () => { const a = h.spawn(); emit(500, { op: 'spawn', agent: a }); emit(500, { op: 'state', value: 'delegating' }); });
  on(540, () => { h.select('cell-3'); emit(540, { op: 'select', id: 'cell-3' }); });

  const frames = [];
  for (let f = 0; f <= LAST; f++) {
    for (const fn of at[f] || []) fn();
    events.length = 0;
    h.frame(DT);
    if (FRAMES.includes(f)) {
      frames.push({ f, state: h.snap(), ops: events.map(roundEvent) });
    }
  }
  const out = {
    note: 'Generated by crates/organism/harness/harness.mjs from design-system/reference/organism.html. Do not edit.',
    seed: SEED, fps: 60, width: 640, frames_recorded: FRAMES,
    initialSelected: 'migracion-auth', initialAgents: JSON.parse(vm.runInContext('JSON.stringify(agents)', context))
      .filter((a) => !a.isNew && a.id !== initial.id).concat([initial]),
    timeline, frames,
  };
  return JSON.stringify(deepRound(out), null, 0) + '\n';
}

function roundEvent(e) { return deepRound(e); }
function deepRound(v) {
  if (Array.isArray(v)) return v.map(deepRound);
  if (v && typeof v === 'object') return Object.fromEntries(Object.entries(v).map(([k, x]) => [k, deepRound(x)]));
  return r9(v);
}

const a = run();
const b = run();
if (a !== b) { console.error('harness is not deterministic: two runs differ'); process.exit(2); }
if (process.argv.includes('--check')) {
  const cur = fs.existsSync(OUT) ? fs.readFileSync(OUT, 'utf8') : '';
  if (cur !== a) { console.error('golden is stale: run `node crates/organism/harness/harness.mjs`'); process.exit(1); }
  console.log('golden up to date (' + a.length + ' bytes)');
} else {
  fs.mkdirSync(path.dirname(OUT), { recursive: true });
  fs.writeFileSync(OUT, a);
  console.log('wrote ' + OUT + ' (' + a.length + ' bytes)');
}
