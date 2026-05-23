/**
 * VoiceRecorder — 语音录制组件
 *
 * 使用 MediaRecorder API 录制音频（audio/webm 格式）。
 * 支持开始/停止/取消操作。
 * 低置信度（< 0.6）时展示转写文本，要求用户确认或编辑后发送。
 *
 * Validates: Requirements 2.1, 2.2, 2.4
 */

import { useState, useRef, useCallback } from 'react';
import { Button, Input, Space, Typography, Alert } from 'antd';
import {
  AudioOutlined,
  PauseCircleOutlined,
  CloseCircleOutlined,
  CheckOutlined,
  EditOutlined,
  LoadingOutlined,
} from '@ant-design/icons';
import { postAudio } from '../api/client';
import { ApiError } from '../api/client';

const { Text } = Typography;

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface VoiceRecorderProps {
  sessionId: string;
  onTranscript: (text: string) => void;
  disabled?: boolean;
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

type RecorderState = 'idle' | 'recording' | 'uploading' | 'confirming';

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function VoiceRecorder({ sessionId, onTranscript, disabled }: VoiceRecorderProps) {
  const [state, setState] = useState<RecorderState>('idle');
  const [elapsedSecs, setElapsedSecs] = useState(0);
  const [confirmText, setConfirmText] = useState('');
  const [error, setError] = useState<string | null>(null);

  const mediaRecorderRef = useRef<MediaRecorder | null>(null);
  const chunksRef = useRef<Blob[]>([]);
  const streamRef = useRef<MediaStream | null>(null);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const startTimeRef = useRef<number>(0);

  // -------------------------------------------------------------------------
  // Helpers
  // -------------------------------------------------------------------------

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
    chunksRef.current = [];
    setElapsedSecs(0);
    setError(null);
  }, [stopTimer, releaseStream]);

  const formatTime = (secs: number): string => {
    const m = Math.floor(secs / 60);
    const s = secs % 60;
    return `${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`;
  };

  // -------------------------------------------------------------------------
  // Start recording
  // -------------------------------------------------------------------------

  const handleStart = useCallback(async () => {
    setError(null);
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      streamRef.current = stream;

      const mimeType = MediaRecorder.isTypeSupported('audio/webm')
        ? 'audio/webm'
        : 'audio/wav';

      const recorder = new MediaRecorder(stream, { mimeType });
      mediaRecorderRef.current = recorder;
      chunksRef.current = [];

      recorder.ondataavailable = (e) => {
        if (e.data.size > 0) {
          chunksRef.current.push(e.data);
        }
      };

      recorder.start(250); // collect data every 250ms
      startTimeRef.current = Date.now();
      setState('recording');
      setElapsedSecs(0);

      // Start elapsed timer
      timerRef.current = setInterval(() => {
        const elapsed = Math.floor((Date.now() - startTimeRef.current) / 1000);
        setElapsedSecs(elapsed);
      }, 1000);
    } catch (err: unknown) {
      const message =
        err instanceof DOMException && err.name === 'NotAllowedError'
          ? '麦克风权限被拒绝，请在浏览器设置中允许访问麦克风'
          : '无法启动录音，请检查麦克风设备';
      setError(message);
    }
  }, []);

  // -------------------------------------------------------------------------
  // Stop recording and upload
  // -------------------------------------------------------------------------

  const handleStop = useCallback(() => {
    const recorder = mediaRecorderRef.current;
    if (!recorder || recorder.state === 'inactive') return;

    // Calculate duration before stopping
    const durationSecs = Math.round((Date.now() - startTimeRef.current) / 1000);
    stopTimer();

    recorder.onstop = async () => {
      releaseStream();
      setState('uploading');

      const blob = new Blob(chunksRef.current, { type: recorder.mimeType });
      chunksRef.current = [];

      try {
        const result = await postAudio(sessionId, blob, durationSecs);

        if (result.confidence >= 0.6) {
          // High confidence — auto-send
          onTranscript(result.text);
          setState('idle');
          setElapsedSecs(0);
        } else {
          // Low confidence — require user confirmation
          setConfirmText(result.text);
          setState('confirming');
        }
      } catch (err: unknown) {
        const message =
          err instanceof ApiError
            ? err.payload.message
            : '音频上传失败，请重试';
        setError(message);
        setState('idle');
        setElapsedSecs(0);
      }
    };

    recorder.stop();
  }, [sessionId, onTranscript, stopTimer, releaseStream]);

  // -------------------------------------------------------------------------
  // Cancel recording
  // -------------------------------------------------------------------------

  const handleCancel = useCallback(() => {
    const recorder = mediaRecorderRef.current;
    if (recorder && recorder.state !== 'inactive') {
      recorder.onstop = null;
      recorder.stop();
    }
    resetState();
    setState('idle');
  }, [resetState]);

  // -------------------------------------------------------------------------
  // Confirm transcription (low confidence path)
  // -------------------------------------------------------------------------

  const handleConfirm = useCallback(() => {
    if (confirmText.trim()) {
      onTranscript(confirmText.trim());
    }
    setConfirmText('');
    setState('idle');
    setElapsedSecs(0);
  }, [confirmText, onTranscript]);

  const handleCancelConfirm = useCallback(() => {
    setConfirmText('');
    setState('idle');
    setElapsedSecs(0);
  }, []);

  // -------------------------------------------------------------------------
  // Render
  // -------------------------------------------------------------------------

  // Error display
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

  // Confirming state — show editable transcription text
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
        <Button
          type="primary"
          icon={<CheckOutlined />}
          onClick={handleConfirm}
          disabled={!confirmText.trim()}
          aria-label="确认发送"
        >
          确认
        </Button>
        <Button
          onClick={handleCancelConfirm}
          icon={<CloseCircleOutlined />}
          aria-label="取消"
        >
          取消
        </Button>
      </Space.Compact>
    );
  }

  // Uploading state
  if (state === 'uploading') {
    return (
      <Button disabled icon={<LoadingOutlined />}>
        转写中...
      </Button>
    );
  }

  // Recording state
  if (state === 'recording') {
    return (
      <Space>
        <Text type="danger" style={{ fontFamily: 'monospace', minWidth: 48 }}>
          ● {formatTime(elapsedSecs)}
        </Text>
        <Button
          type="primary"
          danger
          icon={<PauseCircleOutlined />}
          onClick={handleStop}
          aria-label="停止录音"
        >
          停止
        </Button>
        <Button
          icon={<CloseCircleOutlined />}
          onClick={handleCancel}
          aria-label="取消录音"
        >
          取消
        </Button>
      </Space>
    );
  }

  // Idle state — show microphone button
  return (
    <Button
      shape="circle"
      icon={<AudioOutlined />}
      onClick={handleStart}
      disabled={disabled}
      title="语音输入"
      aria-label="开始录音"
    />
  );
}

export default VoiceRecorder;
