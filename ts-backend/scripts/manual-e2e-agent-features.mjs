// Manual E2E test for the three new agent features against a LIVE dev server.
// Simulates a real user journey:
//   0. spin up a mock OpenAI-compatible LLM (scripted per-call responses)
//   1. POST /api/llm-config to point the backend at the mock
//   2. create session, upload a small CSV
//   3. Feature 1: "帮我看看数据" (inspect) → expect auto protocol draft
//   3.5 approve protocol + audit/approve both planned analyses (SPA flow)
//   4. Feature 2: "帮我做完整分析" → LLM returns plan → sequential steps
//   5. Feature 3: "帮我解读结果并给报告措辞" → text answer with result context
//
// Run: node scripts/manual-e2e-agent-features.mjs   (dev server on :8080)

import http from 'node:http';

const BACKEND = 'http://127.0.0.1:8080';
const MOCK_PORT = 18099;

// ---------------------------------------------------------------- mock LLM
const llmCalls = [];

function sseChunk(text) {
  return `data: ${JSON.stringify({ choices: [{ delta: { content: text } }] })}\n\n`;
}

function mockResponseFor(body) {
  const system = body.messages?.[0]?.content ?? '';
  const user = body.messages?.[1]?.content ?? '';
  llmCalls.push({ system: system.slice(0, 60), user });

  if (user === 'ping') return 'pong';

  if (system.includes('智能统计助手')) {
    const req = JSON.parse(user);
    const text = req.current_request ?? '';
    if (text.includes('看看数据')) {
      return JSON.stringify({
        skill_ids: ['inspect'],
        resolved_args: {},
        has_query_intent: true,
        text_response: null,
      });
    }
    if (text.includes('完整分析')) {
      return JSON.stringify({
        skill_ids: [],
        resolved_args: { outcome: 'bmi', predictors: ['age'], group: 'sex', continuous: ['age', 'bmi'], categorical: ['sex'] },
        has_query_intent: true,
        text_response: null,
        plan: ['tableone', 'model_linear'],
      });
    }
    if (text.includes('解读') || text.includes('报告')) {
      const hasResultCtx = (req.session_context ?? '').includes('最近统计结果');
      return JSON.stringify({
        skill_ids: [],
        resolved_args: {},
        has_query_intent: true,
        text_response: hasResultCtx
          ? '基于最近的线性回归结果：可在报告中写"年龄与 BMI 的关联为每岁 β=…（95%CI …）"。CONTEXT_OK'
          : '我看不到任何统计结果上下文。CONTEXT_MISSING',
      });
    }
    return JSON.stringify({ skill_ids: [], resolved_args: {}, has_query_intent: false, text_response: '好的，请继续。' });
  }

  if (system.includes('研究协议起草助手')) {
    return JSON.stringify({
      research_question: '年龄与 BMI 是否相关？',
      study_design: 'cross_sectional',
      population: '成年体检人群',
      outcome: 'bmi',
      primary_analysis: '线性回归',
    });
  }

  if (system.includes('统计结果解读助手')) {
    return '本次分析显示 age 的回归系数为正（示例解读，含数字 0.12），提示年龄与 BMI 存在正向关联。';
  }

  return '（mock 未识别的调用）';
}

const mock = http.createServer((req, res) => {
  let buf = '';
  req.on('data', (d) => (buf += d));
  req.on('end', () => {
    let body = {};
    try { body = JSON.parse(buf); } catch { /* ignore */ }
    const text = mockResponseFor(body);
    res.writeHead(200, { 'content-type': 'text/event-stream' });
    res.write(sseChunk(text));
    res.write('data: [DONE]\n\n');
    res.end();
  });
});

