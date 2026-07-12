/**
 * A8 Network verification for Stats Code (Playwright).
 * Uses page.on('response') — does NOT need Codex Chrome extension.
 *
 * Prerequisites: backend :8080 + frontend :5173 running.
 *
 * Usage:
 *   npm run test:a8
 *   node scripts/a8-network-verify.mjs
 */
import { chromium } from 'playwright';
import { mkdirSync, writeFileSync, existsSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const webRoot = resolve(here, '..');
const BASE = process.env.STATS_URL || 'http://127.0.0.1:5173';
const API = process.env.API_URL || 'http://127.0.0.1:8080';
const demo = resolve(webRoot, 'public/demo_cohort.csv');
const outDir = resolve(
  webRoot,
  'output',
  `a8-network-${new Date().toISOString().slice(0, 19).replace(/[:T]/g, '-')}`,
);
mkdirSync(outDir, { recursive: true });

const wait = (ms) => new Promise((r) => setTimeout(r, ms));

/** @type {{ method: string, url: string, status: number, ok: boolean, resourceType: string, ts: number }[]} */
const apiResponses = [];
/** @type {string[]} */
const consoleErrors = [];
/** @type {string[]} */
const pageErrors = [];
/** @type {string[]} */
const failedReqs = [];

function isApiUrl(url) {
  try {
    const u = new URL(url, 'http://127.0.0.1');
    // Only real backend routes: /api/...  (not Vite /src/api/client.ts)
    return u.pathname === '/api' || u.pathname.startsWith('/api/');
  } catch {
    return /:\/\/[^/]+\/api(\/|$)/.test(url);
  }
}

async function confirmAndRun(page) {
  const confirm = page.getByRole('button', { name: '确认并运行' });
  try {
    await confirm.first().waitFor({ state: 'visible', timeout: 5000 });
    await confirm.first().click({ force: true });
    return true;
  } catch {
    const open = page.locator('.ant-modal-wrap:not([style*="display: none"]) .ant-btn-primary');
    if ((await open.count()) > 0) {
      await open.last().click({ force: true });
      return true;
    }
    return false;
  }
}

async function waitForApi(predicate, timeoutMs = 15000) {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    if (apiResponses.some(predicate)) return true;
    await wait(200);
  }
  return apiResponses.some(predicate);
}

function shortUrl(url) {
  try {
    const u = new URL(url);
    return u.pathname + u.search;
  } catch {
    return url;
  }
}

async function pickSelect(page, labelText, optionText, multi = false) {
  const item = page.locator('.ant-form-item').filter({ hasText: labelText }).first();
  await item.locator('.ant-select').first().click({ force: true, timeout: 8000 });
  await wait(300);
  const dropdown = page.locator('.ant-select-dropdown:visible').last();
  await dropdown.waitFor({ state: 'visible', timeout: 5000 }).catch(() => {});
  let opt = dropdown.locator('.ant-select-item-option-content').filter({ hasText: optionText }).first();
  if ((await opt.count()) === 0) {
    opt = page.locator('.ant-select-item-option-content').filter({ hasText: optionText }).last();
  }
  await opt.click({ force: true, timeout: 8000 });
  if (multi) {
    await page.keyboard.press('Escape');
    await wait(100);
  }
  await wait(200);
}

async function expandAnalysisSetup(page) {
  const header = page.getByText('分析设置', { exact: false }).first();
  if ((await header.count()) > 0) {
    await header.click({ force: true }).catch(() => {});
    await wait(400);
  }
  const panel = page.locator('.ant-collapse-header').filter({ hasText: /分析设置|配置/ }).first();
  if ((await panel.count()) > 0) {
    const expanded = await page.locator('.ant-collapse-item-active').filter({ hasText: /分析设置/ }).count();
    if (expanded === 0) {
      await panel.click({ force: true }).catch(() => {});
      await wait(400);
    }
  }
  const chip = page.getByText(/调整变量|再次分析|重新配置/).first();
  if ((await chip.count()) > 0) {
    await chip.click({ force: true }).catch(() => {});
    await wait(400);
  }
}

// Preflight
const health = await fetch(`${API}/api/health`);
if (!health.ok) {
  console.error(`FAIL preflight: ${API}/api/health -> ${health.status}`);
  console.error('Start services first, e.g. scripts/start-stats.ps1 -NoBrowser');
  process.exit(2);
}
if (!existsSync(demo)) {
  console.error(`FAIL preflight: demo missing ${demo}`);
  process.exit(2);
}

const browser = await chromium.launch({ headless: true });
const context = await browser.newContext({ viewport: { width: 1440, height: 900 }, locale: 'zh-CN' });
const page = await context.newPage();

