import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { DatasetUploader } from './DatasetUploader';

const uploadMock = vi.fn();

vi.mock('../hooks/useDatasetUpload', () => ({
  useDatasetUpload: () => ({
    upload: uploadMock,
    uploading: false,
    progress: 0,
    result: null,
    error: null,
    reset: vi.fn(),
  }),
}));

/** 构造一个指定字节数的 File，但不真的分配内存（size 是只读属性，需覆写）。 */
function fileOfSize(name: string, bytes: number): File {
  const file = new File(['x'], name, { type: 'text/csv' });
  Object.defineProperty(file, 'size', { value: bytes });
  return file;
}

function selectFile(file: File): void {
  const input = document.querySelector('input[type="file"]') as HTMLInputElement;
  fireEvent.change(input, { target: { files: [file] } });
}

describe('DatasetUploader — 上传前大小预检', () => {
  beforeEach(() => {
    uploadMock.mockClear();
  });

  it('rejects a file above the local ceiling without issuing a request', async () => {
    render(<DatasetUploader sessionId="s-1" />);

    // 60 MiB 原始文件 → base64 后约 80 MiB，会撞上服务端 70 MiB 的 bodyLimit。
    selectFile(fileOfSize('huge.csv', 60 * 1024 * 1024));

    expect(await screen.findByText('文件超出上传上限')).toBeInTheDocument();
    // 关键：不发网络请求，用户不用等整个 body 传完才收到英文 413。
    expect(uploadMock).not.toHaveBeenCalled();
  });

  it('states the actual size and the limit so the user knows how much to cut', async () => {
    render(<DatasetUploader sessionId="s-1" />);

    selectFile(fileOfSize('huge.csv', 60 * 1024 * 1024));

    const alert = await screen.findByText(/超出单次上传上限/);
    expect(alert.textContent).toContain('60.0 MB');
    expect(alert.textContent).toContain('50 MB');
  });

  it('still uploads a file within the ceiling', () => {
    render(<DatasetUploader sessionId="s-1" />);

    selectFile(fileOfSize('small.csv', 2 * 1024 * 1024));

    expect(uploadMock).toHaveBeenCalledTimes(1);
    expect(screen.queryByText('文件超出上传上限')).not.toBeInTheDocument();
  });
});
