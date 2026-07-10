/**
 * SSE chat hook — parses POST-based SSE stream from agent-server,
 * dispatches AgentEvent variants to local state.
 *
 * Validates: Requirements 9.5, 14.4
 */

import { useCallback, useRef, useState } from 'react';
import { postMessageFetch, ApiError } from '../api/client';
import type {
  ChoicePrompt,
  SkillResult,
  ErrorPayload,
} from '../api/types';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface ChatMessage {
  id: string;
  role: 'user' | 'agent';
  content: string;
  choicePrompt?: ChoicePrompt;
  skillResult?: SkillResult;
  interpretation?: string;
  timestamp: Date;
}

export type ConnectionStatus = 'idle' | 'connecting' | 'streaming' | 'error';

export interface UseSseChatReturn {
  messages: ChatMessage[];
  setMessages: React.Dispatch<React.SetStateAction<ChatMessage[]>>;
  sendMessage: (text: string) => void;
  status: ConnectionStatus;
  error: ErrorPayload | null;
  isStreaming: boolean;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

let messageCounter = 0;

function generateId(): string {
  return `msg-${Date.now()}-${++messageCounter}`;
}

/**
 * Parse a single SSE frame from raw text lines.
 * Returns { event, data } or null if incomplete/comment.
 */
interface SseFrame {
  event: string;
  data: string;
}

function parseSseLines(lines: string[]): SseFrame | null {
  let event = '';
  let data = '';

  for (const line of lines) {
    if (line.startsWith('event:')) {
      event = line.slice(6).trim();
    } else if (line.startsWith('data:')) {
      data = line.slice(5).trim();
    }
  }

  if (!event && !data) return null;
  return { event: event || 'message', data };
}

// ---------------------------------------------------------------------------
// Hook
// ---------------------------------------------------------------------------

export function useSseChat(sessionId: string): UseSseChatReturn {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [status, setStatus] = useState<ConnectionStatus>('idle');
  const [error, setError] = useState<ErrorPayload | null>(null);

  // Abort controller for cancelling in-flight requests
  const abortRef = useRef<AbortController | null>(null);
  // Timer for 3-second network error detection
  const errorTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const clearErrorTimer = useCallback(() => {
    if (errorTimerRef.current) {
      clearTimeout(errorTimerRef.current);
      errorTimerRef.current = null;
    }
  }, []);

  const sendMessage = useCallback(
    (text: string) => {
      if (!sessionId || !text.trim()) return;

      // Cancel any in-flight stream
      if (abortRef.current) {
        abortRef.current.abort();
      }

      // Reset error state on new send attempt (network recovery)
      setError(null);
      clearErrorTimer();

      // Add user message
      const userMsg: ChatMessage = {
        id: generateId(),
        role: 'user',
        content: text,
        timestamp: new Date(),
      };
      setMessages((prev) => [...prev, userMsg]);
      setStatus('connecting');

      // Create agent message placeholder
      const agentMsgId = generateId();
      const agentMsg: ChatMessage = {
        id: agentMsgId,
        role: 'agent',
        content: '',
        timestamp: new Date(),
      };
      setMessages((prev) => [...prev, agentMsg]);

      // Start SSE stream
      const controller = new AbortController();
      abortRef.current = controller;
      // 该请求是否仍是"当前请求"。打断式追问会 abort 旧请求，
      // 旧请求的 then/catch 回调不得再触碰共享状态（R8.3 竞态防护）。
      const isCurrent = () => abortRef.current === controller;

      // Set up 3-second network error timer (R14.4)
      errorTimerRef.current = setTimeout(() => {
        // If still connecting after 3 seconds, treat as network error
        setStatus((current) => {
          if (current === 'connecting') {
            const errorPayload: ErrorPayload = {
              error_code: 'LlmUnavailable',
              message: '网络连接异常，请检查网络后重试',
            };
            setError(errorPayload);
            return 'error';
          }
          return current;
        });
      }, 3000);

      postMessageFetch(sessionId, text, controller.signal)
        .then((response) => {
          if (!isCurrent()) return;
          clearErrorTimer();
          setStatus('streaming');

          if (!response.body) {
            throw new Error('Response body is null');
          }

          const reader = response.body.getReader();
          const decoder = new TextDecoder();
          let buffer = '';

          const processStream = (): Promise<void> => {
            return reader.read().then(({ done, value }) => {
              if (!isCurrent()) {
                // 已被新请求取代：释放 reader，不再更新任何状态。
                void reader.cancel().catch(() => {});
                return;
              }
              if (done) {
                // Flush decoder & process any remaining buffer
                buffer += decoder.decode();
                if (buffer.trim()) {
                  const lines = buffer.split('\n');
                  const frame = parseSseLines(lines);
                  if (frame) {
                    dispatchEvent(frame, agentMsgId);
                  }
                }
                setStatus((current) => (current === 'error' ? current : 'idle'));
                abortRef.current = null;
                return;
              }

              buffer += decoder.decode(value, { stream: true });

              // SSE events are separated by double newlines
              const parts = buffer.split('\n\n');
              // Keep the last part as it may be incomplete
              buffer = parts.pop() || '';

              for (const part of parts) {
                if (!part.trim()) continue;
                const lines = part.split('\n');
                const frame = parseSseLines(lines);
                if (frame) {
                  dispatchEvent(frame, agentMsgId);
                }
              }

              return processStream();
            });
          };

          return processStream();
        })
        .catch((err) => {
          // 被打断的旧请求：静默退出，不得清理新请求的定时器/状态。
          if (!isCurrent()) return;
          clearErrorTimer();
          abortRef.current = null;

          if (err instanceof ApiError) {
            setError(err.payload);
            setStatus('error');
          } else if (err.name === 'AbortError') {
            // Request was cancelled, not an error
            setStatus('idle');
          } else {
            // Network error — show within 3 seconds (R14.4)
            const errorPayload: ErrorPayload = {
              error_code: 'LlmUnavailable',
              message: '网络连接异常，请检查网络后重试',
            };
            setError(errorPayload);
            setStatus('error');
          }
        });
    },
    [sessionId, clearErrorTimer],
  );

  /**
   * Dispatch a parsed SSE frame to the correct state update.
   */
  function dispatchEvent(frame: SseFrame, agentMsgId: string): void {
    const { event, data } = frame;

    switch (event) {
      case 'text_delta': {
        // Backend wraps text in { text: "..." } envelope
        const parsed = JSON.parse(data) as { text: string };
        const text = parsed.text;
        setMessages((prev) =>
          prev.map((msg) =>
            msg.id === agentMsgId
              ? { ...msg, content: msg.content + text }
              : msg,
          ),
        );
        break;
      }

      case 'choice_prompt': {
        const prompt = JSON.parse(data) as ChoicePrompt;
        setMessages((prev) =>
          prev.map((msg) =>
            msg.id === agentMsgId ? { ...msg, choicePrompt: prompt } : msg,
          ),
        );
        break;
      }

      case 'skill_call': {
        // Show skill call as a text notification in the agent message
        const callInfo = JSON.parse(data) as { skill_id: string; args: unknown };
        setMessages((prev) =>
          prev.map((msg) =>
            msg.id === agentMsgId
              ? {
                  ...msg,
                  content: msg.content + `\n[正在执行: ${callInfo.skill_id}]`,
                }
              : msg,
          ),
        );
        break;
      }

      case 'skill_result': {
        const result = JSON.parse(data) as SkillResult;
        setMessages((prev) =>
          prev.map((msg) =>
            msg.id === agentMsgId ? { ...msg, skillResult: result } : msg,
          ),
        );
        break;
      }

      case 'interpretation': {
        // Backend wraps interpretation in { text: "..." } envelope
        const parsed = JSON.parse(data) as { text: string };
        const interpretation = parsed.text;
        setMessages((prev) =>
          prev.map((msg) =>
            msg.id === agentMsgId
              ? { ...msg, interpretation }
              : msg,
          ),
        );
        break;
      }

      case 'error': {
        const errorPayload = JSON.parse(data) as ErrorPayload;
        setError(errorPayload);
        setStatus('error');
        break;
      }

      case 'done': {
        setStatus('idle');
        break;
      }

      default:
        // Unknown event type — ignore
        break;
    }
  }

  return {
    messages,
    setMessages,
    sendMessage,
    status,
    error,
    isStreaming: status === 'streaming',
  };
}