page.on('pageerror', (e) => pageErrors.push(e.message));
page.on('console', (m) => {
  if (m.type() === 'error') consoleErrors.push(m.text());
});
page.on('requestfailed', (r) => {
  if (isApiUrl(r.url())) {
    failedReqs.push(`${r.method()} ${shortUrl(r.url())} ${r.failure()?.errorText || ''}`);
  }
});
// Core A8 capability: capture Network without Chrome extension
page.on('response', (res) => {
  const url = res.url();
  if (!isApiUrl(url)) return;
  apiResponses.push({
    method: res.request().method(),
    url: shortUrl(url),
    status: res.status(),
    ok: res.ok(),
    resourceType: res.request().resourceType(),
    ts: Date.now(),
  });
});

const steps = [];
function step(name, ok, detail = '') {
  steps.push({ name, ok, detail });
  console.log(`[${ok ? 'PASS' : 'FAIL'}] ${name}${detail ? ' — ' + detail : ''}`);
}

try {
  await page.goto(BASE, { waitUntil: 'domcontentloaded', timeout: 60000 });
  await wait(1200);
  await page.screenshot({ path: resolve(outDir, '01-home.png') });
  step('A1 home loads', (await page.locator('body').innerText()).length > 20);

  const pro = page.getByText('专业', { exact: true }).first();
  if ((await pro.count()) > 0) {
    await pro.click({ force: true });
    await wait(700);
  }
  step('Pro mode', true);

  await page.getByText('数据', { exact: true }).first().click({ force: true }).catch(() => {});
  await wait(300);
  const uploadBtn = page
    .getByLabel('上传数据集')
    .or(page.getByRole('button', { name: /上传/ }))
    .or(page.getByText('上传数据集'));
  if ((await uploadBtn.count()) > 0) await uploadBtn.first().click({ force: true });
  await wait(400);
  const fileInput = page.locator('input[type="file"]');
  if ((await fileInput.count()) === 0) {
    step('A3 upload input', false, 'no file input');
  } else {
    await fileInput.first().setInputFiles(demo);
    await wait(3000);
    const t = await page.locator('body').innerText();
    step('A3 upload demo', t.includes('demo_cohort') || t.includes('240'), t.slice(0, 80).replace(/\n/g, ' '));
    await page.screenshot({ path: resolve(outDir, '02-upload.png') });
    await page.keyboard.press('Escape');
    await wait(300);
  }

  const t1 = page.getByText(/基线特征/).first();
  if ((await t1.count()) > 0) {
    await t1.click({ force: true });
    await wait(400);
    try {
      await pickSelect(page, '连续性数值变量', 'age', true);
      const cont = page.locator('.ant-form-item').filter({ hasText: '连续性数值变量' }).first();
      await cont.locator('.ant-select').first().click({ force: true });
      await wait(200);
      await page.locator('.ant-select-item-option-content').filter({ hasText: 'bmi' }).first().click({ force: true });
      await page.keyboard.press('Escape');
      const beforeRuns = apiResponses.filter((r) => /\/run$/.test(r.url)).length;
      await page.getByRole('button', { name: '开始统计计算' }).click({ force: true });
      const confirmed = await confirmAndRun(page);
      const gotRun = await waitForApi(
        (r) =>
          r.method === 'POST' &&
          /\/run$/.test(r.url) &&
          apiResponses.filter((x) => /\/run$/.test(x.url)).length > beforeRuns,
        20000,
      );
      await wait(1500);
      await page.screenshot({ path: resolve(outDir, '03-tableone.png') });
      const body = await page.locator('body').innerText();
      const failed = /运行失败|SkillInvalid/.test(body);
      const runOk = apiResponses.some((r) => r.method === 'POST' && /\/run$/.test(r.url) && r.ok);
      step(
        'A5 TableOne',
        confirmed && gotRun && runOk && !failed,
        `confirm=${confirmed} runNet=${gotRun} runOk=${runOk} tables=${await page.locator('table').count()}`,
      );
    } catch (e) {
      step('A5 TableOne', false, e.message.slice(0, 160));
    }
  } else {
    step('A5 TableOne', false, 'module chip not found');
  }

  await expandAnalysisSetup(page);
  try {
    await page.evaluate(() => {
      document.querySelectorAll('.ant-collapse-item').forEach((item) => {
        if (!item.classList.contains('ant-collapse-item-active')) {
          item.querySelector('.ant-collapse-header')?.dispatchEvent(new MouseEvent('click', { bubbles: true }));
        }
      });
      const again = Array.from(document.querySelectorAll('button, a, span, div')).find((n) =>
        /调整变量|再次分析|重新配置|可视化统计分析配置/.test((n.textContent || '').trim()),
      );
      again?.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    });
    await wait(600);

    const byValue = page.locator('input.ant-radio-button-input[value="t_test"]');
    if ((await byValue.count()) > 0) {
      await byValue.first().check({ force: true }).catch(async () => {
        await page.locator('label').filter({ has: byValue }).first().click({ force: true });
      });
    } else {
      await page.locator('.ant-radio-button-wrapper').filter({ hasText: /T检验/ }).first().click({ force: true });
    }
    await wait(500);

    await pickSelect(page, '分组自变量', 'sex');
    await pickSelect(page, '待检验因变量', 'bmi');
    const beforeRuns = apiResponses.filter((r) => /\/run$/.test(r.url)).length;
    await page.getByRole('button', { name: '开始统计计算' }).click({ force: true });
    const confirmed = await confirmAndRun(page);
    const gotRun = await waitForApi(
      (r) =>
        r.method === 'POST' &&
        /\/run$/.test(r.url) &&
        apiResponses.filter((x) => /\/run$/.test(x.url)).length > beforeRuns,
      20000,
    );
    await wait(1500);
    await page.screenshot({ path: resolve(outDir, '04-ttest.png') });
    const body = await page.locator('body').innerText();
    const failed = /运行失败|SkillInvalid/.test(body);
    const runCount = apiResponses.filter((r) => r.method === 'POST' && /\/run$/.test(r.url) && r.ok).length;
    const runOk = gotRun && runCount >= 1;
    const uiHint = /P\s*[=＝]\s*0?\.|p\s*值|t\s*=|自由度|均值|Cohen/i.test(body);
    step(
      'A6 t-test UI',
      confirmed && gotRun && runOk && !failed,
      `confirm=${confirmed} runNet=${gotRun} runOkCount=${runCount} uiHint=${uiHint}`,
    );
  } catch (e) {
    step('A6 t-test UI', false, e.message.slice(0, 160));
  }
} finally {
  await browser.close().catch(() => {});
}

