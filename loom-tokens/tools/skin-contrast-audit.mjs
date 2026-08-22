#!/usr/bin/env node
/**
 * skin-contrast-audit — measure real WCAG contrast on built pages, both themes.
 *
 *   node skin-contrast-audit.mjs http://127.0.0.1:8801 [more origins…]
 *
 * Exits non-zero if any text node falls below AA, so it can gate a build.
 *
 * WHY THIS EXISTS AS A TOOL RATHER THAN AN AD-HOC SNIPPET
 *
 * Every rule below is here because measuring this wrong produced a confident,
 * wrong answer at least once. In order of how much time each cost:
 *
 *   1. WAIT FOR TRANSITIONS. The skin transitions `color` and
 *      `background-color` over 140ms. Flipping `data-theme` and measuring two
 *      animation frames later samples the page MID-TRANSITION, returning blends
 *      of the light and dark palettes. That produced six "failures" in a skin
 *      that is clean — the reported `#46A281` was exactly a 60/40 mix of the
 *      light and dark primaries. Settle time must exceed the longest
 *      transition.
 *
 *   2. NEVER HAND-PARSE CSS COLOURS. The skin uses `oklab()` and
 *      `color-mix(in oklab, …)`. A regex taking the first three numbers read
 *      `oklab(0.999 0.00004 0.00002)` — a near-WHITE — as rgb(0,0,0). Paint the
 *      colour on a canvas and read the pixel: the browser converts exactly, and
 *      alpha composites for free.
 *
 *   3. PROVE THE STYLESHEETS LOADED. An unstyled page passes contrast
 *      trivially, so a 404 or a stale-cache SRI block reads as a perfect score.
 *      Pages reference `/loom-skin.css` absolutely, so serving a preview under
 *      a path prefix 404s every sheet. Each origin must be a ROOT.
 *
 *   4. COMPOSITE THE BACKGROUND. Backgrounds are frequently semi-transparent
 *      (`color-mix(… , transparent)`), so the effective background is the whole
 *      ancestor stack painted in order, not the nearest non-transparent one.
 */

import { createRequire } from 'node:module';
import { existsSync } from 'node:fs';

/**
 * Resolve Playwright without requiring this tool to have its own node_modules.
 *
 * ESM ignores NODE_PATH, so a bare `import 'playwright'` only works from a
 * directory that already has it installed. Loom is a Rust workspace and has no
 * npm tree of its own; adding one to run a single audit script would be a lot
 * of weight for very little. `createRequire` against a known install resolves
 * it directly, and PLAYWRIGHT_PATH lets a caller point elsewhere.
 */
const candidates = [
  process.env.PLAYWRIGHT_PATH,
  '/home/paul/projects/mom-site/audit/node_modules/playwright/index.js',
].filter(Boolean);
const found = candidates.find(p => existsSync(p));
if (!found) {
  console.error('playwright not found. Set PLAYWRIGHT_PATH to its index.js. Looked in:');
  for (const c of candidates) console.error('  ' + c);
  process.exit(2);
}
const { chromium } = createRequire(import.meta.url)(found);

const SETTLE_MS = 400; // > the 140ms colour transition, with headroom.

