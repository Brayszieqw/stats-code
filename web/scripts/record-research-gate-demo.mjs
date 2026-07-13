/**
 * Record the research-integrity gate as a real browser flow:
 * issue upload -> server audit block -> corrected upload -> audit pass ->
 * server plan approval -> deterministic run.
 *
 * Prerequisites: the API and web UI are already running.
 * Run from web/: npm run demo:research-gate
 */
import { chromium } from 'playwright';
import {
  copyFileSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const WEB_URL = process.env.STATS_URL || 'http://127.0.0.1:5173';
const API_URL = process.env.API_URL || 'http://127.0.0.1:8080';
const HEADED = process.env.HEADED === '1';
const SLOW_MO = Number(process.env.DEMO_SLOW_MO || 80);
const STEP_PAUSE = Number(process.env.DEMO_STEP_PAUSE || 900);
const here = dirname(fileURLToPath(import.meta.url));
const issuePath = resolve(here, '../public/demo_cohort_with_issues.csv');
const cleanPath = resolve(here, '../public/demo_cohort.csv');
const outDir = resolve(here, '../output/playwright/research-gate-demo');
const reportPath = join(outDir, 'report.json');
const finalVideoPath = join(outDir, 'research-gate-demo.webm');

const RESERVED_ARG_KEYS = new Set([
  'workflow_approval',
  'plan_id',
  'plan_approval_id',
  'approval_id',
  'approved_at',
  'plan_approved_at',
  'protocol_approved_at',
  'audit_status',
  'audit_sha256',
  'blockers',
]);

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

function responsePath(response) {
  return new URL(response.url()).pathname;
}

function requestBody(request) {
  try {
    return request.postDataJSON();
  } catch {
    return null;
  }
}

function findReservedArgKey(value, path = 'args') {
  if (Array.isArray(value)) {
    for (let index = 0; index < value.length; index += 1) {
      const found = findReservedArgKey(value[index], `${path}[${index}]`);
      if (found) return found;
    }
    return null;
  }
  if (!value || typeof value !== 'object') return null;
  for (const [key, child] of Object.entries(value)) {
    if (RESERVED_ARG_KEYS.has(key)) return `${path}.${key}`;
    const found = findReservedArgKey(child, `${path}.${key}`);
    if (found) return found;
  }
  return null;
}

async function apiJson(path, init = {}) {
  const response = await fetch(`${API_URL}${path}`, init);
  const text = await response.text();
  let body;
  try {
    body = JSON.parse(text);
  } catch {
    body = text;
  }
  if (!response.ok) {
    throw new Error(`${init.method || 'GET'} ${path} -> ${response.status}: ${text.slice(0, 300)}`);
  }
  return body;
}

async function jsonFromResponse(response, label) {
  const text = await response.text();
  let body;
  try {
    body = JSON.parse(text);
  } catch {
    body = text;
  }
  if (!response.ok()) {
    throw new Error(`${label} -> ${response.status()}: ${text.slice(0, 300)}`);
  }
  return body;
}

async function waitUntil(check, message, timeoutMs = 15_000) {
  const deadline = Date.now() + timeoutMs;
  let lastError;
  while (Date.now() < deadline) {
    try {
      if (await check()) return;
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 100));
  }
  throw new Error(`${message}${lastError ? `: ${lastError.message}` : ''}`);
}

async function pause(page) {
  if (STEP_PAUSE > 0) await page.waitForTimeout(STEP_PAUSE);
}

async function screenshot(page, name) {
  await page.screenshot({
    path: join(outDir, `${name}.png`),
    animations: 'disabled',
    caret: 'hide',
  });
}

async function uploadThroughUi(page, sessionId, path, fileName) {
  await page.locator('.pro-thread-actions').getByRole('button', { name: '上传数据集' }).click();
  const drawer = page.locator('.ant-drawer:visible').filter({ hasText: '上传数据集' });
  await drawer.waitFor({ state: 'visible' });
  const resetLink = drawer.getByText('重新上传', { exact: true });
  if (await resetLink.count()) await resetLink.click();

  const responsePromise = page.waitForResponse((response) => (
    response.request().method() === 'POST'
    && responsePath(response) === `/api/sessions/${sessionId}/datasets`
  ));
  await drawer.locator('input[type="file"]').setInputFiles(path);
  const response = await responsePromise;
  const dataset = await jsonFromResponse(response, `upload ${fileName}`);
  assert(dataset.file_name === fileName, `uploaded filename mismatch: ${dataset.file_name}`);
  await page.getByRole('button', { name: `数据集: ${fileName}` }).waitFor({ state: 'visible' });
  return dataset;
}