const status5xx = apiResponses.filter((r) => r.status >= 500);
const status4xx = apiResponses.filter((r) => r.status >= 400 && r.status < 500);
const ok2xx = apiResponses.filter((r) => r.status >= 200 && r.status < 300);
const appConsoleErrors = consoleErrors.filter(
  (t) => !/Sentry|chrome-extension:|Extension context/i.test(t),
);
const hasUpload = apiResponses.some((r) => r.method === 'POST' && /\/datasets/.test(r.url) && r.ok);
const runOkList = apiResponses.filter((r) => r.method === 'POST' && /\/run$/.test(r.url) && r.ok);
const hasRun = runOkList.length >= 1;
const hasHealthOrSessions = apiResponses.some((r) => /\/(health|sessions)/.test(r.url) && r.ok);

const a8Pass =
  apiResponses.length > 0 &&
  status5xx.length === 0 &&
  failedReqs.length === 0 &&
  appConsoleErrors.length === 0 &&
  pageErrors.length === 0 &&
  hasUpload &&
  hasRun;

step(
  'A8 Network+Console',
  a8Pass,
  `api=${apiResponses.length} 2xx=${ok2xx.length} 4xx=${status4xx.length} 5xx=${status5xx.length} failed=${failedReqs.length} consoleAppErr=${appConsoleErrors.length} pageErr=${pageErrors.length}`,
);

const report = {
  generatedAt: new Date().toISOString(),
  base: BASE,
  api: API,
  outDir,
  method: 'playwright-page-on-response',
  usesChromeExtension: false,
  steps,
  a8: {
    pass: a8Pass,
    apiCount: apiResponses.length,
    status2xx: ok2xx.length,
    status4xx: status4xx.length,
    status5xx: status5xx.length,
    failedReqs,
    hasUpload,
    hasRun,
    hasHealthOrSessions,
    pageErrors,
    consoleErrorsApp: appConsoleErrors,
    consoleErrorsAll: consoleErrors,
  },
  apiResponses,
};

writeFileSync(resolve(outDir, 'report.json'), JSON.stringify(report, null, 2), 'utf8');

console.log('\n=== /api/* responses (Playwright Network) ===');
if (apiResponses.length === 0) {
  console.log('(none captured)');
} else {
  for (const r of apiResponses) {
    console.log(`${r.status}\t${r.method}\t${r.url}`);
  }
}

console.log('\n=== A8 verdict ===');
console.log(a8Pass ? 'PASS' : 'FAIL');
console.log(`usesChromeExtension: false`);
console.log(`report: ${resolve(outDir, 'report.json')}`);

const allCriticalOk = a8Pass && steps.filter((s) => s.name.startsWith('A')).every((s) => s.ok);
process.exit(allCriticalOk ? 0 : 1);
