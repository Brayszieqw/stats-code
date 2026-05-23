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

const ACCEPTED_EXTENSIONS = new Set(['.csv', '.tsv', '.xlsx', '.xls']);

function getFileExtension(name: string): string {
  const idx = name.lastIndexOf('.');
  if (idx < 0) return '';
  return name.slice(idx).toLowerCase();
}

function isAcceptedFile(file: File): boolean {
  return ACCEPTED_EXTENSIONS.has(getFileExtension(file.name));
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
          error_code: 'DatasetTooLarge',
          message: `不支持的文件格式 "${getFileExtension(file.name)}"，请上传 .csv/.tsv/.xlsx/.xls 文件`,
        });
        return;
      }

      setUploading(true);
      setProgress(0);
      setResult(null);
      setError(null);

      // Use XMLHttpRequest for upload progress tracking
      const xhr = new XMLHttpRequest();
      const formData = new FormData();
      formData.append('file', file);

      xhr.upload.addEventListener('progress', (event) => {
        if (event.lengthComputable) {
          const pct = Math.round((event.loaded / event.total) * 100);
          setProgress(pct);
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
      xhr.send(formData);
    },
    [sessionId],
  );

  return { upload, uploading, progress, result, error, reset };
}

// Re-export for convenience
export { isAcceptedFile, ACCEPTED_EXTENSIONS };
