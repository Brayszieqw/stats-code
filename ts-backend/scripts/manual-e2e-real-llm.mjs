// Real-LLM journey against the demo exe on :8080.
// Reads the API key from env STATS_E2E_KEY (never hardcode/echo keys).
// Prints the agent's actual streamed replies so a human can judge how
// "agent-like" the behavior feels with a real DeepSeek backend.

const BACKEND = 'http://127.0.0.1:8080';
const KEY = process.env.STATS_E2E_KEY;
if (!KEY) {
  console.error('set STATS_E2E_KEY first');
  process.exit(2);
}

async function api(method, path, body) {
  const res = await fetch(`${BACKEND}${path}`, {
    method,
    headers: { 'content-type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  return { status: res.status, text: await res.text() };
}

function parseSse(text) {
  const events = [];
  for (const frame of text.split(/\n\n/)) {
    const ev = frame.match(/^event:\s*(.+)$/m);
    const data = frame.match(/^data:\s*(.+)$/m);
    if (!ev || !data) continue;
    let payload = {};
    try { payload = JSON.parse(data[1]); } catch { /* ignore */ }
    events.push({ type: ev[1].trim(), ...payload });
  }
  return events;
}

function show(label, events) {
  console.log(`\n===== ${label} =====`);
  for (const e of events) {
    if (e.type === 'text_delta') process.stdout.write(e.text);
    else if (e.type === 'skill_call') console.log(`\n[skill_call] ${e.skill_id} args=${JSON.stringify(e.args)}`);
    else if (e.type === 'skill_result') console.log(`[skill_result] risk=${JSON.stringify(e.result?.risk_signals ?? [])}`);
    else if (e.type === 'interpretation') console.log(`\n[interpretation]\n${e.text}`);
    else if (e.type === 'choice_prompt') console.log(`\n[choice_prompt] ${e.prompt?.question ?? ''}`);
    else if (e.type === 'error') console.log(`\n[error] ${JSON.stringify(e.payload)}`);
  }
  console.log('\n');
}

async function send(sid, text) {
  const r = await api('POST', `/api/sessions/${sid}/messages`, { text, settings: { decision_assistant: false } });
  return parseSse(r.text);
}

// 1. Real DeepSeek config
const cfg = await api('POST', '/api/llm-config', {
  provider: 'deepseek', api_key: KEY, base_url: null, model: null,
});
console.log('llm-config:', cfg.status, cfg.status === 200 ? 'OK' : cfg.text.slice(0, 200));
if (cfg.status !== 200) process.exit(1);

// 2. Session + dataset (with participant_id so the audit passes)
const sid = JSON.parse((await api('POST', '/api/sessions', {})).text).id;
const csv = 'participant_id,age,bmi,sex\nP01,34,22.1,M\nP02,41,24.9,F\nP03,29,21.3,M\nP04,55,27.8,F\nP05,47,26.0,M\nP06,38,23.4,F\nP07,61,28.9,M\nP08,25,20.7,F\nP09,52,25.5,M\nP10,44,24.1,F\nP11,36,23.0,M\nP12,48,26.5,F\nP13,58,27.2,M\nP14,31,21.9,F\nP15,43,24.6,M\nP16,27,20.9,F\nP17,50,26.8,M\nP18,39,23.8,F\nP19,62,29.1,M\nP20,33,22.4,F\n';
const up = await api('POST', `/api/sessions/${sid}/datasets`, { filename: 'demo-bmi.csv', data: Buffer.from(csv).toString('base64') });
console.log('dataset:', up.status);
const dsId = JSON.parse(up.text).dataset_id;

// 3. F1: inspect → auto protocol draft
show('用户：帮我看看这份数据', await send(sid, '帮我看看这份数据'));

// 4. Approve protocol + both plans (simulating the SPA panel actions)
await api('PATCH', `/api/sessions/${sid}/protocol`, {
  status: 'Approved',
  research_question: '成年人年龄与 BMI 是否相关？',
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
for (const [skillId, args] of [
  ['tableone', { group: 'sex', continuous: ['age', 'bmi'], categorical: ['sex'] }],
  ['model_linear', { outcome: 'bmi', predictors: ['age'] }],
]) {
  const audit = await api('POST', `/api/sessions/${sid}/datasets/${dsId}/audit`, { skill_id: skillId, args, expected_protocol_version: 1 });
  const a = JSON.parse(audit.text);
  const appr = await api('POST', `/api/sessions/${sid}/analysis-plans/approve`, {
    skill_id: skillId, dataset_id: dsId, args,
    expected_protocol_version: 1, expected_audit_id: a.audit_id,
    expected_audit_sha256: a.audit_sha256, audit_roles: a.roles,
  });
  console.log(`approve ${skillId}:`, appr.status);
}

// 5. F2: multi-step plan with the real model
show('用户：请按分组 sex 做基线特征表，再用 age 预测 bmi 做线性回归，帮我做完整分析',
  await send(sid, '请按分组 sex 做基线特征表（连续变量 age、bmi，分类变量 sex），再用 age 预测 bmi 做线性回归，帮我做完整分析'));

// 6. F3: interpretation + report wording with the real model
show('用户：请解读刚才的回归结果，并给我一段可以写进论文结果部分的中文措辞',
  await send(sid, '请解读刚才的回归结果，并给我一段可以写进论文结果部分的中文措辞'));