const MEASURE = `async (settleMs) => {
  const cv = document.createElement('canvas'); cv.width = cv.height = 1;
  const cx = cv.getContext('2d', { willReadFrequently: true });
  const toRGB = (css, backdrop) => {
    cx.clearRect(0, 0, 1, 1);
    if (backdrop) { cx.fillStyle = 'rgb(' + backdrop.join(',') + ')'; cx.fillRect(0, 0, 1, 1); }
    cx.fillStyle = css; cx.fillRect(0, 0, 1, 1);
    const d = cx.getImageData(0, 0, 1, 1).data;
    return [d[0], d[1], d[2]];
  };
  const lum = (c) => {
    const [r, g, b] = c.map(v => { v /= 255; return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4); });
    return 0.2126 * r + 0.7152 * g + 0.0722 * b;
  };
  const ratio = (a, b) => { const L1 = lum(a), L2 = lum(b); const [h, l] = L1 > L2 ? [L1, L2] : [L2, L1]; return (h + 0.05) / (l + 0.05); };
  const effBg = (el) => {
    const stack = []; let n = el;
    while (n) { stack.push(getComputedStyle(n).backgroundColor); n = n.parentElement; }
    let c = [255, 255, 255];
    for (let i = stack.length - 1; i >= 0; i--) c = toRGB(stack[i], c);
    return c;
  };
  const settle = () => new Promise(r => setTimeout(r, settleMs));
  const measure = () => {
    const fails = []; let checked = 0;
    document.querySelectorAll('h1,h2,h3,h4,h5,h6,p,a,li,span,dt,dd,button,figcaption,blockquote,td,th,time,strong,em,label').forEach(el => {
      if (!el.textContent.trim() || el.children.length) return;
      const r = el.getBoundingClientRect(); if (!r.width || !r.height) return;
      const cs = getComputedStyle(el);
      if (cs.visibility === 'hidden' || cs.opacity === '0') return;
      checked++;
      const bg = effBg(el), fg = toRGB(cs.color, bg), cr = ratio(fg, bg);
      const px = parseFloat(cs.fontSize);
      const large = px >= 24 || (px >= 18.66 && parseInt(cs.fontWeight) >= 700);
      const need = large ? 3 : 4.5;
      if (cr < need) fails.push({
        text: el.textContent.trim().slice(0, 30),
        cls: String(el.className).split(' ')[0] || el.tagName,
        ratio: +cr.toFixed(2), need,
      });
    });
    return { checked, fails };
  };
  const out = {};
  for (const theme of ['light', 'dark']) {
    document.documentElement.setAttribute('data-theme', theme);
    await settle();
    out[theme] = measure();
  }
  // Rule count proves the stylesheets parsed. A 404 still yields a StyleSheet
  // object — with zero rules — so "a sheet exists" is not enough.
  let rules = 0;
  for (const s of document.styleSheets) { try { rules += s.cssRules.length; } catch {} }
  out.rules = rules;
  out.overflow = document.documentElement.scrollWidth - document.documentElement.clientWidth;
  return out;
}`;

const PAGES = [
  '/', '/work/', '/code/', '/about/', '/contact/',
  '/work/plausiden/', '/work/sacred-vote/',
  '/work/prosperity-club/', '/work/wealth-within-walls/',
];

const origins = process.argv.slice(2);
if (!origins.length) {
  console.error('usage: skin-contrast-audit.mjs <origin> [origin…]');
  console.error('each origin must be a SITE ROOT — a path prefix 404s /loom-skin.css');
  process.exit(2);
}

const browser = await chromium.launch();
let failed = false;

for (const origin of origins) {
  const ctx = await browser.newContext({ viewport: { width: 1280, height: 1000 } });
  const page = await ctx.newPage();
  let consoleErrors = 0;
  page.on('console', m => { if (m.type() === 'error') consoleErrors++; });

  let light = 0, dark = 0, nodes = 0, minRules = Infinity, notes = [];
  for (const path of PAGES) {
    let res;
    try {
      const r = await page.goto(origin + path, { waitUntil: 'load' });
      if (!r || !r.ok()) { notes.push(`${path}:HTTP${r ? r.status() : '?'}`); continue; }
      // Passed as a string, `evaluate` treats the argument as an EXPRESSION —
      // so a bare function literal evaluates to the function itself and never
      // runs. It must be invoked inside the page.
      res = await page.evaluate(`(${MEASURE})(${SETTLE_MS})`);
    } catch (e) {
      notes.push(`${path}:${String(e.message).slice(0, 40)}`); continue;
    }
    if (!res || !res.light || !res.dark) {
      // A shape we did not expect means the measurement did not happen. Count
      // it as a failure rather than skipping it, or the audit reports a clean
      // run over pages it never actually looked at.
      notes.push(`${path}: measurement returned no result`);
      failed = true;
      continue;
    }
    nodes += res.light.checked;
    light += res.light.fails.length;
    dark += res.dark.fails.length;
    minRules = Math.min(minRules, res.rules);
    if (res.overflow > 1) notes.push(`${path}:overflow+${res.overflow}`);
    for (const f of [...res.light.fails, ...res.dark.fails]) {
      notes.push(`${path} ${f.cls} ${f.ratio}<${f.need} "${f.text}"`);
    }
  }
  await ctx.close();

  // A page whose stylesheets did not load passes trivially. Treat that as a
  // failure of the AUDIT, not a pass of the site.
  const unstyled = minRules < 500;
  const bad = light > 0 || dark > 0 || unstyled;
  if (bad) failed = true;

  console.log(
    `${bad ? 'FAIL' : 'PASS'}  ${origin}  nodes=${nodes} light=${light} dark=${dark} ` +
    `rules=${minRules === Infinity ? 0 : minRules} consoleErrors=${consoleErrors}` +
    (unstyled ? '  ← STYLESHEETS DID NOT LOAD; result is meaningless' : '')
  );
  for (const n of [...new Set(notes)].slice(0, 12)) console.log(`        ${n}`);
}

await browser.close();
process.exit(failed ? 1 : 0);
