/**
 * InfiniSynapse 集成 API client（Vibe Coding 参赛）。
 *
 * 与 client.ts 同语义：相对路径走 Vite 代理 / 同源后端，非 2xx 抛 ApiError。
 * 密钥只提交给本地后端保存，前端不落任何存储。
 */

import { ApiError } from './client';
import type { ErrorPayload } from './types';

export interface InfiniStatus {
  configured: boolean;
  base_url: string | null;
}

export interface InfiniTaskStatus {
  is_running: boolean;
  completed: boolean;
  failed: boolean;
  result_text: string | null;
  latest_text: string | null;
  message_count: number;
}

export interface InfiniDataSource {
  id: number | string | null;
  name: string;
  type: string;
  enabled: boolean;
  description: string | null;
}

async function handle<T>(res: Response): Promise<T> {
  if (!res.ok) {
    let payload: ErrorPayload;
    try {
      payload = await res.json();
    } catch {
      payload = { error_code: 'SkillExecutionFailed', message: `HTTP ${res.status}: ${res.statusText}` };
    }
    throw new ApiError(res.status, payload);
  }
  return res.json() as Promise<T>;
}

export async function getInfiniStatus(): Promise<InfiniStatus> {
  return handle<InfiniStatus>(await fetch('/api/infinisynapse/status'));
}

export async function postInfiniConfig(apiKey: string, baseUrl?: string): Promise<InfiniStatus> {
  return handle<InfiniStatus>(
    await fetch('/api/infinisynapse/config', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ api_key: apiKey, ...(baseUrl && baseUrl.trim() ? { base_url: baseUrl.trim() } : {}) }),
    }),
  );
}

export async function postInfiniAnalyze(text: string): Promise<{ task_id: string }> {
  return handle<{ task_id: string }>(
    await fetch('/api/infinisynapse/analyze', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ text }),
    }),
  );
}

export async function getInfiniTask(taskId: string): Promise<InfiniTaskStatus> {
  return handle<InfiniTaskStatus>(await fetch(`/api/infinisynapse/tasks/${encodeURIComponent(taskId)}`));
}

export async function getInfiniTaskFiles(taskId: string): Promise<{ cwd: string; files: string[] }> {
  return handle<{ cwd: string; files: string[] }>(
    await fetch(`/api/infinisynapse/tasks/${encodeURIComponent(taskId)}/files`),
  );
}

export function infiniDownloadUrl(taskId: string): string {
  return `/api/infinisynapse/tasks/${encodeURIComponent(taskId)}/download`;
}

export function infiniFileUrl(taskId: string, path: string): string {
  return `/api/infinisynapse/tasks/${encodeURIComponent(taskId)}/file?path=${encodeURIComponent(path)}`;
}

export async function listInfiniDataSources(): Promise<{ items: InfiniDataSource[] }> {
  return handle<{ items: InfiniDataSource[] }>(await fetch('/api/infinisynapse/datasources'));
}
