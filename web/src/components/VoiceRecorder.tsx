/**
 * VoiceRecorder — 语音录制 / 转写
 *
 * 优先级：
 *  1. 浏览器 Web Speech API（Chrome/Edge 本地真转写，不依赖云端 Whisper）
 *  2. MediaRecorder → POST /api/sessions/:sid/audio（OpenAI 兼容 Whisper）
 *
 * 低置信度（< 0.6）时展示可编辑文本，确认后发送。
 */

import { useState, useRef, useCallback, useEffect } from 'react';
import { Button, Input, Space, Typography, Alert } from 'antd';
import {
  AudioOutlined,
  PauseCircleOutlined,
  CloseCircleOutlined,
  CheckOutlined,
  EditOutlined,
  LoadingOutlined,
} from '@ant-design/icons';
import { postAudio, ApiError } from '../api/client';

const { Text } = Typography;

export interface VoiceRecorderProps {
  sessionId: string;
  onTranscript: (text: string) => void;
  disabled?: boolean;
}

type RecorderState = 'idle' | 'recording' | 'uploading' | 'confirming';

type BrowserSpeechRecognition = {
  lang: string;
  continuous: boolean;
  interimResults: boolean;
  maxAlternatives: number;
  start: () => void;
  stop: () => void;
  abort: () => void;
  onresult: ((ev: SpeechRecognitionEventLike) => void) | null;
  onerror: ((ev: { error?: string }) => void) | null;
  onend: (() => void) | null;
};

type SpeechRecognitionEventLike = {
  results: ArrayLike<{
    isFinal: boolean;
    0: { transcript: string; confidence: number };
  }>;
};

function getSpeechRecognitionCtor(): (new () => BrowserSpeechRecognition) | null {
  const w = window as unknown as {
    SpeechRecognition?: new () => BrowserSpeechRecognition;
    webkitSpeechRecognition?: new () => BrowserSpeechRecognition;
  };
  return w.SpeechRecognition ?? w.webkitSpeechRecognition ?? null;
}

