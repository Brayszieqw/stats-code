// server/conversation/orchestrator.ts — conversation MessageHandler.
//
// Mirrors crates/agent-core/src/orchestrator.rs::process_message: intent
// recognition → decision table → skill dispatch → interpretation → optional
// decision-assistant follow-up → terminal done. The orchestrator only PRODUCES
// AgentEvents; the router serializes them via serializeSseFrame in emission
// order (the SSE contract shapes are unchanged).
//
// The LLM is reached via a provider factory closure that re-reads the current
// persisted config per message, so a runtime POST /api/llm-config applies on
// the next message without a restart (Requirement 10.2).

import { randomUUID } from 'node:crypto';
import type {
  AgentEvent,
  DatasetStore,
  MessageHandler,
  SessionSettings,
  SessionStore,
  UserMessageInput,
} from '../state.js';
import type { LlmEvent, LlmProvider, LlmRequest } from './llm-provider.js';
import type { SkillDescriptor, SkillRegistry } from './skill-registry.js';
import { SkillRunner } from './skill-runner.js';
import { SkillRunErrorException, type SkillResult } from './skill-runner-types.js';

export interface OrchestratorDeps {
  sessionStore: SessionStore;
  datasetStore: DatasetStore;
  registry: SkillRegistry;
  runner: SkillRunner;
  /** Returns a provider from the CURRENT persisted config, or null if unconfigured. */
  llmProviderFactory: () => LlmProvider | null;
}

export interface IntentResult {
  skill_ids: string[];
  resolved_args: Record<string, unknown>;
  has_query_intent: boolean;
  text_response: string | null;
}

interface ChoiceOption {
  option_id: string;
  text: string;
  explanation: string | null;
}

interface ChoicePrompt {
  prompt_id: string;
  question: string;
  options: ChoiceOption[];
  multi_select: boolean;
  allow_custom_text: boolean;
  recommendation: string | null;
}

type Action =
  | { kind: 'ask_choice'; prompt: ChoicePrompt }
  | { kind: 'run_skill'; skillId: string; args: Record<string, unknown> }
  | { kind: 'respond'; text: string }
  | { kind: 'error'; payload: { error_code: string; message: string } };

/** Collect all text deltas from an LLM stream; stop at done/error. */
async function collectStreamText(stream: AsyncIterable<LlmEvent>): Promise<{ text: string; errored: boolean }> {
  let text = '';
  for await (const event of stream) {
    if (event.type === 'text_delta') text += event.text;
    else if (event.type === 'done') break;
    else if (event.type === 'error') return { text, errored: true };
  }
  return { text, errored: false };
}

/** Extract a JSON object from text that may be wrapped in markdown fences. */
function extractJsonFromText(text: string): string {
  const t = text.trim();
  const jsonFence = t.indexOf('```json');
  if (jsonFence !== -1) {
    const start = jsonFence + 7;
    const end = t.indexOf('```', start);
    if (end !== -1) return t.slice(start, end).trim();
  }
  const fence = t.indexOf('```');
  if (fence !== -1) {
    const start = fence + 3;
    const end = t.indexOf('```', start);
    if (end !== -1) return t.slice(start, end).trim();
  }
  const open = t.indexOf('{');
  const close = t.lastIndexOf('}');
  if (open !== -1 && close > open) return t.slice(open, close + 1);
  return t;
}

/** Parse the LLM JSON into an IntentResult; fall back to a text response. */
function parseIntentResponse(text: string): IntentResult {
  const jsonStr = extractJsonFromText(text);
  try {
    const parsed = JSON.parse(jsonStr) as Partial<IntentResult>;
    if (parsed && Array.isArray(parsed.skill_ids)) {
      return {
        skill_ids: parsed.skill_ids.filter((s): s is string => typeof s === 'string'),
        resolved_args:
          parsed.resolved_args && typeof parsed.resolved_args === 'object'
            ? (parsed.resolved_args as Record<string, unknown>)
            : {},
        has_query_intent: parsed.has_query_intent === true,
        text_response: typeof parsed.text_response === 'string' ? parsed.text_response : null,
      };
    }
  } catch {
    // fall through to text response
  }
  return { skill_ids: [], resolved_args: {}, has_query_intent: false, text_response: text };
}

function requiredArgs(desc: SkillDescriptor): string[] {
  const req = desc.inputSchema.required;
  return Array.isArray(req) ? req.filter((v): v is string => typeof v === 'string') : [];
}

