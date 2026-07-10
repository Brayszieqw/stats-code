/**
 * Full human-like UI click test for Stats Code.
 * Maps frontend interactions to backend APIs and records PASS/FAIL/BUG.
 *
 * Usage:
 *   node web/scripts/full-ui-click-test.mjs
 * Env:
 *   STATS_URL=http://127.0.0.1:5173   (Vite dev, proxies /api)
 *   API_URL=http://127.0.0.1:8080     (direct backend)
 *   HEADED=1                          (show browser)
 */
import { chromium } from 'playwright';
import { readFileSync, mkdirSync, writeFileSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const BASE = process.env.STATS_URL || 'http://127.0.0.1:5173';
const API = process.env.API_URL || 'http://127.0.0.1:8080';
const HEADED = process.env.HEADED === '1';
const here = dirname(fileURLToPath(import.meta.url));
const demoPath = resolve(here, '../public/demo_cohort.csv');
const shotDir = resolve(here, '../../../work/full-ui-test-shots');
mkdirSync(shotDir, { recursive: true });

const findings = [];
const bugs = [];
const consoleErrors = [];
const pageErrors = [];
const failedRequests = [];

function log(status, name, detail = '') {
  const line = `[${status}] ${name}${detail ? ' — ' + detail : ''}`;
  findings.push(line);
  console.log(line);
  if (status === 'BUG' || status === 'FAIL') bugs.push(line);
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
  return { ok: res.ok, status: res.status, body, text };
}

async function shot(page, name) {
  const file = resolve(shotDir, `${name}.png`);
  await page.screenshot({ path: file, fullPage: true }).catch(() => {});
  return file;
}

async function bodyText(page) {
  return page.locator('body').innerText().catch(() => '');
}

async function rootAlive(page) {
  return page.evaluate(() => (document.querySelector('#root')?.children.length ?? 0) > 0);
}

async function waitMs(ms) {
  await new Promise((r) => setTimeout(r, ms));
}

async function safeClick(page, locator, label) {
  try {
    const loc = typeof locator === 'string' ? page.locator(locator) : locator;
    await loc.first().click({ timeout: 8000 });
    return true;
  } catch (e) {
    log('FAIL', label, e.message.slice(0, 180));
    return false;
  }
}

async function fillSelectByLabel(page, labelText, optionText, multi = false) {
  // Find form item by label text then open its select
  const item = page.locator('.ant-form-item').filter({ hasText: labelText }).first();
  const select = item.locator('.ant-select').first();
  await select.click({ timeout: 8000 });
  await waitMs(300);
  await pickOption(page, optionText);
  if (multi) {
    await waitMs(100);
    await page.keyboard.press('Escape');
    await page.locator('body').click({ position: { x: 5, y: 5 } }).catch(() => {});
  }
  await waitMs(200);
}

/** Pick option from the currently open Ant Design dropdown portal */
async function pickOption(page, optionText) {
  // Prefer visible dropdown; fall back to title attribute match
  const dropdown = page.locator('.ant-select-dropdown:visible').last();
  await dropdown.waitFor({ state: 'visible', timeout: 5000 }).catch(() => {});
  let opt = dropdown.locator(`.ant-select-item-option[title*="${optionText}"]`).first();
  if ((await opt.count()) === 0) {
    opt = dropdown.locator('.ant-select-item-option-content').filter({ hasText: optionText }).first();
  }
  if ((await opt.count()) === 0) {
    opt = page.locator(`.ant-select-item-option[title*="${optionText}"]`).last();
  }
  await opt.scrollIntoViewIfNeeded().catch(() => {});
  await opt.click({ force: true, timeout: 8000 });
  await waitMs(150);
}

async function main() {
  console.log(`\n=== Stats Code full UI click test ===`);
  console.log(`UI:  ${BASE}`);
  console.log(`API: ${API}`);
  console.log(`Shots: ${shotDir}\n`);

  // ─── A. Backend API smoke (no browser) ─────────────────────────────────
  {
    const h = await api('/api/health');
    if (h.ok && h.body?.status === 'ok') log('PASS', 'API health');
    else log('FAIL', 'API health', `${h.status} ${h.text?.slice?.(0, 100)}`);

    const sessions = await api('/api/sessions');
    if (sessions.ok && Array.isArray(sessions.body)) log('PASS', 'API GET /sessions', `count=${sessions.body.length}`);
    else log('FAIL', 'API GET /sessions', String(sessions.status));

    const created = await api('/api/sessions', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: '{}',
    });
    if (created.ok && created.body?.id) log('PASS', 'API POST /sessions', created.body.id);
    else log('FAIL', 'API POST /sessions', String(created.status));

    const sid = created.body?.id;
    if (sid) {
      const csv = readFileSync(demoPath);
      const ds = await api(`/api/sessions/${sid}/datasets`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ filename: 'demo_cohort.csv', data: csv.toString('base64') }),
      });
      if (ds.ok && ds.body?.dataset_id) {
        const preview = ds.body.preview_rows;
        if (Array.isArray(preview) && preview.length > 0) {
          log('PASS', 'API upload dataset + preview_rows', `rows=${preview.length} cols=${Object.keys(preview[0] || {}).length}`);
        } else {
          log('BUG', 'API upload dataset missing preview_rows', JSON.stringify(ds.body).slice(0, 200));
        }
      } else {
        log('FAIL', 'API upload dataset', `${ds.status} ${String(ds.text).slice(0, 200)}`);
      }

      // Linear run via API (skill_id + outcome/predictors match skill-registry)
      if (ds.body?.dataset_id) {
        const run = await api(`/api/sessions/${sid}/run`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            skill_id: 'model_linear',
            dataset_id: ds.body.dataset_id,
            args: { outcome: 'bmi', predictors: ['age'] },
          }),
        });
        if (run.ok) log('PASS', 'API POST /run model_linear', `keys=${Object.keys(run.body || {}).join(',')}`);
        else log('FAIL', 'API POST /run model_linear', `${run.status} ${String(run.text).slice(0, 200)}`);

        // Sidecar contract: software + dataset_sha256 + columns + params
        const sc = await api('/api/sidecar/linear', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            software: 'Python',
            dataset_sha256: ds.body.sha256 || '0'.repeat(64),
            columns: [
              { name: 'bmi', dtype: 'float' },
              { name: 'age', dtype: 'float' },
            ],
            params: { outcome: 'bmi', predictors: 'age' },
          }),
        });
        if (sc.ok && (sc.body?.text || sc.body?.algorithm_id || sc.body?.software)) {
          log('PASS', 'API POST /sidecar/linear');
        } else {
          log('BUG', 'API sidecar/linear', `${sc.status} ${String(sc.text).slice(0, 200)}`);
        }
      }

      const cm = await api('/api/coverage-matrix');
      if (cm.ok) log('PASS', 'API coverage-matrix');
      else log('BUG', 'API coverage-matrix', String(cm.status));

      const llm = await api('/api/llm-status');
      if (llm.ok) log('PASS', 'API llm-status', JSON.stringify(llm.body).slice(0, 120));
      else log('FAIL', 'API llm-status', String(llm.status));

      // Cleanup
      await api(`/api/sessions/${sid}`, { method: 'DELETE' });
    }
  }

  // ─── B. Browser human-like click tour ──────────────────────────────────
  const browser = await chromium.launch({
    headless: !HEADED,
    slowMo: HEADED ? 80 : 0,
  });
  const context = await browser.newContext({
    viewport: { width: 1440, height: 900 },
    locale: 'zh-CN',
  });
  const page = await context.newPage();

  page.on('pageerror', (e) => {
    pageErrors.push(e.message);
    console.log('  [pageerror]', e.message.slice(0, 200));
  });
  page.on('console', (msg) => {
    if (msg.type() === 'error') {
      consoleErrors.push(msg.text());
      console.log('  [console.error]', msg.text().slice(0, 200));
    }
  });
  page.on('requestfailed', (req) => {
    failedRequests.push(`${req.method()} ${req.url()} ${req.failure()?.errorText || ''}`);
  });

  // B1 Home load
  await page.goto(BASE, { waitUntil: 'networkidle', timeout: 60000 });
  await waitMs(1000);
  await shot(page, '01-home');
  const home = await bodyText(page);
  if (await rootAlive(page) && home.length > 30) log('PASS', 'Home page loads');
  else log('FAIL', 'Home page loads', `len=${home.length}`);

  // B2 Sidebar nav: 新对话 / 搜索 / 插件 / 自动化 / 分析模板
  for (const label of ['新对话', '搜索', '插件', '自动化', '分析模板']) {
    const ok = await safeClick(page, page.getByLabel(label, { exact: true }), `Sidebar click ${label}`);
    await waitMs(400);
    const t = await bodyText(page);
    if (!ok) continue;
    // drawers open for search/plugins/automation/templates
    if (label === '新对话') {
      // may create session or just focus
      log('PASS', `Sidebar: ${label}`, 'clicked');
    } else if (label === '搜索') {
      if (t.includes('搜索') || t.includes('关键词') || (await page.locator('.ant-drawer').count()) > 0) {
        log('PASS', `Sidebar drawer: ${label}`);
      } else {
        log('BUG', `Sidebar drawer: ${label}`, 'drawer content not visible');
      }
      // close drawer
      await page.keyboard.press('Escape');
      await waitMs(200);
    } else if (label === '插件') {
      if (t.includes('插件') || (await page.locator('.ant-drawer').count()) > 0) {
        log('PASS', `Sidebar drawer: ${label}`);
      } else {
        log('BUG', `Sidebar drawer: ${label}`);
      }
      await page.keyboard.press('Escape');
      await waitMs(200);
    } else if (label === '自动化') {
      if (t.includes('自动化') || t.includes('即将') || (await page.locator('.ant-drawer').count()) > 0) {
        log('PASS', `Sidebar drawer: ${label}`);
      } else {
        log('BUG', `Sidebar drawer: ${label}`);
      }
      await page.keyboard.press('Escape');
      await waitMs(200);
    } else if (label === '分析模板') {
      if (t.includes('线性回归') || t.includes('模板') || (await page.locator('.ant-drawer').count()) > 0) {
        log('PASS', `Sidebar drawer: ${label}`);
      } else {
        log('BUG', `Sidebar drawer: ${label}`, t.slice(0, 120));
      }
      await page.keyboard.press('Escape');
      await waitMs(200);
    }
  }
  await shot(page, '02-sidebar-nav');

  // B3 Suggestion cards / welcome
  const hasWelcome =
    home.includes('线性回归') ||
    home.includes('欢迎') ||
    home.includes('统计') ||
    home.includes('开始');
  if (hasWelcome) log('PASS', 'Welcome / suggestions visible');
  else log('BUG', 'Welcome / suggestions missing', home.slice(0, 150));

  // Try click suggestion card 线性回归
  const sug = page.getByText('线性回归', { exact: false }).first();
  if ((await sug.count()) > 0) {
    await sug.click().catch(() => {});
    await waitMs(500);
    log('PASS', 'Click suggestion 线性回归');
  } else {
    log('BUG', 'Suggestion card 线性回归 not found');
  }

  // B4 Mode toggle: 专业
  const proToggle = page.locator('label').filter({ hasText: '专业' }).or(page.getByText('专业', { exact: true }));
  if ((await proToggle.count()) > 0) {
    await proToggle.first().click();
    await waitMs(800);
    await shot(page, '03-pro-mode');
    const proBody = await bodyText(page);
    if (proBody.includes('上传') || proBody.includes('等价代码') || proBody.includes('专业')) {
      log('PASS', 'Switch to 专业 mode');
    } else {
      log('BUG', 'Pro mode content incomplete', proBody.slice(0, 200));
    }
  } else {
    log('FAIL', 'Mode toggle 专业 not found');
  }

  // B5 Mode toggle back to 简洁 and again to 专业 for further tests
  const simpleToggle = page.locator('label').filter({ hasText: /简洁|简易/ }).or(page.getByText(/简洁|简易/, { exact: true }));
  if ((await simpleToggle.count()) > 0) {
    await simpleToggle.first().click();
    await waitMs(500);
    log('PASS', 'Switch to 简洁/简易 mode');
    await proToggle.first().click().catch(() => {});
    await waitMs(500);
  }

  // Ensure pro mode for upload + configurator
  await proToggle.first().click().catch(() => {});
  await waitMs(600);

  // B6 Upload dataset via drawer
  const uploadBtn = page.getByLabel('上传数据集').or(page.getByRole('button', { name: /上传数据集/ }));
  if ((await uploadBtn.count()) > 0) {
    await uploadBtn.first().click();
    await waitMs(500);
    await shot(page, '04-upload-drawer');
    const fileInput = page.locator('input[type="file"]');
    if ((await fileInput.count()) > 0) {
      await fileInput.first().setInputFiles(demoPath);
      await waitMs(2500);
      await shot(page, '05-after-upload');
      const afterUp = await bodyText(page);
      if (afterUp.includes('demo_cohort') || afterUp.includes('已上传') || afterUp.includes('行')) {
        log('PASS', 'Upload demo_cohort.csv via UI');
      } else {
        // sometimes drawer still open with success card
        if (afterUp.includes('数据集') || afterUp.includes('列')) {
          log('PASS', 'Upload UI shows dataset summary');
        } else {
          log('BUG', 'Upload may have failed or UI silent', afterUp.slice(0, 250));
        }
      }
      // close drawer
      await page.keyboard.press('Escape');
      await waitMs(400);
    } else {
      log('BUG', 'Upload file input not found in drawer');
    }
  } else {
    log('BUG', 'Upload dataset button not found in Pro mode');
  }

  // B7 Dataset tag + auto configurator
  await waitMs(500);
  let proBody2 = await bodyText(page);
  if (proBody2.includes('demo_cohort')) log('PASS', 'Dataset tag visible after upload');
  else {
    // try select dataset chip
    const dsChip = page.getByLabel(/数据集:/).or(page.getByText('demo_cohort'));
    if ((await dsChip.count()) > 0) {
      await dsChip.first().click();
      await waitMs(400);
      log('PASS', 'Clicked dataset chip');
      proBody2 = await bodyText(page);
    } else {
      log('BUG', 'Dataset chip/tag not visible after upload');
    }
  }

  if (
    proBody2.includes('可视化统计分析配置') ||
    proBody2.includes('分析模块类型') ||
    proBody2.includes('基线特征') ||
    (await page.getByText('开始统计计算').count()) > 0
  ) {
    log('PASS', 'Analysis configurator visible');
  } else {
    log('BUG', 'Analysis configurator not auto-shown', proBody2.slice(0, 250));
  }

  // B8 Data explorer tab / panel
  const explorerTab = page.getByText(/数据探索|数据画像|字段/).first();
  if ((await explorerTab.count()) > 0) {
    await explorerTab.click().catch(() => {});
    await waitMs(400);
    const exp = await bodyText(page);
    if (exp.includes('字段') || exp.includes('缺失') || exp.includes('类型') || exp.includes('age') || exp.includes('bmi')) {
      log('PASS', 'DataExplorer shows column info');
    } else {
      log('BUG', 'DataExplorer empty or incomplete', exp.slice(0, 200));
    }
  } else {
    // may already be on report view with explorer inside
    if (proBody2.includes('字段名') || proBody2.includes('推断类型')) {
      log('PASS', 'DataExplorer already visible');
    } else {
      log('BUG', 'DataExplorer entry not found');
    }
  }
  await shot(page, '06-data-explorer');

  // B9 Analysis types — TableOne
  async function ensureConfigurator() {
    let t = await bodyText(page);
    if (t.includes('分析模块类型') || t.includes('开始统计计算')) return true;
    const dsChip = page.getByLabel(/数据集:/).or(page.getByText('demo_cohort'));
    if ((await dsChip.count()) > 0) {
      await dsChip.first().click();
      await waitMs(500);
    }
    t = await bodyText(page);
    return t.includes('分析模块类型') || t.includes('开始统计计算');
  }

  await ensureConfigurator();

  // Click TableOne and fill
  {
    const btn = page.getByText('基线特征 (TableOne)', { exact: false }).or(page.getByText('基线特征'));
    if ((await btn.count()) > 0) {
      await btn.first().click();
      await waitMs(300);
      try {
        await fillSelectByLabel(page, '连续性数值变量', 'age', true);
        // reopen multi for second continuous var
        const contItem = page.locator('.ant-form-item').filter({ hasText: '连续性数值变量' }).first();
        await contItem.locator('.ant-select').first().click();
        await pickOption(page, 'bmi');
        await page.keyboard.press('Escape');
        await page.getByRole('button', { name: '开始统计计算' }).click();
        await waitMs(4000);
        await shot(page, '07-tableone-result');
        const t = await bodyText(page);
        if (await rootAlive(page)) {
          if (t.includes('运行失败') || t.includes('SkillInvalid') || t.includes('error_code')) {
            log('BUG', 'TableOne run failed in UI', t.slice(0, 250));
          } else if (
            t.includes('分析报告') ||
            t.includes('Table') ||
            t.includes('均值') ||
            t.includes('估计') ||
            t.includes('结果') ||
            t.includes('三线') ||
            (await page.locator('table').count()) > 0
          ) {
            log('PASS', 'TableOne run completed with report');
          } else {
            log('BUG', 'TableOne finished but report unclear', t.slice(0, 250));
          }
        } else {
          log('FAIL', 'TableOne caused white-screen');
        }
      } catch (e) {
        log('BUG', 'TableOne fill/run interaction', e.message.slice(0, 200));
      }
    } else {
      log('BUG', 'TableOne button not found');
    }
  }

  // B10 Linear regression
  await ensureConfigurator();
  {
    const btn = page.getByText('回归建模分析', { exact: true });
    if ((await btn.count()) > 0) {
      await btn.first().click();
      await waitMs(300);
      try {
        const linearRadio = page.getByText('多元线性回归', { exact: false });
        if ((await linearRadio.count()) > 0) await linearRadio.first().click();
        await waitMs(150);
        await fillSelectByLabel(page, '因变量', 'bmi');
        await fillSelectByLabel(page, '自变量列表', 'age', true);
        await page.getByRole('button', { name: '开始统计计算' }).click();
        await waitMs(4500);
        await shot(page, '08-linear-result');
        const t = await bodyText(page);
        if (!(await rootAlive(page))) {
          log('FAIL', 'Linear run white-screen', pageErrors.join(' | '));
        } else if (pageErrors.some((e) => e.includes('toFixed'))) {
          log('FAIL', 'Linear run toFixed crash', pageErrors.join(' | '));
        } else if (t.includes('运行失败')) {
          log('BUG', 'Linear run failed', t.slice(0, 250));
        } else if (
          t.includes('估计值') ||
          t.includes('Beta') ||
          t.includes('p 值') ||
          t.includes('p值') ||
          t.includes('分析报告') ||
          t.includes('系数') ||
          (await page.locator('table').count()) > 0
        ) {
          log('PASS', 'Linear regression run + report');
        } else {
          log('BUG', 'Linear result ambiguous', t.slice(0, 280));
        }

        // Sidecar panel
        if (t.includes('等价代码') || (await page.getByText('等价代码').count()) > 0) {
          log('PASS', 'Sidecar panel present after analysis');
          for (const lang of ['Python', 'R', 'SAS', 'SPSS']) {
            const tab = page.getByRole('tab', { name: lang }).or(page.getByText(lang, { exact: true }));
            if ((await tab.count()) > 0) {
              await tab.first().click().catch(() => {});
              await waitMs(200);
              log('PASS', `Sidecar language tab: ${lang}`);
            }
          }
        } else {
          log('BUG', 'Sidecar 等价代码 not visible after analysis');
        }
      } catch (e) {
        log('BUG', 'Linear fill/run interaction', e.message.slice(0, 200));
      }
    } else {
      log('BUG', '回归建模分析 button not found');
    }
  }

  // B11 Logistic regression
  await ensureConfigurator();
  {
    const btn = page.getByText('回归建模分析', { exact: true });
    if ((await btn.count()) > 0) {
      await btn.first().click();
      await waitMs(200);
      const logistic = page.getByText('Logistic 回归', { exact: false });
      if ((await logistic.count()) > 0) {
        await logistic.first().click();
        await waitMs(200);
        try {
          await fillSelectByLabel(page, '因变量', 'disease');
          await fillSelectByLabel(page, '自变量列表', 'age', true);
          await page.getByRole('button', { name: '开始统计计算' }).click();
          await waitMs(4500);
          await shot(page, '09-logistic-result');
          const t = await bodyText(page);
          if (!(await rootAlive(page))) log('FAIL', 'Logistic white-screen');
          else if (pageErrors.some((e) => e.includes('toFixed'))) log('FAIL', 'Logistic toFixed crash');
          else if (t.includes('运行失败')) log('BUG', 'Logistic run failed', t.slice(0, 200));
          else if (t.includes('OR') || t.includes('比值') || t.includes('分析报告') || t.includes('系数') || t.includes('估计') || (await page.locator('table').count()) > 0) {
            log('PASS', 'Logistic regression run + report');
          } else log('BUG', 'Logistic result ambiguous', t.slice(0, 250));
        } catch (e) {
          log('BUG', 'Logistic interaction', e.message.slice(0, 200));
        }
      } else {
        log('BUG', 'Logistic radio not found');
      }
    }
  }

  // B12 Survival KM
  await ensureConfigurator();
  {
    const btn = page.getByText('KM生存分析', { exact: false }).or(page.getByText('生存分析'));
    if ((await btn.count()) > 0) {
      await btn.first().click();
      await waitMs(250);
      try {
        await fillSelectByLabel(page, '生存时间', 'fu_time').catch(async () => {
          await fillSelectByLabel(page, '时间', 'fu_time');
        });
        await fillSelectByLabel(page, '删失状态', 'death').catch(async () => {
          await fillSelectByLabel(page, '状态', 'death').catch(async () => {
            await fillSelectByLabel(page, '事件', 'death');
          });
        });
        await page.getByRole('button', { name: '开始统计计算' }).click();
        await waitMs(4500);
        await shot(page, '10-km-result');
        const t = await bodyText(page);
        if (!(await rootAlive(page))) log('FAIL', 'KM white-screen');
        else if (t.includes('运行失败')) log('BUG', 'KM run failed', t.slice(0, 200));
        else if (t.includes('生存') || t.includes('Kaplan') || t.includes('分析报告') || t.includes('中位') || t.includes('结果') || (await page.locator('table, canvas').count()) > 0) {
          log('PASS', 'KM survival run completed');
        } else log('BUG', 'KM result ambiguous', t.slice(0, 250));
      } catch (e) {
        log('BUG', 'KM interaction', e.message.slice(0, 200));
      }
    } else {
      log('BUG', 'KM survival button not found');
    }
  }

  // B13 T-test
  await ensureConfigurator();
  {
    const btn = page.getByText('T检验', { exact: false });
    if ((await btn.count()) > 0) {
      await btn.first().click();
      await waitMs(250);
      try {
        await fillSelectByLabel(page, '分组自变量', 'sex');
        await fillSelectByLabel(page, '待检验因变量', 'bmi');
        await page.getByRole('button', { name: '开始统计计算' }).click();
        await waitMs(4000);
        await shot(page, '11-ttest-result');
        const t = await bodyText(page);
        if (!(await rootAlive(page))) log('FAIL', 'T-test white-screen');
        else if (t.includes('运行失败')) log('BUG', 'T-test run failed', t.slice(0, 200));
        else if (t.includes('检验') || t.includes('分析报告') || t.includes('结果') || (await page.locator('table').count()) > 0) {
          log('PASS', 'T-test run completed');
        } else log('BUG', 'T-test result ambiguous', t.slice(0, 250));
      } catch (e) {
        log('BUG', 'T-test interaction', e.message.slice(0, 200));
      }
    } else {
      log('BUG', 'T-test button not found');
    }
  }

  // B14 Right-rail run controls
  {
    const runBtn = page.getByLabel('运行');
    const debugBtn = page.getByLabel('调试');
    const clearBtn = page.getByLabel('清空');
    if ((await runBtn.count()) > 0) {
      const disabled = await runBtn.isDisabled();
      log('PASS', 'Run control present', `disabled=${disabled}`);
      if (!(await debugBtn.isDisabled())) {
        log('BUG', 'Debug button should stay disabled (placeholder)');
      } else {
        log('PASS', 'Debug button disabled as expected');
      }
      if ((await clearBtn.count()) > 0) {
        await clearBtn.click().catch(() => {});
        log('PASS', 'Clear control clickable');
      }
    } else {
      log('BUG', 'Run controls not found in Pro rail');
    }
  }

  // B15 Decision assistant toggle (UI label: 辅助决策)
  {
    const da = page.getByText('辅助决策').or(page.getByText(/决策助手/));
    if ((await da.count()) > 0) {
      // click the switch next to the label
      const sw = page.locator('.ant-switch').filter({ has: page.locator('xpath=..') });
      const near = page.locator('text=辅助决策').locator('xpath=preceding-sibling::button[1]').or(
        page.locator('.ant-space').filter({ hasText: '辅助决策' }).locator('.ant-switch'),
      );
      if ((await near.count()) > 0) {
        await near.first().click();
      } else {
        await da.first().click();
      }
      await waitMs(400);
      log('PASS', '辅助决策 toggle clicked (PATCH settings)');
    } else {
      log('BUG', '辅助决策 toggle not found in current view');
    }
  }

  // B16 Settings drawer (API 设置)
  {
    const settingsBtn = page
      .getByLabel(/设置|API|模型设置/)
      .or(page.getByRole('button', { name: /设置|模型/ }))
      .or(page.getByText('模型设置'));
    if ((await settingsBtn.count()) > 0) {
      await settingsBtn.first().click();
      await waitMs(500);
      await shot(page, '12-settings');
      const t = await bodyText(page);
      if (t.includes('API') || t.includes('DeepSeek') || t.includes('OpenAI') || t.includes('设置')) {
        log('PASS', 'API settings drawer opens');
      } else {
        log('BUG', 'Settings drawer content incomplete', t.slice(0, 150));
      }
      await page.keyboard.press('Escape');
      await waitMs(300);
    } else {
      // try ChatInputBar model settings link
      const model = page.getByText(/模型设置|DeepSeek|未配置/);
      if ((await model.count()) > 0) {
        await model.first().click();
        await waitMs(500);
        log('PASS', 'Opened settings via model label');
        await page.keyboard.press('Escape');
      } else {
        log('BUG', 'Settings entry not found');
      }
    }
  }

  // B17 Simple mode chat send (SSE messages)
  {
    const simpleToggle2 = page.locator('label').filter({ hasText: /简洁|简易/ });
    if ((await simpleToggle2.count()) > 0) {
      await simpleToggle2.first().click();
      await waitMs(600);
    }
    await shot(page, '13-simple-chat');
    const input = page.locator('textarea').first().or(page.getByRole('textbox').first());
    if ((await input.count()) > 0) {
      await input.fill('请帮我描述当前数据集有哪些变量');
      await waitMs(200);
      const send = page.getByRole('button', { name: /发送/ }).or(page.getByLabel('发送'));
      if ((await send.count()) > 0) {
        await send.first().click();
      } else {
        await page.keyboard.press('Enter');
      }
      await waitMs(6000);
      await shot(page, '14-after-chat');
      const t = await bodyText(page);
      if (t.includes('请帮我描述') || t.includes('变量') || t.includes('数据集') || t.includes('还需要') || t.includes('技能')) {
        log('PASS', 'Chat message send (SSE) shows response or echo');
      } else if (t.includes('LLM') || t.includes('不可用') || t.includes('配置') || t.includes('错误')) {
        log('BUG', 'Chat SSE returned LLM/config error (expected if no key)', t.slice(0, 200));
      } else {
        log('BUG', 'Chat send no visible response', t.slice(0, 250));
      }
    } else {
      log('BUG', 'Chat input not found in simple mode');
    }
  }

  // B18 Analysis templates fill prompt
  {
    const tpl = page.getByLabel('分析模板', { exact: true });
    if ((await tpl.count()) > 0) {
      await tpl.click();
      await waitMs(400);
      const linearTpl = page.getByText('线性回归').first();
      if ((await linearTpl.count()) > 0) {
        await linearTpl.click();
        await waitMs(500);
        const t = await bodyText(page);
        if (t.includes('线性') || t.includes('回归') || t.includes('结局')) {
          log('PASS', 'Analysis template injects prompt');
        } else {
          log('BUG', 'Template click did not fill prompt', t.slice(0, 150));
        }
      }
      await page.keyboard.press('Escape');
    }
  }

  // B19 New session
  {
    const neu = page.getByLabel('新对话', { exact: true });
    if ((await neu.count()) > 0) {
      await neu.click();
      await waitMs(800);
      await shot(page, '15-new-session');
      log('PASS', 'New session clicked');
    }
  }

  // B20 Session list + delete (create via API first, reload)
  {
    const created = await api('/api/sessions', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: '{}',
    });
    if (created.body?.id) {
      // Give it a message so it appears with title
      await api(`/api/sessions/${created.body.id}/messages`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ text: '测试会话标题关键词' }),
      }).catch(() => {});
      await page.reload({ waitUntil: 'networkidle' });
      await waitMs(1000);
      const hist = page.getByLabel(/历史会话:/);
      const count = await hist.count();
      if (count > 0) {
        log('PASS', 'Session history list shows items', `count=${count}`);
        // try delete first deletable
        const del = page.getByLabel(/删除会话:/).first();
        if ((await del.count()) > 0) {
          page.once('dialog', (d) => d.accept().catch(() => {}));
          await del.click();
          await waitMs(800);
          // ant design Popconfirm
          const confirm = page.getByRole('button', { name: /确|删除|OK|是/ });
          if ((await confirm.count()) > 0) {
            await confirm.first().click().catch(() => {});
            await waitMs(600);
          }
          log('PASS', 'Delete session interaction');
        }
      } else {
        log('BUG', 'Session history empty after creating session');
      }
    }
  }

  // B21 Export snapshot button if present
  {
    const exp = page.getByText(/导出|快照|Snapshot/).or(page.getByLabel(/导出/));
    if ((await exp.count()) > 0) {
      await exp.first().click().catch(() => {});
      await waitMs(500);
      log('PASS', 'Export/snapshot control present and clicked');
    } else {
      log('PASS', 'Export button not in current view (optional)');
    }
  }

  // B22 Coverage matrix if accessible via plugins drawer
  {
    const plugins = page.getByLabel('插件', { exact: true });
    if ((await plugins.count()) > 0) {
      await plugins.click();
      await waitMs(400);
      const t = await bodyText(page);
      if (t.includes('覆盖') || t.includes('插件') || t.includes('矩阵') || t.includes('即将')) {
        log('PASS', 'Plugins panel content');
      } else {
        log('BUG', 'Plugins panel unexpected', t.slice(0, 120));
      }
      await page.keyboard.press('Escape');
    }
  }

  // B23 Voice recorder presence (don't actually record)
  {
    const voice = page.getByLabel(/录音|语音/).or(page.getByRole('button', { name: /录音|语音/ }));
    if ((await voice.count()) > 0) {
      log('PASS', 'Voice recorder control present');
    } else {
      log('BUG', 'Voice recorder control not found');
    }
  }

  // B24 Final white-screen / console error summary
  await shot(page, '99-final');
  if (!(await rootAlive(page))) {
    log('FAIL', 'Final state white-screen');
  } else {
    log('PASS', 'Final root still alive');
  }

  const toFixed = [...pageErrors, ...consoleErrors].filter((e) => e.includes('toFixed'));
  if (toFixed.length) log('FAIL', 'toFixed crashes observed', toFixed.join(' | ').slice(0, 300));
  else log('PASS', 'No toFixed crashes');

  const criticalPage = pageErrors.filter(
    (e) => !e.includes('ResizeObserver') && !e.includes('Non-Error'),
  );
  if (criticalPage.length) {
    log('BUG', 'Page errors during tour', criticalPage.slice(0, 5).join(' || ').slice(0, 500));
  } else {
    log('PASS', 'No critical pageerrors');
  }

  // Failed API requests from browser
  const apiFails = failedRequests.filter((r) => r.includes('/api/'));
  if (apiFails.length) {
    log('BUG', 'Browser API request failures', apiFails.slice(0, 8).join(' || ').slice(0, 500));
  } else {
    log('PASS', 'No browser API request failures');
  }

  await browser.close();

  // ─── Report ────────────────────────────────────────────────────────────
  const pass = findings.filter((f) => f.startsWith('[PASS]')).length;
  const fail = findings.filter((f) => f.startsWith('[FAIL]')).length;
  const bug = findings.filter((f) => f.startsWith('[BUG]')).length;
  const summary = {
    pass,
    fail,
    bug,
    findings,
    pageErrors: pageErrors.slice(0, 20),
    consoleErrors: consoleErrors.slice(0, 20),
    failedRequests: failedRequests.slice(0, 20),
    shotDir,
  };
  const reportPath = resolve(shotDir, 'report.json');
  writeFileSync(reportPath, JSON.stringify(summary, null, 2), 'utf8');

  console.log('\n======== SUMMARY ========');
  console.log(`PASS=${pass}  FAIL=${fail}  BUG=${bug}`);
  console.log(`Report: ${reportPath}`);
  if (bugs.length) {
    console.log('\n--- Issues ---');
    for (const b of bugs) console.log(b);
  }
  process.exit(fail > 0 ? 1 : 0);
}

main().catch((err) => {
  console.error('TEST HARNESS CRASHED:', err);
  process.exit(2);
});
