/**
 * Pro-mode smoke: create session, upload demo, switch pro, run linear, assert no white-screen.
 * Usage: npx playwright test  (or: node --experimental-vm-modules with playwright)
 * Actually run via: npx playwright install chromium once; then node this file.
 */
import { chromium } from 'playwright';
import { readFileSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const BASE = process.env.STATS_URL || 'http://127.0.0.1:8080';
const here = dirname(fileURLToPath(import.meta.url));
const demoPath = resolve(here, '../public/demo_cohort.csv');

function assert(cond, msg) {
  if (!cond) throw new Error(msg);
}

async function api(path, init) {
  const res = await fetch(`${BASE}${path}`, init);
  const text = await res.text();
  let body;
  try {
    body = JSON.parse(text);
  } catch {
    body = text;
  }
  if (!res.ok) throw new Error(`${init?.method || 'GET'} ${path} → ${res.status} ${text.slice(0, 300)}`);
  return body;
}

const findings = [];

async function main() {
  // 1) Fresh session + dataset
  const session = await api('/api/sessions', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: '{}' });
  const sid = session.id;
  const csv = readFileSync(demoPath);
  const b64 = csv.toString('base64');
  const ds = await api(`/api/sessions/${sid}/datasets`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ filename: 'demo_cohort.csv', data: b64 }),
  });
  findings.push(`session=${sid} dataset=${ds.dataset_id} rows=${ds.row_count}`);

  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  const consoleErrors = [];
  page.on('pageerror', (e) => consoleErrors.push(e.message));
  page.on('console', (msg) => {
    if (msg.type() === 'error') consoleErrors.push(msg.text());
  });

  await page.goto(`${BASE}/?session_id=${sid}`, { waitUntil: 'networkidle' });
  await page.waitForTimeout(800);

  // Switch to pro
  await page.locator('label').filter({ hasText: '专业' }).click();
  await page.waitForTimeout(500);

  const body1 = await page.locator('body').innerText();
  assert(body1.includes('上传数据集'), 'pro mode should show 上传数据集');
  assert(body1.includes('demo_cohort'), 'dataset tag should be visible');
  // Auto-select unique dataset → configurator visible
  assert(
    body1.includes('可视化统计分析配置') || (await page.locator('text=可视化统计分析配置').count()) > 0,
    'configurator should auto-appear for single dataset',
  );
  findings.push('PASS: pro shell + auto configurator');

  // Right-rail buttons: run disabled without analysis; debug always disabled
  const runBtn = page.getByLabel('运行');
  assert(await runBtn.isDisabled(), '运行 should be disabled without analysis');
  const debugBtn = page.getByLabel('调试');
  assert(await debugBtn.isDisabled(), '调试 should always be disabled (placeholder)');
  findings.push('PASS: right-rail idle disables');

  // Fill regression via form API-ish: click 回归, set fields through UI
  await page.getByText('回归建模分析', { exact: true }).click();
  await page.waitForTimeout(200);

  // Dependent select
  const selects = page.locator('.ant-select');
  await selects.nth(0).click();
  await page.locator('.ant-select-item-option').filter({ hasText: 'bmi' }).first().click();
  await page.waitForTimeout(150);
  // Independent multi
  await selects.nth(1).click();
  await page.locator('.ant-select-item-option').filter({ hasText: /^age / }).first().click();
  await page.keyboard.press('Escape');
  await page.waitForTimeout(150);

  await page.getByRole('button', { name: '开始统计计算' }).click();
  // Wait for result or error (not white screen)
  await page.waitForTimeout(3000);

  const rootKids = await page.evaluate(() => document.querySelector('#root')?.children.length ?? 0);
  const body2 = await page.locator('body').innerText();
  assert(rootKids > 0, `root should not be empty (white-screen). console=${consoleErrors.join(' | ')}`);
  assert(body2.length > 50, 'body should have content after run');

  // Result table or at least report region
  const hasReport =
    body2.includes('分析报告结果') ||
    body2.includes('估计值') ||
    body2.includes('Beta') ||
    body2.includes('p 值') ||
    body2.includes('运行失败');
  assert(hasReport, `expected report/table after run, got snippet: ${body2.slice(0, 400)}`);
  findings.push('PASS: linear run did not white-screen; report visible');

  // Reload poisoned session path
  await page.goto(`${BASE}/?session_id=${sid}`, { waitUntil: 'networkidle' });
  await page.waitForTimeout(1000);
  await page.locator('label').filter({ hasText: '专业' }).click();
  await page.waitForTimeout(500);
  const root2 = await page.evaluate(() => document.querySelector('#root')?.children.length ?? 0);
  assert(root2 > 0, `reload session should not white-screen. errors=${consoleErrors.join(' | ')}`);
  findings.push('PASS: reload session with skill results stays alive');

  // No toFixed crash in console
  const toFixedCrash = consoleErrors.some((e) => e.includes('toFixed'));
  assert(!toFixedCrash, `toFixed still crashing: ${consoleErrors.join(' | ')}`);
  findings.push('PASS: no toFixed pageerror');

  await browser.close();
  console.log(findings.join('\n'));
  console.log('ALL PRO SMOKE CHECKS PASSED');
}

main().catch((err) => {
  console.error('SMOKE FAILED:', err.message);
  console.error(findings.join('\n'));
  process.exit(1);
});