export function VoiceRecorder({ sessionId, onTranscript, disabled }: VoiceRecorderProps) {
  const [state, setState] = useState<RecorderState>('idle');
  const [elapsedSecs, setElapsedSecs] = useState(0);
  const [confirmText, setConfirmText] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [modeLabel, setModeLabel] = useState<'browser' | 'upload' | null>(null);

  const mediaRecorderRef = useRef<MediaRecorder | null>(null);
  const speechRef = useRef<BrowserSpeechRecognition | null>(null);
  const chunksRef = useRef<Blob[]>([]);
  const streamRef = useRef<MediaStream | null>(null);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const startTimeRef = useRef<number>(0);
  const unmountedRef = useRef(false);
  const interimTextRef = useRef('');

  const stopTimer = useCallback(() => {
    if (timerRef.current !== null) {
      clearInterval(timerRef.current);
      timerRef.current = null;
    }
  }, []);

  const releaseStream = useCallback(() => {
    if (streamRef.current) {
      streamRef.current.getTracks().forEach((t) => t.stop());
      streamRef.current = null;
    }
  }, []);

  const resetState = useCallback(() => {
    stopTimer();
    releaseStream();
    mediaRecorderRef.current = null;
    if (speechRef.current) {
      try {
        speechRef.current.onresult = null;
        speechRef.current.onerror = null;
        speechRef.current.onend = null;
        speechRef.current.abort();
      } catch {
        /* ignore */
      }
      speechRef.current = null;
    }
    chunksRef.current = [];
    interimTextRef.current = '';
    setElapsedSecs(0);
    setModeLabel(null);
  }, [stopTimer, releaseStream]);

  const formatTime = (secs: number): string => {
    const m = Math.floor(secs / 60);
    const s = secs % 60;
    return `${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`;
  };

  const finishWithTranscript = useCallback(
    (text: string, confidence: number) => {
      const trimmed = text.trim();
      if (!trimmed) {
        setError('未识别到有效语音，请重试或改用键盘输入');
        setState('idle');
        setElapsedSecs(0);
        return;
      }
      if (confidence >= 0.6) {
        onTranscript(trimmed);
        setState('idle');
        setElapsedSecs(0);
      } else {
        setConfirmText(trimmed);
        setState('confirming');
      }
    },
    [onTranscript],
  );

  useEffect(() => {
    unmountedRef.current = false;
    return () => {
      unmountedRef.current = true;
      if (timerRef.current !== null) {
        clearInterval(timerRef.current);
        timerRef.current = null;
      }
      const recorder = mediaRecorderRef.current;
      if (recorder && recorder.state !== 'inactive') {
        recorder.onstop = null;
        try {
          recorder.stop();
        } catch {
          /* ignore */
        }
      }
      mediaRecorderRef.current = null;
      if (speechRef.current) {
        try {
          speechRef.current.onresult = null;
          speechRef.current.onerror = null;
          speechRef.current.onend = null;
          speechRef.current.abort();
        } catch {
          /* ignore */
        }
        speechRef.current = null;
      }
      if (streamRef.current) {
        streamRef.current.getTracks().forEach((t) => t.stop());
        streamRef.current = null;
      }
    };
  }, []);

  /** Path A: browser Web Speech API (preferred real STT). */
  const startBrowserSpeech = useCallback((): boolean => {
    const Ctor = getSpeechRecognitionCtor();
    if (!Ctor) return false;

    try {
      const recognition = new Ctor();
      recognition.lang = 'zh-CN';
      recognition.continuous = true;
      recognition.interimResults = true;
      recognition.maxAlternatives = 1;
      speechRef.current = recognition;
      interimTextRef.current = '';
      let finalText = '';

      recognition.onresult = (ev) => {
        let interim = '';
        for (let i = 0; i < ev.results.length; i += 1) {
          const row = ev.results[i];
          if (!row) continue;
          const alt = row[0];
          if (!alt) continue;
          if (row.isFinal) {
            finalText += alt.transcript;
          } else {
            interim += alt.transcript;
          }
        }
        interimTextRef.current = (finalText + interim).trim();
      };

      recognition.onerror = (ev) => {
        if (unmountedRef.current) return;
        const code = ev.error ?? 'unknown';
        if (code === 'aborted' || code === 'no-speech') {
          // Soft end — user cancel or silence.
          return;
        }
        // Fall through to MediaRecorder path on hard errors after stop is less useful mid-flight.
        setError(
          code === 'not-allowed'
            ? '麦克风权限被拒绝，请在浏览器设置中允许访问麦克风'
            : `浏览器语音识别失败（${code}）。可重试或改用云端转写。`,
        );
        stopTimer();
        setState('idle');
        setElapsedSecs(0);
        speechRef.current = null;
      };

      recognition.onend = null;

      recognition.start();
      startTimeRef.current = Date.now();
      setModeLabel('browser');
      setState('recording');
      setElapsedSecs(0);
      timerRef.current = setInterval(() => {
        setElapsedSecs(Math.floor((Date.now() - startTimeRef.current) / 1000));
      }, 1000);
      return true;
    } catch {
      return false;
    }
  }, [stopTimer]);

  /** Path B: MediaRecorder + server Whisper. */
  const startMediaUpload = useCallback(async () => {
    const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
    if (unmountedRef.current) {
      stream.getTracks().forEach((t) => t.stop());
      return;
    }
    streamRef.current = stream;
    const mimeType = MediaRecorder.isTypeSupported('audio/webm') ? 'audio/webm' : 'audio/wav';
    const recorder = new MediaRecorder(stream, { mimeType });
    mediaRecorderRef.current = recorder;
    chunksRef.current = [];
    recorder.ondataavailable = (e) => {
      if (e.data.size > 0) chunksRef.current.push(e.data);
    };
    recorder.start(250);
    startTimeRef.current = Date.now();
    setModeLabel('upload');
    setState('recording');
    setElapsedSecs(0);
    timerRef.current = setInterval(() => {
      setElapsedSecs(Math.floor((Date.now() - startTimeRef.current) / 1000));
    }, 1000);
  }, []);

  const handleStart = useCallback(async () => {
    setError(null);
    // Prefer browser speech — real STT without Whisper dependency.
    if (startBrowserSpeech()) return;
    try {
      await startMediaUpload();
    } catch (err: unknown) {
      const message =
        err instanceof DOMException && err.name === 'NotAllowedError'
          ? '麦克风权限被拒绝，请在浏览器设置中允许访问麦克风'
          : '无法启动录音，请检查麦克风设备';
      setError(message);
    }
  }, [startBrowserSpeech, startMediaUpload]);

  const handleStop = useCallback(() => {
    stopTimer();

    // Browser speech path
    if (speechRef.current) {
      const recognition = speechRef.current;
      const textSnapshot = interimTextRef.current;
      recognition.onend = () => {
        if (unmountedRef.current) return;
        speechRef.current = null;
        // Browser confidences are often 0; treat non-empty as medium-high.
        finishWithTranscript(textSnapshot || interimTextRef.current, 0.75);
      };
      try {
        recognition.stop();
      } catch {
        finishWithTranscript(textSnapshot, 0.75);
        speechRef.current = null;
      }
      return;
    }

    // MediaRecorder path
    const recorder = mediaRecorderRef.current;
    if (!recorder || recorder.state === 'inactive') return;
    const durationSecs = Math.round((Date.now() - startTimeRef.current) / 1000);

    recorder.onstop = async () => {
      releaseStream();
      if (unmountedRef.current) return;
      setState('uploading');
      const blob = new Blob(chunksRef.current, { type: recorder.mimeType });
      chunksRef.current = [];
      try {
        const result = await postAudio(sessionId, blob, durationSecs);
        if (unmountedRef.current) return;
        finishWithTranscript(result.text, result.confidence);
      } catch (err: unknown) {
        if (unmountedRef.current) return;
        const message =
          err instanceof ApiError ? err.payload.message : '音频上传失败，请重试';
        setError(message);
        setState('idle');
        setElapsedSecs(0);
      }
    };
    recorder.stop();
  }, [sessionId, finishWithTranscript, stopTimer, releaseStream]);

  const handleCancel = useCallback(() => {
    if (speechRef.current) {
      try {
        speechRef.current.onresult = null;
        speechRef.current.onerror = null;
        speechRef.current.onend = null;
        speechRef.current.abort();
      } catch {
        /* ignore */
      }
      speechRef.current = null;
    }
    const recorder = mediaRecorderRef.current;
    if (recorder && recorder.state !== 'inactive') {
      recorder.onstop = null;
      recorder.stop();
    }
    resetState();
    setState('idle');
  }, [resetState]);

  const handleConfirm = useCallback(() => {
    if (confirmText.trim()) onTranscript(confirmText.trim());
    setConfirmText('');
    setState('idle');
    setElapsedSecs(0);
  }, [confirmText, onTranscript]);

  const handleCancelConfirm = useCallback(() => {
    setConfirmText('');
    setState('idle');
    setElapsedSecs(0);
  }, []);

  if (error) {
    return (
      <Alert
        type="error"
        message={error}
        showIcon
        closable
        onClose={() => setError(null)}
        style={{ marginBottom: 8 }}
      />
    );
  }

  if (state === 'confirming') {
    return (
      <Space.Compact style={{ width: '100%' }}>
        <Input
          value={confirmText}
          onChange={(e) => setConfirmText(e.target.value)}
          placeholder="转写结果（置信度较低，请确认或编辑）"
          prefix={<EditOutlined style={{ color: '#faad14' }} />}
          onPressEnter={handleConfirm}
          aria-label="编辑转写文本"
        />
        <Button type="primary" icon={<CheckOutlined />} onClick={handleConfirm} disabled={!confirmText.trim()} aria-label="确认发送">
          确认
        </Button>
        <Button onClick={handleCancelConfirm} icon={<CloseCircleOutlined />} aria-label="取消">
          取消
        </Button>
      </Space.Compact>
    );
  }

  if (state === 'uploading') {
    return (
      <Button disabled icon={<LoadingOutlined />}>
        云端转写中...
      </Button>
    );
  }

  if (state === 'recording') {
    return (
      <Space>
        <Text type="danger" style={{ fontFamily: 'monospace', minWidth: 48 }}>
          ● {formatTime(elapsedSecs)}
        </Text>
        {modeLabel === 'browser' ? (
          <Text type="secondary" style={{ fontSize: 12 }}>
            浏览器识别
          </Text>
        ) : null}
        <Button type="primary" danger icon={<PauseCircleOutlined />} onClick={handleStop} aria-label="停止录音">
          停止
        </Button>
        <Button icon={<CloseCircleOutlined />} onClick={handleCancel} aria-label="取消录音">
          取消
        </Button>
      </Space>
    );
  }

  return (
    <Button
      shape="circle"
      icon={<AudioOutlined />}
      onClick={() => void handleStart()}
      disabled={disabled}
      title={getSpeechRecognitionCtor() ? '语音输入（浏览器识别）' : '语音输入（云端 Whisper）'}
      aria-label="开始录音"
    />
  );
}

export default VoiceRecorder;
