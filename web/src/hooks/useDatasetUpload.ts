/**
 * useDatasetUpload — manages dataset file upload state.
 *
 * Uses postDataset from api/client.ts to upload files.
 * Exposes upload function, progress/loading state, result, and error.
 *
 * Validates: Requirements 3.1, 3.2, 3.3
 */

import { useCallback, useState } from 'react';
import type { DatasetSummary, ErrorPayload } from '../api/types';

// ---------------------------------------------------------------------------
// Accepted file extensions
// ---------------------------------------------------------------------------

const ACCEPTED_EXTENSIONS = new Set(['.csv', '.tsv']);

function getFileExtension(name: string): string {
  const idx = name.lastIndexOf('.');
  if (idx < 0) return '';
  return name.slice(idx).toLowerCase();
}

function isAcceptedFile(file: File): boolean {
  return ACCEPTED_EXTENSIONS.has(getFileExtension(file.name));
}

function readFileAsBase64(
  file: File,
  onProgress: (percent: number) => void,
): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();

    reader.addEventListener('progress', (event) => {
      if (event.lengthComputable) {
        onProgress(Math.round((event.loaded / event.total) * 40));
      }
    });
    reader.addEventListener('error', () => {
      reject(reader.error ?? new Error('failed to read file'));
    });
    reader.addEventListener('load', () => {
      if (typeof reader.result !== 'string') {
        reject(new Error('file reader returned a non-string result'));
        return;
      }
      const comma = reader.result.indexOf(',');
      resolve(comma >= 0 ? reader.result.slice(comma + 1) : reader.result);
    });

    reader.readAsDataURL(file);
  });
}

// ---------------------------------------------------------------------------
// Hook
// ---------------------------------------------------------------------------

export interface UseDatasetUploadReturn {
  upload: (file: File) => void;
  uploading: boolean;
  progress: number;
  result: DatasetSummary | null;
  error: ErrorPayload | null;
  reset: () => void;
}

export function useDatasetUpload(sessionId: string): UseDatasetUploadReturn {
  const [uploading, setUploading] = useState(false);
  const [progress, setProgress] = useState(0);
  const [result, setResult] = useState<DatasetSummary | null>(null);
  const [error, setError] = useState<ErrorPayload | null>(null);

  const reset = useCallback(() => {
    setUploading(false);
    setProgress(0);
    setResult(null);
    setError(null);
  }, []);

  const upload = useCallback(
    (file: File) => {
      // Validate extension client-side
      if (!isAcceptedFile(file)) {
        setError({
          error_code: 'SkillInvalidArgs',
          message: `不支持的文件格式 "${getFileExtension(file.name)}"，请上传 .csv/.tsv 文件`,
        });
        return;
      }

      setUploading(true);
      setProgress(0);
      setResult(null);
      setError(null);

      void readFileAsBase64(file, setProgress)
        .then((data) => {
          const xhr = new XMLHttpRequest();
          const body = JSON.stringify({ filename: file.name, data });

          xhr.upload.addEventListener('progress', (event) => {
            if (event.lengthComputable) {
              const pct = 40 + Math.round((event.loaded / event.total) * 55);
              setProgress(Math.min(95, pct));
            }
          });

          xhr.addEventListener('load', () => {
            setUploading(false);
            setProgress(100);

            if (xhr.status >= 200 && xhr.status < 300) {
              try {
                const summary = JSON.parse(xhr.responseText) as DatasetSummary;
                setResult(summary);
              } catch {
                setError({
                  error_code: 'SkillExecutionFailed',
                  message: '解析服务器响应失败',
                });
              }
            } else {
              try {
                const payload = JSON.parse(xhr.responseText) as ErrorPayload;
                setError(payload);
              } catch {
                setError({
                  error_code: 'SkillExecutionFailed',
                  message: `上传失败：HTTP ${xhr.status}`,
                });
              }
            }
          });

          xhr.addEventListener('error', () => {
            setUploading(false);
            setError({
              error_code: 'LlmUnavailable',
              message: '网络连接异常，请检查网络后重试',
            });
          });

          xhr.addEventListener('abort', () => {
            setUploading(false);
          });

          xhr.open('POST', `/api/sessions/${sessionId}/datasets`);
          xhr.setRequestHeader('Content-Type', 'application/json');
          xhr.send(body);
        })
        .catch((err: unknown) => {
          setUploading(false);
          setError({
            error_code: 'SkillExecutionFailed',
            message: err instanceof Error ? err.message : '读取文件失败',
          });
        });
    },
    [sessionId],
  );

  return { upload, uploading, progress, result, error, reset };
}

// Re-export for convenience
export { isAcceptedFile, ACCEPTED_EXTENSIONS };