// ---------------------------------------------------------------- helpers
async function api(method, path, body) {
  const res = await fetch(`${BACKEND}${path}`, {
    method,
    headers: { 'content-type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const textBody = await res.text();
  return { status: res.status, text: textBody };
}

function parseSse(text) {
  const events = [];
  for (const frame of text.split(/\n\n/)) {
    const evLine = frame.match(/^event:\s*(.+)$/m);
    const dataLine = frame.match(/^data:\s*(.+)$/m);
    if (!evLine || !dataLine) continue;
    let payload = {};
    try { payload = JSON.parse(dataLine[1]); } catch { /* keep {} */ }
    events.push({ type: evLine[1].trim(), ...payload });
  }
  return events;
}

async function sendMessage(sid, text) {
  const { status, text: body } = await api('POST', `/api/sessions/${sid}/messages`, {
    text,
    settings: { decision_assistant: false },
  });
  return { status, events: parseSse(body) };
}

const results = [];
function check(name, ok, detail) {
  results.push({ name, ok, detail });
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${detail ? `  — ${detail}` : ''}`);
}

// ---------------------------------------------------------------- journey
await new Promise((resolve) => mock.listen(MOCK_PORT, '127.0.0.1', resolve));
console.log(`mock LLM on :${MOCK_PORT}`);

try {
  const cfg = await api('POST', '/api/llm-config', {
    provider: 'custom',
    api_key: 'sk-mock-not-a-real-key',
    base_url: `http://127.0.0.1:${MOCK_PORT}/v1`,
    model: 'mock-model',
  });
  check('LLM config accepted', cfg.status === 200, `status=${cfg.status} ${cfg.text.slice(0, 120)}`);

  const s = await api('POST', '/api/sessions', {});
  const sid = JSON.parse(s.text).id;
  const csv = 'participant_id,age,bmi,sex\nP01,34,22.1,M\nP02,41,24.9,F\nP03,29,21.3,M\nP04,55,27.8,F\nP05,47,26.0,M\nP06,38,23.4,F\nP07,61,28.9,M\nP08,25,20.7,F\nP09,52,25.5,M\nP10,44,24.1,F\n';
  const up = await api('POST', `/api/sessions/${sid}/datasets`, {
    filename: 'demo.csv',
    data: Buffer.from(csv, 'utf8').toString('base64'),
  });
  check('dataset uploaded', up.status === 201, `status=${up.status} ${up.text.slice(0, 120)}`);

  // Feature 1 — inspect → auto protocol draft
  const r1 = await sendMessage(sid, '帮我看看数据');
  const r1types = r1.events.map((e) => e.type);
  const draftText = r1.events.filter((e) => e.type === 'text_delta').map((e) => e.text).join('');
  check('F1: inspect ran', r1types.includes('skill_result'), r1types.join('→'));
  check(
    'F1: protocol draft emitted',
    draftText.includes('研究协议草稿') && draftText.includes('research_question'),
    draftText.slice(0, 160).replace(/\n/g, ' '),
  );

  // Real users approve the protocol + plans before gated analyses (SPA flow).
  const prot = await api('PATCH', `/api/sessions/${sid}/protocol`, {
    status: 'Approved',
    research_question: '年龄与 BMI 是否相关？',
    study_design: 'cross_sectional',
    population: '成年体检人群',
    eligibility_criteria: '每人一行',
    exposure: 'age',
    comparator: 'age 每增加 1 岁',
    outcome: 'bmi',
    time_zero: '基线',
    follow_up: '横断面',
    analysis_unit: '参与者',
    estimand: 'age 每增加 1 岁对应的平均 bmi 差',
    confounders: '',
    missing_data_strategy: '完整案例',
    primary_analysis: '线性回归',
    sensitivity_analysis: '',
  });
  check('protocol approved', prot.status === 200, `status=${prot.status} ${prot.text.slice(0, 100)}`);
  const dsId = JSON.parse(up.text).dataset_id;
  for (const [skillId, args] of [
    ['tableone', { group: 'sex', continuous: ['age', 'bmi'], categorical: ['sex'] }],
    ['model_linear', { outcome: 'bmi', predictors: ['age'] }],
  ]) {
    const audit = await api('POST', `/api/sessions/${sid}/datasets/${dsId}/audit`, {
      skill_id: skillId, args, expected_protocol_version: 1,
    });
    if (audit.status !== 200) {
      check(`plan approved: ${skillId}`, false, `audit status=${audit.status} ${audit.text.slice(0, 160)}`);
      continue;
    }
    const auditBody = JSON.parse(audit.text);
    const appr = await api('POST', `/api/sessions/${sid}/analysis-plans/approve`, {
      skill_id: skillId, dataset_id: dsId, args,
      expected_protocol_version: 1,
      expected_audit_id: auditBody.audit_id,
      expected_audit_sha256: auditBody.audit_sha256,
      audit_roles: auditBody.roles,
    });
    check(`plan approved: ${skillId}`, appr.status === 201, `status=${appr.status} ${appr.text.slice(0, 120)}`);
  }

  // Feature 2 — multi-step plan
  const r2 = await sendMessage(sid, '帮我做完整分析');
  const r2types = r2.events.map((e) => e.type);
  const skillResults = r2.events.filter((e) => e.type === 'skill_result');
  const planText = r2.events.filter((e) => e.type === 'text_delta').map((e) => e.text).join('');
  check('F2: plan announced', planText.includes('已生成分析计划'), planText.slice(0, 120).replace(/\n/g, ' '));
  check(
    'F2: multiple skills executed sequentially',
    skillResults.length >= 2,
    `skill_result count=${skillResults.length}; types=${r2types.join('→')}`,
  );
  const interp = r2.events.filter((e) => e.type === 'interpretation').map((e) => e.text).join(' ');
  check(
    'F2: interpretation allows numbers (not censored)',
    interp.includes('0.12'),
    interp.slice(0, 140).replace(/\n/g, ' '),
  );

  // Feature 3 — interpretation request sees result context
  const r3 = await sendMessage(sid, '请帮我解读刚才的结果，并给出报告措辞建议');
  const r3text = r3.events.filter((e) => e.type === 'text_delta').map((e) => e.text).join('');
  check('F3: LLM answered interpretation request (no refusal)', r3text.includes('CONTEXT_OK'), r3text.slice(0, 200).replace(/\n/g, ' '));

  const lastIntent = llmCalls.filter((c) => c.system.includes('智能统计助手')).at(-1);
  const ctxHasResult = lastIntent !== undefined && lastIntent.user.includes('最近统计结果');
  check('F3: session context contained 最近统计结果', ctxHasResult, lastIntent ? lastIntent.user.slice(0, 120) : 'no intent call recorded');
} finally {
  mock.close();
}

const failed = results.filter((r) => !r.ok);
console.log(`\n${results.length - failed.length}/${results.length} checks passed`);
process.exit(failed.length === 0 ? 0 : 1);
