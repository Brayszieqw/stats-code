/**
 * InfiniSynapsePanel — InfiniSynapse 泛数据分析集成面板（Vibe Coding 参赛）。
 *
 * 最小闭环：表单提交（分析指令）→ 进度反馈（2s 轮询）→ 结果展示（completion_result
 * 文本 + 产物文件列表）→ 用户下载（结果 zip）。另附数据源清单（Server API
 * /api/ai_database/list）。未配置密钥时先展示配置表单；密钥经本地后端探测
 * （/api/ai/ping）后落盘，不进浏览器存储。
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import { Alert, Button, Input, List, Space, Spin, Tag, Typography } from 'antd';
import { CloudDownloadOutlined, ExperimentOutlined, ReloadOutlined } from '@ant-design/icons';
import {
  getInfiniStatus,
  getInfiniTask,
  getInfiniTaskFiles,
  infiniDownloadUrl,
  infiniFileUrl,
  listInfiniDataSources,
  postInfiniAnalyze,
  postInfiniConfig,
  type InfiniDataSource,
  type InfiniTaskStatus,
} from '../api/infinisynapse';
import { ApiError } from '../api/client';

const { Paragraph, Text } = Typography;
const POLL_INTERVAL_MS = 2_000;

function errText(err: unknown): string {
  if (err instanceof ApiError) return err.payload.message;
  return err instanceof Error ? err.message : String(err);
}

export function InfiniSynapsePanel() {
  const [configured, setConfigured] = useState<boolean | null>(null);
  const [apiKey, setApiKey] = useState('');
  const [baseUrl, setBaseUrl] = useState('');
  const [savingKey, setSavingKey] = useState(false);

  const [prompt, setPrompt] = useState('');
  const [taskId, setTaskId] = useState<string | null>(null);
  const [task, setTask] = useState<InfiniTaskStatus | null>(null);
  const [files, setFiles] = useState<string[]>([]);
  const [submitting, setSubmitting] = useState(false);

  const [sources, setSources] = useState<InfiniDataSource[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const pollTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    getInfiniStatus()
      .then((s) => setConfigured(s.configured))
      .catch((err) => {
        setConfigured(false);
        setError(errText(err));
      });
    return () => {
      if (pollTimer.current) clearTimeout(pollTimer.current);
    };
  }, []);

  const handleSaveKey = useCallback(async () => {
    setSavingKey(true);
    setError(null);
    try {
      await postInfiniConfig(apiKey, baseUrl);
      setConfigured(true);
      setApiKey('');
    } catch (err) {
      setError(errText(err));
    } finally {
      setSavingKey(false);
    }
  }, [apiKey, baseUrl]);

  const poll = useCallback((id: string) => {
    getInfiniTask(id)
      .then((status) => {
        setTask(status);
        if (status.completed || status.failed) {
          void getInfiniTaskFiles(id)
            .then((ws) => setFiles(ws.files))
            .catch(() => setFiles([]));
          return;
        }
        pollTimer.current = setTimeout(() => poll(id), POLL_INTERVAL_MS);
      })
      .catch((err) => {
        setError(errText(err));
      });
  }, []);

  const handleAnalyze = useCallback(async () => {
    setSubmitting(true);
    setError(null);
    setTask(null);
    setFiles([]);
    try {
      const { task_id } = await postInfiniAnalyze(prompt);
      setTaskId(task_id);
      poll(task_id);
    } catch (err) {
      setError(errText(err));
    } finally {
      setSubmitting(false);
    }
  }, [prompt, poll]);

  const handleLoadSources = useCallback(async () => {
    setError(null);
    try {
      const { items } = await listInfiniDataSources();
      setSources(items);
    } catch (err) {
      setError(errText(err));
    }
  }, []);

  if (configured === null) {
    return <Spin style={{ display: 'block', margin: '24px auto' }} />;
  }

  if (!configured) {
    return (
      <Space direction="vertical" size={12} style={{ width: '100%' }}>
        <Paragraph style={{ marginBottom: 0 }}>
          配置 InfiniSynapse API Key（控制台「API Key Management」创建，sk- 开头）。
          密钥保存在本机后端，不会进入浏览器。
        </Paragraph>
        <Input.Password
          value={apiKey}
          onChange={(e) => setApiKey(e.target.value)}
          placeholder="sk-…"
          aria-label="InfiniSynapse API Key"
        />
        <Input
          value={baseUrl}
          onChange={(e) => setBaseUrl(e.target.value)}
          placeholder="Base URL（默认 https://app.infinisynapse.cn）"
          aria-label="InfiniSynapse Base URL"
        />
        {error ? <Alert type="error" showIcon message={error} /> : null}
        <Button type="primary" onClick={handleSaveKey} loading={savingKey} disabled={apiKey.trim().length === 0}>
          测试并保存
        </Button>
      </Space>
    );
  }

  return (
    <Space direction="vertical" size={16} style={{ width: '100%' }}>
      <div>
        <Paragraph style={{ marginBottom: 4, fontSize: 13 }}>
          分析指令（发送给 InfiniSynapse 泛数据分析引擎）
        </Paragraph>
        <Input.TextArea
          value={prompt}
          onChange={(e) => setPrompt(e.target.value)}
          rows={3}
          placeholder="例：分析示例数据中各组的发病率差异，输出统计摘要"
          aria-label="InfiniSynapse 分析指令"
        />
        <Button
          type="primary"
          icon={<ExperimentOutlined />}
          onClick={handleAnalyze}
          loading={submitting}
          disabled={prompt.trim().length === 0 || (task !== null && task.is_running)}
          style={{ marginTop: 8 }}
        >
          发起分析
        </Button>
      </div>

      {error ? <Alert type="error" showIcon message={error} /> : null}

      {taskId && task ? (
        <div>
          <Space size={8} wrap>
            <Text strong>任务</Text>
            <Text code>{taskId.slice(0, 8)}…</Text>
            {task.completed ? (
              <Tag color="green">已完成</Tag>
            ) : task.failed ? (
              <Tag color="red">失败</Tag>
            ) : (
              <Tag color="blue" icon={<Spin size="small" />}>
                运行中（{task.message_count} 条消息）
              </Tag>
            )}
          </Space>
          {!task.completed && task.latest_text ? (
            <Paragraph type="secondary" style={{ fontSize: 12, marginTop: 8, whiteSpace: 'pre-wrap' }}>
              {task.latest_text}
            </Paragraph>
          ) : null}
          {task.completed && task.result_text ? (
            <Paragraph style={{ marginTop: 8, whiteSpace: 'pre-wrap' }}>{task.result_text}</Paragraph>
          ) : null}
          {(task.completed || task.failed) && (
            <Space direction="vertical" size={8} style={{ width: '100%', marginTop: 8 }}>
              {files.length > 0 ? (
                <List
                  size="small"
                  header={<Text strong>产物文件（点击下载）</Text>}
                  dataSource={files}
                  renderItem={(f) => (
                    <List.Item style={{ fontSize: 12 }}>
                      <a href={infiniFileUrl(taskId, f)}>{f}</a>
                    </List.Item>
                  )}
                />
              ) : null}
              <Button icon={<CloudDownloadOutlined />} href={infiniDownloadUrl(taskId)}>
                下载结果 zip
              </Button>
            </Space>
          )}
        </div>
      ) : null}

      <div>
        <Button icon={<ReloadOutlined />} onClick={handleLoadSources} size="small">
          查看云端数据源
        </Button>
        {sources !== null ? (
          <List
            size="small"
            style={{ marginTop: 8 }}
            locale={{ emptyText: '暂无数据源（可在 InfiniSynapse 控制台添加）' }}
            dataSource={sources}
            renderItem={(s) => (
              <List.Item style={{ fontSize: 12 }}>
                <Space size={8}>
                  <Text strong>{s.name}</Text>
                  <Tag>{s.type}</Tag>
                  {s.enabled ? <Tag color="green">启用</Tag> : <Tag>停用</Tag>}
                </Space>
              </List.Item>
            )}
          />
        ) : null}
      </div>
    </Space>
  );
}
