import { chromium } from 'playwright';
import { readFileSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const BASE = 'http://127.0.0.1:5173';
const API = 'http://127.0.0.1:8080';
const here = dirname(fileURLToPath(import.meta.url));
const demo = resolve(here, '../public/demo_cohort.csv');
const shotDir = resolve(here, '../../../work/full-ui-test-shots');
const wait = (ms) => new Promise((r) => setTimeout(r, ms));

async function api(p, init) {
  const r = await fetch(API + p, init);
  const t = await r.text();
  const b = JSON.parse(t);
  if (!r.ok) throw new Error(r.status + t.slice(0, 200));
  return b;
}

async function pick(page, label, opt) {
  const item = page.locator('.ant-form-item').filter({ hasText: label }).first();
  await item.locator('.ant-select').first().click();
  await wait(250);
  const dd = page.locator('.ant-select-dropdown:visible').last();
  await dd.locator(`.ant-select-item-option[title^="${opt}"]`).first().click({ force: true });
  await wait(150);
}

const session = await api('/api/sessions', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: '{}',
});
const sid = session.id;
await api(`/api/sessions/${sid}/datasets`, {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({ filename: 'demo_cohort.csv', data: readFileSync(demo).toString('base64') }),
});

const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
const pageErrors = [];
page.on('pageerror', (e) => pageErrors.push(e.message));
const reqFails = [];
page.on('requestfailed', (r) =>
  reqFails.push(`${r.method()} ${r.url()} ${r.failure()?.errorText || ''}`),
);

await page.goto(`${BASE}/?session_id=${sid}`, { waitUntil: 'networkidle' });
await wait(800);
await page.locator('label').filter({ hasText: '专业' }).first().click();
await wait(700);
const chip = page.getByLabel(/数据集:/).or(page.getByText('demo_cohort'));
if ((await chip.count()) > 0) await chip.first().click().catch(() => {});
await wait(400);

await page.getByText('回归建模分析', { exact: true }).click();
await wait(200);
await page.getByText('Cox 比例风险回归', { exact: false }).click();
await wait(200);

const labels = await page.locator('.ant-form-item-label').allInnerTexts();
console.log('FORM_LABELS', JSON.stringify(labels));

for (const [label, opt] of [
  ['时间', 'fu_time'],
  ['终点事件', 'death'],
  ['自变量列表', 'age'],
]) {
  try {
    await pick(page, label, opt);
    if (label.includes('自变量')) await page.keyboard.press('Escape');
    console.log('PICK_OK', label, opt);
  } catch (e) {
    console.log('PICK_FAIL', label, e.message.slice(0, 160));
  }
}

await page.getByRole('button', { name: '开始统计计算' }).click();
await wait(5000);
const body = await page.locator('body').innerText();
console.log('COX_ALIVE', await page.evaluate(() => (document.querySelector('#root')?.children.length ?? 0) > 0));
console.log('COX_TABLE', await page.locator('table').count());
console.log('COX_FAIL', body.includes('运行失败'));
console.log('COX_SNIP', body.slice(0, 500).replace(/\n/g, ' | '));
console.log('COX_PAGE_ERR', pageErrors.join(' || ').slice(0, 300));
await page.screenshot({ path: resolve(shotDir, 'cox-result.png'), fullPage: true });

// 辅助决策 in 自动化 drawer
await page.getByLabel('自动化', { exact: true }).click();
await wait(500);
const da = await page.getByText('辅助决策').count();
console.log('DA_IN_AUTOMATION', da);
if (da > 0) {
  const sw = page.locator('.ant-space').filter({ hasText: '辅助决策' }).locator('.ant-switch');
  console.log('DA_SWITCH', await sw.count());
  if ((await sw.count()) > 0) {
    const before = await sw.first().getAttribute('aria-checked');
    await sw.first().click();
    await wait(700);
    const after = await sw.first().getAttribute('aria-checked');
    console.log('DA_TOGGLED', before, '->', after);
  }
}
await page.screenshot({ path: resolve(shotDir, 'automation-da.png'), fullPage: true });

// Export in AnalysisResultView path — simple mode messages
console.log('EXPORT_IN_BODY', body.includes('导出') || body.includes('快照'));

// Sidecar abort after rapid re-run
await page.keyboard.press('Escape');
await wait(200);
await page.getByText('回归建模分析', { exact: true }).click();
await wait(150);
await page.getByText('多元线性回归', { exact: false }).click().catch(() => {});
await wait(100);
try {
  await pick(page, '因变量', 'bmi');
  await pick(page, '自变量列表', 'age');
  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: '开始统计计算' }).click();
  await wait(3000);
  // second run immediately
  await page.getByRole('button', { name: '开始统计计算' }).click();
  await wait(3000);
} catch (e) {
  console.log('RERUN_ERR', e.message.slice(0, 120));
}
console.log(
  'REQ_FAILS',
  reqFails
    .filter((r) => r.includes('/api/'))
    .slice(0, 15)
    .join(' || '),
);

// Session bloat: list count
const list = await api('/api/sessions');
console.log('SESSION_COUNT', Array.isArray(list) ? list.length : list);

await browser.close();
console.log('DONE');
