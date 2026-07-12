/**
 * Capture crisp hero product shots for docs/stats-code-intro.html
 * Run from web/:  node scripts/capture-hero.mjs
 */
import { chromium } from 'playwright';
import { writeFileSync, mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const OUT_DIR = join(__dirname, '../../docs/assets');
const BASE = process.env.STATS_URL || 'http://127.0.0.1:5173';
const API = process.env.STATS_API || 'http://127.0.0.1:8080';
const VIEW_W = 1440;
const VIEW_H = 900;
const DPR = 2;

async function pickSessionId() {
  if (process.env.STATS_SESSION_ID) return process.env.STATS_SESSION_ID;
  const res = await fetch(`${API}/api/sessions`);
  if (!res.ok) throw new Error(`list sessions failed: ${res.status}`);
  const list = await res.json();
  const ranked = [...list].sort((a, b) => {
    const score = (s) => (s.dataset_count || 0) * 10 + (s.message_count || 0);
    return score(b) - score(a);
  });
  if (!ranked.length) throw new Error('no sessions available');
  return ranked[0].id;
}

async function main() {
  mkdirSync(OUT_DIR, { recursive: true });
  const sid = await pickSessionId();
  console.log(`session=${sid} viewport=${VIEW_W}x${VIEW_H}@${DPR}x`);

  const browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({
    viewport: { width: VIEW_W, height: VIEW_H },
    deviceScaleFactor: DPR,
    colorScheme: 'light',
  });
  const page = await context.newPage();
  await page.goto(`${BASE}/?session_id=${encodeURIComponent(sid)}`, {
    waitUntil: 'networkidle',
    timeout: 60_000,
  });
  const pro = page.locator('label').filter({ hasText: '专业' });
  if (await pro.count()) {
    await pro.first().click();
    await page.waitForTimeout(700);
  }
  await page.waitForTimeout(1200);
  await page.evaluate(async () => {
    if (document.fonts?.ready) await document.fonts.ready;
  });
  await page.waitForTimeout(400);

  const out2x = join(OUT_DIR, 'hero-product.png');
  await page.screenshot({
    path: out2x,
    type: 'png',
    animations: 'disabled',
    caret: 'hide',
    scale: 'device',
  });

  const buf2x = await page.screenshot({
    type: 'png',
    animations: 'disabled',
    caret: 'hide',
    scale: 'device',
  });
  const b64 = Buffer.from(buf2x).toString('base64');
  const oneX = await page.evaluate(
    async ({ b64, w, h }) => {
      const img = new Image();
      img.src = `data:image/png;base64,${b64}`;
      await new Promise((resolve, reject) => {
        img.onload = resolve;
        img.onerror = reject;
      });
      const canvas = document.createElement('canvas');
      canvas.width = w;
      canvas.height = h;
      const ctx = canvas.getContext('2d');
      ctx.imageSmoothingEnabled = true;
      ctx.imageSmoothingQuality = 'high';
      ctx.drawImage(img, 0, 0, w, h);
      return canvas.toDataURL('image/png').split(',')[1];
    },
    { b64, w: VIEW_W, h: VIEW_H },
  );
  writeFileSync(join(OUT_DIR, 'hero-product-1x.png'), Buffer.from(oneX, 'base64'));
  await browser.close();
  console.log(`wrote ${out2x}`);
  console.log(`wrote ${join(OUT_DIR, 'hero-product-1x.png')}`);
}

main().catch((err) => {
  console.error('CAPTURE FAILED:', err);
  process.exit(1);
});
