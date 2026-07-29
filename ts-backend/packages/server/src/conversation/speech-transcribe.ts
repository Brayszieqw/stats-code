// server/conversation/speech-transcribe.ts — Whisper-compatible STT over a
// generic transcription endpoint.
//
// POST {baseUrl}/audio/transcriptions (multipart) with model whisper-1.
// Uses the same API key / base URL as the chat LLM config so a single
// POST /api/llm-config unlocks both chat and voice.

import type { LlmConfig } from '../state.js';
import { normalizeProviderBaseUrl } from './llm-provider.js';

export interface SpeechTranscript {
  text: string;
  confidence: number;
  auto_processed: boolean;
}

export interface TranscribeAudioOptions {
  bytes: Uint8Array;
  /** MIME type from the client (e.g. audio/webm). */
  contentType?: string;
  /** Original filename hint for the multipart part. */
  filename?: string;
  /** BCP-47 / ISO-639-1 language hint (default zh). */
  language?: string;
  config: LlmConfig;
  fetchImpl?: typeof fetch;
  /** Whisper-compatible model id (default whisper-1). */
  model?: string;
}

export class SpeechTranscribeError extends Error {
  constructor(
    public readonly code: 'LlmUnavailable' | 'SkillInvalidArgs' | 'InternalError',
    message: string,
  ) {
    super(message);
    this.name = 'SpeechTranscribeError';
  }
}

function extensionFor(contentType: string | undefined, filename: string | undefined): string {
  if (filename && filename.includes('.')) {
    return filename.slice(filename.lastIndexOf('.') + 1).toLowerCase();
  }
  const ct = (contentType ?? '').toLowerCase();
  if (ct.includes('webm')) return 'webm';
  if (ct.includes('wav')) return 'wav';
  if (ct.includes('mpeg') || ct.includes('mp3')) return 'mp3';
  if (ct.includes('mp4') || ct.includes('m4a')) return 'm4a';
  if (ct.includes('ogg')) return 'ogg';
  return 'webm';
}

function transcriptionsEndpoint(baseUrl: string): string {
  const trimmed = baseUrl.replace(/\/+$/, '');
  if (trimmed.endsWith('/audio/transcriptions')) return trimmed;
  return `${trimmed}/audio/transcriptions`;
}

/**
 * Transcribe raw audio via a Whisper-compatible transcription endpoint.
 * DeepSeek and other chat-only hosts do not expose Whisper — callers should
 * prefer the browser Web Speech API for chat-only installs, and use this
 * path when the configured base URL supports /audio/transcriptions (e.g. a
 * relay/proxy service compatible with the Whisper transcription API).
 */
export async function transcribeAudio(opts: TranscribeAudioOptions): Promise<SpeechTranscript> {
  const { config } = opts;
  if (!config.api_key?.trim()) {
    throw new SpeechTranscribeError('LlmUnavailable', 'LLM 未配置，无法调用语音转写');
  }

  const baseUrl = normalizeProviderBaseUrl(config.provider, config.base_url);
  const endpoint = transcriptionsEndpoint(baseUrl);
  const fetchImpl = opts.fetchImpl ?? globalThis.fetch;
  const ext = extensionFor(opts.contentType, opts.filename);
  const filename = opts.filename?.includes('.') ? opts.filename : `recording.${ext}`;
  const mime = opts.contentType && opts.contentType.length > 0 ? opts.contentType : `audio/${ext}`;

  // Node 18+ / undici FormData + Blob accept Uint8Array-backed Blobs.
  const form = new FormData();
  const blob = new Blob([Buffer.from(opts.bytes)], { type: mime });
  form.append('file', blob, filename);
  form.append('model', opts.model && opts.model.length > 0 ? opts.model : 'whisper-1');
  form.append('language', opts.language && opts.language.length > 0 ? opts.language : 'zh');
  form.append('response_format', 'json');

  let res: Response;
  try {
    res = await fetchImpl(endpoint, {
      method: 'POST',
      headers: {
        authorization: `Bearer ${config.api_key}`,
      },
      body: form,
    });
  } catch (err) {
    const cause = err instanceof Error ? err.message : String(err);
    throw new SpeechTranscribeError(
      'LlmUnavailable',
      `语音转写网络失败：${cause}。可改用浏览器内置语音识别，或配置兼容 Whisper 转写接口的 Base URL（如中转服务）。`,
    );
  }

  const raw = await res.text().catch(() => '');
  if (!res.ok) {
    const detail = raw.trim().slice(0, 200);
    // DeepSeek and chat-only hosts typically 404 here.
    if (res.status === 404 || res.status === 405) {
      throw new SpeechTranscribeError(
        'LlmUnavailable',
        '当前 API 不支持 Whisper 语音转写（/audio/transcriptions）。' +
          '请使用浏览器内置语音识别，或在设置中配置兼容 Whisper 转写接口的 Base URL（如中转服务）。',
      );
    }
    if (res.status >= 400 && res.status < 500) {
      throw new SpeechTranscribeError(
        'SkillInvalidArgs',
        detail ? `语音转写被拒绝（${res.status}）：${detail}` : `语音转写被拒绝（${res.status}）`,
      );
    }
    throw new SpeechTranscribeError(
      'LlmUnavailable',
      detail ? `语音转写失败（${res.status}）：${detail}` : `语音转写失败（${res.status}）`,
    );
  }

  let text = '';
  try {
    const json = JSON.parse(raw) as { text?: unknown };
    text = typeof json.text === 'string' ? json.text.trim() : '';
  } catch {
    text = raw.trim();
  }

  if (!text) {
    throw new SpeechTranscribeError('SkillInvalidArgs', '语音转写结果为空，请重新录制');
  }

  // Whisper JSON does not always return segment confidences; treat successful
  // cloud STT as high-confidence so the UI auto-sends by default.
  const confidence = 0.85;
  return {
    text,
    confidence,
    auto_processed: confidence >= 0.6,
  };
}
