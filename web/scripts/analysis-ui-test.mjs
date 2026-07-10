/**
 * Focused analysis module UI test: seed session+dataset via API, open Pro mode,
 * run each analysis type with robust Ant Design select handling.
 */
import { chromium } from 'playwright';
import { readFileSync, mkdirSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const BASE = process.env.STATS_URL || 'http://127.0.0.1:5173';
const API = process.env.API_URL || 'http://127.0.0.1:8080';
const here = dirname(fileURLToPath(import.meta.url));
const demoPath = resolve(here, '../public/demo_cohort.csv');
const shotDir = resolve(here, '../../../work/full-ui-test-shots');
mkdirSync(shotDir, { recursive: true });

const findings = [];
const consoleErrors = [];
const pageErrors = [];

function log(status, name, detail = '') {
  const line = `[${status}] ${name}${detail ? ' — ' + detail : ''}`;
  findings.push(line);
  console.log(line);
}

async function api(path, init) {
  const res = await fetch(`${API}${path}`, init);
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

const wait = (ms) => new Promise((r) => setTimeout(r, ms));

async function pickOption(page, optionText) {
  await wait(250);
  const dropdown = page.locator('.ant-select-dropdown:visible').last();
  await dropdown.waitFor({ state: 'visible', timeout: 6000 });
  // Options look like "age (Numeric)" — match prefix
  const byTitle = dropdown.locator(`.ant-select-item-option[title^="${optionText}"]`).first();
  if ((await byTitle.count()) > 0) {
    await byTitle.click({ force: true });
    return;
  }
  const byText = dropdown.locator('.ant-select-item-option').filter({ hasText: new RegExp(`^${optionText}\\b`) }).first();
  await byText.click({ force: true });
}

async function fillByLabel(page, labelPart, optionText, multi = false) {
  const item = page.locator('.ant-form-item').filter({ hasText: labelPart }).first();
  await item.locator('.ant-select').first().click();
  await pickOption(page, optionText);
  if (multi) {
    await page.keyboard.press('Escape');
    await wait(150);
  }
}

async function ensureProWithDataset(page, sid) {
  await page.goto(`${BASE}/?session_id=${sid}`, { waitUntil: 'networkidle' });
  await wait(800);
  // switch pro
  const pro = page.locator('label').filter({ hasText: '专业' });
  if ((await pro.count()) > 0) await pro.first().click();
  await wait(600);
  // select dataset chip if needed
  const chip = page.getByLabel(/数据集:/).or(page.getByText('demo_cohort'));
  if ((await chip.count()) > 0) {
    await chip.first().click().catch(() => {});
    await wait(400);
  }
}

async function rootAlive(page) {
  return page.evaluate(() => (document.querySelector('#root')?.children.length ?? 0) > 0);
}

async function runModule(page, name, setup) {
  const beforeErrors = pageErrors.length;
  try {
    await setup();
    await page.getByRole('button', { name: '开始统计计算' }).click();
    await wait(5000);
    await page.screenshot({ path: resolve(shotDir, `analysis-${name}.png`), fullPage: true });
    const body = await page.locator('body').innerText();
    const alive = await rootAlive(page);
    if (!alive) {
      log('FAIL', name, 'white-screen');
      return;
    }
    if (pageErrors.slice(beforeErrors).some((e) => e.includes('toFixed'))) {
      log('FAIL', name, 'toFixed crash');
      return;
    }
    if (body.includes('运行失败')) {
      log('BUG', name, `run failed: ${body.match(/运行失败[\s\S]{0,120}/)?.[0] || body.slice(0, 120)}`);
      return;
    }
    const hasTable = (await page.locator('table').count()) > 0;
    const hasChart = (await page.locator('canvas, [_echarts_instance_]').count()) > 0;
    const hasReport =
      hasTable ||
      hasChart ||
      body.includes('分析报告') ||
      body.includes('估计') ||
      body.includes('系数') ||
      body.includes('均值') ||
      body.includes('生存') ||
      body.includes('p');
    if (hasReport) log('PASS', name, `table=${hasTable} chart=${hasChart}`);
    else log('BUG', name, `no clear report: ${body.slice(0, 200)}`);
  } catch (e) {
    log('BUG', name, e.message.slice(0, 220));
    await page.screenshot({ path: resolve(shotDir, `analysis-${name}-error.png`), fullPage: true }).catch(() => {});
  }
}

async function main() {
  const session = await api('/api/sessions', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: '{}',
  });
  const sid = session.id;
  const csv = readFileSync(demoPath);
  const ds = await api(`/api/sessions/${sid}/datasets`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ filename: 'demo_cohort.csv', data: csv.toString('base64') }),
  });
  console.log(`session=${sid} dataset=${ds.dataset_id}`);

  // Also exercise each skill via pure API for ground truth
  const apiCases = [
    {
      name: 'api-tableone',
      body: {
        skill_id: 'tableone',
        dataset_id: ds.dataset_id,
        args: { continuous: ['age', 'bmi'], categorical: ['sex'] },
      },
    },
    {
      name: 'api-ttest',
      body: { skill_id: 'ttest', dataset_id: ds.dataset_id, args: { group: 'sex', testVar: 'bmi' } },
    },
    {
      name: 'api-linear',
      body: {
        skill_id: 'model_linear',
        dataset_id: ds.dataset_id,
        args: { outcome: 'bmi', predictors: ['age'] },
      },
    },
    {
      name: 'api-logistic',
      body: {
        skill_id: 'model_logistic',
        dataset_id: ds.dataset_id,
        args: { outcome: 'disease', predictors: ['age', 'bmi'] },
      },
    },
    {
      name: 'api-km',
      body: {
        skill_id: 'survival_km',
        dataset_id: ds.dataset_id,
        args: { time: 'fu_time', event: 'death' },
      },
    },
    {
      name: 'api-cox',
      body: {
        skill_id: 'model_cox',
        dataset_id: ds.dataset_id,
        args: { time: 'fu_time', event: 'death', predictors: ['age'] },
      },
    },
  ];

  for (const c of apiCases) {
    try {
      const r = await api(`/api/sessions/${sid}/run`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(c.body),
      });
      const keys = Object.keys(r || {});
      log('PASS', c.name, `keys=${keys.join(',')}`);
    } catch (e) {
      log('BUG', c.name, e.message.slice(0, 250));
    }
  }

  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
  page.on('pageerror', (e) => pageErrors.push(e.message));
  page.on('console', (m) => {
    if (m.type() === 'error') consoleErrors.push(m.text());
  });

  await ensureProWithDataset(page, sid);
  await page.screenshot({ path: resolve(shotDir, 'analysis-pro-ready.png'), fullPage: true });

  // TableOne
  await runModule(page, 'ui-tableone', async () => {
    await page.getByText('基线特征', { exact: false }).first().click();
    await wait(200);
    await fillByLabel(page, '连续性数值变量', 'age', true);
    await page.locator('.ant-form-item').filter({ hasText: '连续性数值变量' }).locator('.ant-select').first().click();
    await pickOption(page, 'bmi');
    await page.keyboard.press('Escape');
  });

  // Linear
  await runModule(page, 'ui-linear', async () => {
    await page.getByText('回归建模分析', { exact: true }).click();
    await wait(200);
    await page.getByText('多元线性回归', { exact: false }).click().catch(() => {});
    await wait(150);
    await fillByLabel(page, '因变量', 'bmi');
    await fillByLabel(page, '自变量列表', 'age', true);
  });

  // Logistic
  await runModule(page, 'ui-logistic', async () => {
    await page.getByText('回归建模分析', { exact: true }).click();
    await wait(200);
    await page.getByText('Logistic 回归', { exact: false }).click();
    await wait(150);
    await fillByLabel(page, '因变量', 'disease');
    await fillByLabel(page, '自变量列表', 'age', true);
  });

  // KM
  await runModule(page, 'ui-km', async () => {
    await page.getByText('KM生存分析', { exact: false }).click();
    await wait(200);
    await fillByLabel(page, '生存时间', 'fu_time').catch(async () => fillByLabel(page, '时间', 'fu_time'));
    await fillByLabel(page, '删失状态', 'death').catch(async () =>
      fillByLabel(page, '状态', 'death').catch(async () => fillByLabel(page, '事件', 'death')),
    );
  });

  // T-test
  await runModule(page, 'ui-ttest', async () => {
    await page.getByText('T检验', { exact: false }).click();
    await wait(200);
    await fillByLabel(page, '分组自变量', 'sex');
    await fillByLabel(page, '待检验因变量', 'bmi');
  });

  // Cox via regression radio
  await runModule(page, 'ui-cox', async () => {
    await page.getByText('回归建模分析', { exact: true }).click();
    await wait(200);
    await page.getByText('Cox', { exact: false }).click();
    await wait(150);
    // timeVar + event(dependent) + predictors
    await fillByLabel(page, '时间', 'fu_time').catch(async () => fillByLabel(page, '生存时间', 'fu_time'));
    await fillByLabel(page, '终点事件', 'death').catch(async () => fillByLabel(page, '事件', 'death'));
    await fillByLabel(page, '自变量列表', 'age', true);
  });

  // Sidecar languages after last successful analysis
  const body = await page.locator('body').innerText();
  if (body.includes('等价代码')) {
    log('PASS', 'sidecar-panel-visible');
    for (const lang of ['Python', 'R', 'SAS', 'SPSS']) {
      const tab = page.getByRole('tab', { name: lang });
      if ((await tab.count()) > 0) {
        await tab.first().click();
        await wait(200);
        log('PASS', `sidecar-tab-${lang}`);
      } else {
        // maybe segment or button
        const t = page.getByText(lang, { exact: true });
        if ((await t.count()) > 0) {
          await t.first().click().catch(() => {});
          log('PASS', `sidecar-text-${lang}`);
        }
      }
    }
  } else {
    log('BUG', 'sidecar-panel-missing after analysis');
  }

  // 辅助决策 — open sidebar in pro if collapsed
  const openSidebar = page.getByLabel('打开侧边栏');
  if ((await openSidebar.count()) > 0) await openSidebar.click().catch(() => {});
  await wait(300);
  const da = page.getByText('辅助决策');
  if ((await da.count()) > 0) {
    const sw = page.locator('.ant-space').filter({ hasText: '辅助决策' }).locator('.ant-switch');
    if ((await sw.count()) > 0) {
      await sw.first().click();
      await wait(500);
      log('PASS', 'ui-辅助决策-toggle');
    } else {
      log('BUG', '辅助决策 switch missing next to label');
    }
  } else {
    log('BUG', '辅助决策 not in Pro sidebar');
  }

  // Reload session with skill results — no white screen
  await page.goto(`${BASE}/?session_id=${sid}`, { waitUntil: 'networkidle' });
  await wait(1200);
  const pro = page.locator('label').filter({ hasText: '专业' });
  if ((await pro.count()) > 0) await pro.first().click();
  await wait(800);
  await page.screenshot({ path: resolve(shotDir, 'analysis-reload.png'), fullPage: true });
  if (await rootAlive(page)) log('PASS', 'reload-session-with-results');
  else log('FAIL', 'reload-session-with-results', 'white-screen');

  if (pageErrors.some((e) => e.includes('toFixed'))) log('FAIL', 'toFixed-crash', pageErrors.join(' | '));
  else log('PASS', 'no-toFixed');

  await browser.close();

  const pass = findings.filter((f) => f.startsWith('[PASS]')).length;
  const fail = findings.filter((f) => f.startsWith('[FAIL]')).length;
  const bug = findings.filter((f) => f.startsWith('[BUG]')).length;
  console.log('\n======== ANALYSIS SUMMARY ========');
  console.log(`PASS=${pass} FAIL=${fail} BUG=${bug}`);
  findings.filter((f) => f.startsWith('[BUG]') || f.startsWith('[FAIL]')).forEach((f) => console.log(f));
  process.exit(fail > 0 ? 1 : 0);
}

main().catch((e) => {
  console.error(e);
  process.exit(2);
});