function findMissingArgs(desc: SkillDescriptor, resolved: Record<string, unknown>): string[] {
  return requiredArgs(desc).filter((arg) => resolved[arg] === undefined || resolved[arg] === null);
}

export function createOrchestrator(deps: OrchestratorDeps): MessageHandler {
  const { sessionStore, datasetStore, registry, runner, llmProviderFactory } = deps;

  async function addSessionDatasetDefault(
    sessionId: string,
    intent: IntentResult,
  ): Promise<IntentResult> {
    if (intent.skill_ids.length !== 1 || intent.resolved_args.dataset_id != null) {
      return intent;
    }
    const desc = registry.get(intent.skill_ids[0]!);
    if (!desc || !requiredArgs(desc).includes('dataset_id')) {
      return intent;
    }

    try {
      const session = await sessionStore.get(sessionId);
      const latestDataset = session.datasets.at(-1);
      if (!latestDataset) return intent;
      return {
        ...intent,
        resolved_args: {
          ...intent.resolved_args,
          dataset_id: latestDataset.dataset_id,
        },
      };
    } catch {
      return intent;
    }
  }

  function buildSkillDescriptions(): string {
    return registry
      .list()
      .map((desc) => `- ${desc.skillId} (${desc.displayName}): 必需参数=[${requiredArgs(desc).join(', ')}]`)
      .join('\n');
  }

  function buildSkillChoicePrompt(skillIds: string[]): ChoicePrompt {
    const options: ChoiceOption[] = skillIds
      .map((id) => registry.get(id))
      .filter((d): d is SkillDescriptor => d !== undefined)
      .map((desc) => ({
        option_id: desc.skillId,
        text: desc.displayName,
        explanation: `使用 ${desc.displayName} 进行分析`,
      }));
    return {
      prompt_id: randomUUID(),
      question: '检测到多个可能的分析方法，请选择：',
      options,
      multi_select: false,
      allow_custom_text: true,
      recommendation: null,
    };
  }

  function buildMissingArgsPrompt(desc: SkillDescriptor, missing: string[]): ChoicePrompt {
    const properties = (desc.inputSchema.properties as Record<string, { description?: string }> | undefined) ?? {};
    // Do NOT turn raw parameter names into clickable options (e.g. option_id
    // "dataset_id" with label "数据集 ID") — users click them and the system
    // records a fake selection without a real value. Ask for free-form text only.
    const labels = missing.map((arg) => {
      const description = properties[arg]?.description;
      return description && description.length > 0 ? description : arg;
    });
    return {
      prompt_id: randomUUID(),
      question:
        `执行「${desc.displayName}」还需要以下信息：${labels.join('、')}。` +
        '请在下方直接填写（例如：结局变量 bmi，预测变量 age）。',
      options: [],
      multi_select: false,
      allow_custom_text: true,
      recommendation: null,
    };
  }

  function generateFollowUpPrompt(result: SkillResult): ChoicePrompt {
    const options: ChoiceOption[] = [];
    const signals = result.risk_signals;
    if (signals.includes('PValueAboveAlpha')) {
      options.push({ option_id: 'change_model', text: '尝试其他模型', explanation: '当前结果不显著，可能需要换用其他统计方法' });
    }
    if (signals.includes('VifTooHigh')) {
      options.push({ option_id: 'reduce_vars', text: '减少自变量', explanation: '存在多重共线性，建议移除部分高相关变量' });
    }
    if (signals.includes('LowPower')) {
      options.push({ option_id: 'increase_sample', text: '功效分析', explanation: '检验功效不足，建议进行样本量估算' });
    }
    options.push({ option_id: 'sensitivity', text: '做敏感性分析', explanation: '验证结果稳健性' });
    options.push({ option_id: 'add_variables', text: '补充变量', explanation: '加入更多协变量或交互项' });
    options.push({ option_id: 'done', text: '结束分析', explanation: '当前结果已满足需求' });
    const recommendation = signals.length > 0 ? 'sensitivity' : null;
    return {
      prompt_id: randomUUID(),
      question: '分析已完成。您接下来想：',
      options,
      multi_select: false,
      allow_custom_text: true,
      recommendation,
    };
  }

  function intentSystemPrompt(): string {
    const descriptions = buildSkillDescriptions();
    return (
      '你是一个统计分析智能体。根据用户消息识别意图并匹配统计技能。\n' +
      `可用技能列表：\n${descriptions}\n\n` +
      '请以 JSON 格式返回：\n' +
      '{"skill_ids": [匹配的skill_id列表], "resolved_args": {已解析的参数}, ' +
      '"has_query_intent": bool, "text_response": "如无匹配skill则返回文字回复"}\n' +
      '规则：\n' +
      '- 若会话上下文中已有 dataset_id / 结局变量 / 预测变量，请写入 resolved_args，不要重复追问。\n' +
      '- 用户补充的参数（如“结局变量 bmi，预测变量 age”）应合并进 resolved_args。\n' +
      '- 没有匹配技能时 skill_ids 为空数组并给出 text_response。\n'
    );
  }

  /** Compact recent dialogue for multi-turn intent (last few turns only). */
  function formatSessionContext(
    messages: import('../state.js').Message[],
    datasets: { dataset_id: string; file_name: string; columns: { name: string }[] }[],
  ): string {
    const parts: string[] = [];
    if (datasets.length > 0) {
      parts.push(
        '已上传数据集：' +
          datasets
            .map(
              (d) =>
                `${d.file_name} (dataset_id=${d.dataset_id}; 列=${d.columns.map((c) => c.name).join(',')})`,
            )
            .join(' | '),
      );
    } else {
      parts.push('已上传数据集：无');
    }
    const recent = messages.slice(-8);
    if (recent.length > 0) {
      parts.push('最近对话：');
      for (const msg of recent) {
        if ('User' in msg) {
          const c = msg.User.content;
          let text = '';
          if ('Text' in c) text = c.Text;
          else if ('AudioTranscript' in c) text = c.AudioTranscript.text;
          else if ('ChoiceAnswer' in c) {
            const a = c.ChoiceAnswer;
            text = [a.options.join(','), a.custom_text ?? ''].filter(Boolean).join(' | ');
          }
          if (text.trim()) parts.push(`用户: ${text.trim().slice(0, 240)}`);
        } else if ('Agent' in msg) {
          const bits: string[] = [];
          for (const b of msg.Agent.blocks) {
            if ('Text' in b) bits.push(b.Text);
            else if ('ChoicePrompt' in b) bits.push(`[提问] ${b.ChoicePrompt.question}`);
            else if ('SkillResult' in b) bits.push('[已返回统计结果]');
            else if ('Interpretation' in b) bits.push(b.Interpretation.slice(0, 160));
          }
          const joined = bits.join(' ').trim();
          if (joined) parts.push(`助手: ${joined.slice(0, 280)}`);
        }
      }
    }
    return parts.join('\n');
  }

  async function recognizeIntent(
    provider: LlmProvider,
    userText: string,
    sessionId: string,
  ): Promise<{ intent: IntentResult } | { error: { error_code: string; message: string } }> {
    let contextBlock = '';
    try {
      const session = await sessionStore.get(sessionId);
      contextBlock = formatSessionContext(session.messages ?? [], session.datasets ?? []);
    } catch {
      contextBlock = '';
    }
    const system =
      intentSystemPrompt() + (contextBlock ? `\n\n—— 会话上下文 ——\n${contextBlock}\n—— 结束 ——\n` : '');
    const request: LlmRequest = {
      messages: [
        { role: 'system', content: system },
        { role: 'user', content: userText },
      ],
      maxTokens: 1024,
      temperature: 0.1,
    };
    const { text, errored } = await collectStreamText(provider.chatStream(request));
    if (errored) {
      return { error: { error_code: 'LlmUnavailable', message: 'AI 服务暂时不可用' } };
    }
    return { intent: parseIntentResponse(text) };
  }

  function decideAction(intent: IntentResult, settings: SessionSettings): Action {
    if (intent.skill_ids.length === 0) {
      const text =
        intent.text_response ??
        (!settings.decision_assistant && !intent.has_query_intent
          ? '好的。'
          : '我可以帮您进行统计分析。请告诉我您想做什么分析？');
      return { kind: 'respond', text };
    }
    if (intent.skill_ids.length === 1) {
      const skillId = intent.skill_ids[0]!;
      const desc = registry.get(skillId);
      if (!desc) {
        return { kind: 'error', payload: { error_code: 'SkillInvalidArgs', message: `未找到技能：${skillId}` } };
      }
      const missing = findMissingArgs(desc, intent.resolved_args);
      if (missing.length === 0) {
        return { kind: 'run_skill', skillId, args: intent.resolved_args };
      }
      return { kind: 'ask_choice', prompt: buildMissingArgsPrompt(desc, missing) };
    }
    return { kind: 'ask_choice', prompt: buildSkillChoicePrompt(intent.skill_ids) };
  }

  async function generateInterpretation(
    provider: LlmProvider,
    skillId: string,
    result: SkillResult,
  ): Promise<string> {
    const displayName = registry.get(skillId)?.displayName ?? skillId;
    const resultJson = JSON.stringify(result.payload, null, 2);
    const riskInfo = result.risk_signals.length > 0 ? `\n\n检测到的风险信号：${result.risk_signals.join(', ')}` : '';
    const request: LlmRequest = {
      messages: [
        {
          role: 'system',
          content: `你是一个统计分析专家。请对以下 ${displayName} 的分析结果进行解读。\n分析结果：\n${resultJson}${riskInfo}\n`,
        },
        { role: 'user', content: '请解读上述分析结果。' },
      ],
      maxTokens: 2048,
      temperature: 0.3,
    };
    const { text, errored } = await collectStreamText(provider.chatStream(request));
    if (errored || text.length === 0) return '解读生成失败，请稍后重试。';
    return text;
  }

  /** Resolve the dataset bytes/summary for a skill run from session + store. */
  async function loadDatasetContext(
    sessionId: string,
    args: Record<string, unknown>,
  ): Promise<{ bytes: Uint8Array; summary: import('../state.js').DatasetSummary }> {
    const session = await sessionStore.get(sessionId);
    const datasetId = typeof args.dataset_id === 'string' ? args.dataset_id : undefined;
    let summary = datasetId ? session.datasets.find((d) => d.dataset_id === datasetId) : undefined;
    // Fall back to the most recently uploaded dataset when none is named.
    if (!summary && session.datasets.length > 0) {
      summary = session.datasets[session.datasets.length - 1];
    }
    if (!summary) {
      throw new SkillRunErrorException({
        kind: 'invalid_args',
        missing: ['dataset_id'],
        message: '在当前会话中未找到数据集',
      });
    }
    const bytes = await datasetStore.readRawById(summary.dataset_id);
    return { bytes, summary };
  }

  async function* handleMessage(sessionId: string, input: UserMessageInput): AsyncIterable<AgentEvent> {
    const provider = llmProviderFactory();
    if (!provider) {
      yield { type: 'error', payload: { error_code: 'LlmUnavailable', message: 'LLM 未配置' } };
      yield { type: 'done' };
      return;
    }

    const recognized = await recognizeIntent(provider, input.text, sessionId);
    if ('error' in recognized) {
      yield { type: 'error', payload: recognized.error };
      yield { type: 'done' };
      return;
    }

    const intent = await addSessionDatasetDefault(sessionId, recognized.intent);
    const action = decideAction(intent, input.settings);

    switch (action.kind) {
      case 'ask_choice':
        yield { type: 'choice_prompt', prompt: action.prompt };
        break;
      case 'respond':
        yield { type: 'text_delta', text: action.text };
        break;
      case 'error':
        yield { type: 'error', payload: action.payload };
        break;
      case 'run_skill': {
        const desc = registry.get(action.skillId)!;
        yield { type: 'skill_call', skill_id: action.skillId, args: action.args };
        let result: SkillResult;
        try {
          const ctx = await loadDatasetContext(sessionId, action.args);
          result = await runner.run(desc, action.args, {
            datasetBytes: ctx.bytes,
            datasetSummary: ctx.summary,
          });
        } catch (err) {
          yield { type: 'error', payload: mapSkillError(err) };
          break;
        }
        // Exactly one skill_result, then at least one interpretation AFTER it.
        yield { type: 'skill_result', result };
        const interpretation = await generateInterpretation(provider, action.skillId, result);
        yield { type: 'interpretation', text: interpretation };
        // Decision-assistant follow-up (Requirement 8.4).
        if (input.settings.decision_assistant) {
          yield { type: 'choice_prompt', prompt: generateFollowUpPrompt(result) };
        }
        break;
      }
    }

    yield { type: 'done' };
  }

  return { handleMessage };
}

/** Map a thrown skill error to an AgentEvent error payload. */
function mapSkillError(err: unknown): { error_code: string; message: string } {
  if (err instanceof SkillRunErrorException) {
    const d = err.detail;
    switch (d.kind) {
      case 'timeout':
        return { error_code: 'SkillTimeout', message: `统计任务执行超时（超过 ${d.wallSecs} 秒）` };
      case 'invalid_args':
        return { error_code: 'SkillInvalidArgs', message: d.message };
      case 'execution_failed':
        return { error_code: 'SkillExecutionFailed', message: `统计任务失败：${d.diagnosticExcerpt}` };
    }
  }
  return { error_code: 'SkillExecutionFailed', message: (err as Error).message ?? 'unknown error' };
}