async function chooseSelect(page, workspace, itemLabel, optionPrefix, multiple = false) {
  const item = workspace.locator('.ant-form-item').filter({ hasText: itemLabel }).first();
  await item.locator('.ant-select').first().click();
  await item.locator('input[role="combobox"]').first().fill(optionPrefix);
  const dropdown = page.locator('.ant-select-dropdown:visible').last();
  await dropdown.waitFor({ state: 'visible' });
  const option = dropdown.locator('.ant-select-item-option').filter({
    hasText: new RegExp(`^${optionPrefix}(?:\\s|$)`),
  }).first();
  await option.click({ force: true });
  if (multiple) await page.keyboard.press('Escape');
}

async function submitLogistic(page, sessionId, datasetId) {
  const workspace = page.getByLabel('分析检查器');
  await workspace.getByText('回归建模分析', { exact: true }).click();
  await workspace.getByText('Logistic 回归 (二分类Y)', { exact: true }).click();
  await chooseSelect(page, workspace, '因变量 (Dependent Variable Y)', 'disease');
  await chooseSelect(page, workspace, '自变量列表 (Independent Variables X)', 'age', true);

  const auditPromise = page.waitForResponse((response) => (
    response.request().method() === 'POST'
    && responsePath(response) === `/api/sessions/${sessionId}/datasets/${datasetId}/audit`
  ));
  await workspace.getByRole('button', { name: '开始统计计算' }).click();
  const response = await auditPromise;
  return {
    audit: await jsonFromResponse(response, 'dataset audit'),
    request: requestBody(response.request()),
  };
}

async function run() {
  mkdirSync(outDir, { recursive: true });
  rmSync(finalVideoPath, { force: true });
  const startedAt = new Date().toISOString();
  const report = {
    status: 'running',
    started_at: startedAt,
    web_url: WEB_URL,
    api_url: API_URL,
    steps: [],
    requests: {},
    artifacts: [],
    console_errors: [],
    page_errors: [],
  };

  let browser;
  let context;
  let page;
  let video;
  let failure;

  const mark = (name, evidence = {}) => {
    report.steps.push({ name, at: new Date().toISOString(), ...evidence });
    console.log(`[PASS] ${name}`);
  };

  try {
    assert(readFileSync(issuePath, 'utf8').includes('P004,1,1'), 'issue demo file is missing the duplicate key row');
    assert(readFileSync(cleanPath, 'utf8').startsWith('participant_id,'), 'clean demo file has no participant key');
    await apiJson('/api/health');
    const session = await apiJson('/api/sessions', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: '{}',
    });
    report.session_id = session.id;

    browser = await chromium.launch({ headless: !HEADED, slowMo: Number.isFinite(SLOW_MO) ? SLOW_MO : 80 });
    context = await browser.newContext({
      viewport: { width: 1600, height: 1000 },
      locale: 'zh-CN',
      colorScheme: 'light',
      recordVideo: { dir: outDir, size: { width: 1600, height: 1000 } },
    });
    await context.addInitScript(() => {
      window.localStorage.setItem('dual-mode-frontend.mode', 'pro');
    });
    page = await context.newPage();
    video = page.video();
    page.on('pageerror', (error) => report.page_errors.push(error.message));
    page.on('console', (message) => {
      if (message.type() === 'error') report.console_errors.push(message.text());
    });

    await page.goto(`${WEB_URL}/?session_id=${encodeURIComponent(session.id)}`, {
      waitUntil: 'networkidle',
      timeout: 60_000,
    });
    const skipLlmSetup = page.getByRole('button', { name: '暂不配置，进入专业模式' });
    if (await skipLlmSetup.isVisible().catch(() => false)) await skipLlmSetup.click();
    await page.getByText('专业统计分析', { exact: true }).waitFor({ state: 'visible' });

    const protocolResponsePromise = page.waitForResponse((response) => (
      response.request().method() === 'PATCH'
      && responsePath(response) === `/api/sessions/${session.id}/protocol`
    ));
    await page.getByRole('button', { name: '打开研究协议' }).click();
    const protocolDrawer = page.locator('.ant-drawer:visible').filter({ hasText: '研究协议卡' });
    await protocolDrawer.getByRole('button', { name: '加载演示协议' }).click();
    await protocolDrawer.getByRole('button', { name: '审批协议' }).click();
    const protocolResponse = await protocolResponsePromise;
    const protocolRequest = requestBody(protocolResponse.request());
    const protocolSession = await jsonFromResponse(protocolResponse, 'protocol approval');
    assert(protocolSession.research_protocol?.status === 'Approved', 'server did not approve protocol');
    assert(!('approved_at' in protocolRequest), 'protocol request must not self-report approved_at');
    assert(!('approval_id' in protocolRequest), 'protocol request must not self-report approval_id');
    assert(!('content_sha256' in protocolRequest), 'protocol request must not self-report content hash');
    report.requests.protocol = protocolRequest;
    report.protocol = {
      version: protocolSession.research_protocol.version,
      content_sha256: protocolSession.research_protocol.content_sha256,
      approval_id: protocolSession.research_protocol.approval_id,
    };
    await protocolDrawer.waitFor({ state: 'hidden' });
    await screenshot(page, '01-protocol-approved');
    mark('服务端审批研究协议', report.protocol);
    await pause(page);

    const issueDataset = await uploadThroughUi(page, session.id, issuePath, 'demo_cohort_with_issues.csv');
    await screenshot(page, '02-issue-dataset-uploaded');
    mark('通过 UI 上传问题数据', { dataset_id: issueDataset.dataset_id, rows: issueDataset.row_count });
    await pause(page);

    const blocked = await submitLogistic(page, session.id, issueDataset.dataset_id);
    report.requests.blocked_audit = blocked.request;
    assert(blocked.request.expected_protocol_version === report.protocol.version, 'blocked audit did not bind protocol version');
    assert(!('approved_at' in blocked.request), 'audit request must not self-report approval time');
    assert(blocked.audit.status === 'blocked', `issue audit unexpectedly returned ${blocked.audit.status}`);
    const blockerCodes = blocked.audit.findings
      .filter((finding) => finding.severity === 'blocker')
      .map((finding) => finding.code);
    for (const code of ['DUPLICATE_PRIMARY_KEY', 'EVENT_ENCODING_INVALID', 'PERSON_TIME_NONPOSITIVE']) {
      assert(blockerCodes.includes(code), `expected blocker ${code}, got ${blockerCodes.join(', ')}`);
      await page.getByText(code, { exact: true }).waitFor({ state: 'visible' });
    }
    const blockedApprove = page.getByRole('button', { name: '批准方案并运行' });
    assert(await blockedApprove.isDisabled(), 'approval button must stay disabled for blocked audit');
    await screenshot(page, '03-server-audit-blocked');
    mark('服务端审计发现错误并阻断', {
      audit_id: blocked.audit.audit_id,
      audit_sha256: blocked.audit.audit_sha256,
      blocker_codes: blockerCodes,
    });
    await pause(page);
    await page.locator('.analysis-preflight-modal:visible .ant-modal-close').click();

    const cleanDataset = await uploadThroughUi(page, session.id, cleanPath, 'demo_cohort.csv');
    await screenshot(page, '04-corrected-dataset-uploaded');
    mark('通过 UI 换成修正数据', { dataset_id: cleanDataset.dataset_id, rows: cleanDataset.row_count });
    await pause(page);

    const passed = await submitLogistic(page, session.id, cleanDataset.dataset_id);
    report.requests.passed_audit = passed.request;
    assert(passed.request.expected_protocol_version === report.protocol.version, 'passed audit did not bind protocol version');
    assert(!('approved_at' in passed.request), 'audit request must not self-report approval time');
    assert(passed.audit.status === 'passed', `clean audit unexpectedly returned ${passed.audit.status}`);
    await page.getByText('服务端审计通过，未发现阻断项', { exact: true }).waitFor({ state: 'visible' });
    const approveButton = page.getByRole('button', { name: '批准方案并运行' });
    await waitUntil(() => approveButton.isEnabled(), 'approval button did not become enabled');
    await screenshot(page, '05-server-audit-passed');
    mark('修正数据通过服务端审计', {
      audit_id: passed.audit.audit_id,
      audit_sha256: passed.audit.audit_sha256,
    });
    await pause(page);

    const approvalPromise = page.waitForResponse((response) => (
      response.request().method() === 'POST'
      && responsePath(response) === `/api/sessions/${session.id}/analysis-plans/approve`
    ));
    const runPromise = page.waitForResponse((response) => (
      response.request().method() === 'POST'
      && responsePath(response) === `/api/sessions/${session.id}/run`
    ));
    await approveButton.click();
    const approvalResponse = await approvalPromise;
    const approvalBody = requestBody(approvalResponse.request());
    const approval = await jsonFromResponse(approvalResponse, 'analysis plan approval');
    const runResponse = await runPromise;
    const runBody = requestBody(runResponse.request());
    const runResult = await jsonFromResponse(runResponse, 'deterministic run');
    report.requests.plan_approval = approvalBody;
    report.requests.run = runBody;

    assert(approval.audit_id === passed.audit.audit_id, 'approved plan does not bind the reviewed audit id');
    assert(approval.audit_sha256 === passed.audit.audit_sha256, 'approved plan does not bind the reviewed audit hash');
    assert(approvalBody.expected_audit_id === passed.audit.audit_id, 'approval request lost reviewed audit id');
    assert(approvalBody.expected_audit_sha256 === passed.audit.audit_sha256, 'approval request lost reviewed audit hash');
    assert(!('approved_at' in approvalBody), 'plan approval request must not self-report approval time');
    assert(runBody.plan_id === approval.plan_id, 'run request does not use server-issued plan id');
    assert(!findReservedArgKey(runBody.args), `run args contain server-owned field ${findReservedArgKey(runBody.args)}`);
    assert(runResult.analysis?.plan_id === approval.plan_id, 'result metadata does not preserve the approved plan id');
    await page.getByText('分析报告结果', { exact: true }).waitFor({ state: 'visible', timeout: 30_000 });
    await screenshot(page, '06-plan-approved-and-run-complete');
    mark('服务端批准方案并完成确定性运行', {
      plan_id: approval.plan_id,
      run_id: runResult.analysis?.run_id,
    });
    await pause(page);

    assert(report.page_errors.length === 0, `page errors observed: ${report.page_errors.join(' | ')}`);
    report.status = 'passed';
    report.completed_at = new Date().toISOString();
  } catch (error) {
    failure = error;
    report.status = 'failed';
    report.error = error instanceof Error ? error.message : String(error);
    report.completed_at = new Date().toISOString();
    if (page) {
      await screenshot(page, '99-failure').catch(() => {});
    }
  } finally {
    await context?.close().catch(() => {});
    await browser?.close().catch(() => {});
    if (video) {
      const generatedVideo = await video.path().catch(() => null);
      if (generatedVideo) {
        copyFileSync(generatedVideo, finalVideoPath);
        if (resolve(generatedVideo) !== resolve(finalVideoPath)) rmSync(generatedVideo, { force: true });
        report.artifacts.push(finalVideoPath);
      }
    }
    for (const name of [
      '01-protocol-approved.png',
      '02-issue-dataset-uploaded.png',
      '03-server-audit-blocked.png',
      '04-corrected-dataset-uploaded.png',
      '05-server-audit-passed.png',
      '06-plan-approved-and-run-complete.png',
    ]) {
      report.artifacts.push(join(outDir, name));
    }
    writeFileSync(reportPath, `${JSON.stringify(report, null, 2)}\n`, 'utf8');
  }

  if (failure) throw failure;
  console.log(`Video: ${finalVideoPath}`);
  console.log(`Report: ${reportPath}`);
  console.log('RESEARCH GATE DEMO PASSED');
}

run().catch((error) => {
  console.error('RESEARCH GATE DEMO FAILED:', error instanceof Error ? error.message : error);
  process.exitCode = 1;
});
