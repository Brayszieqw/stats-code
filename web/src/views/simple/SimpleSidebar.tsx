/**
 * SimpleSidebar — 简易模式左侧导航。
 *
 * 布局对齐参考图 2（顶部入口列表 + 分组历史 + 底部信息卡），视觉与文案为
 * Stats Code 原创，不照搬 Codex 元素。搜索/插件/自动化/模板为占位
 * message.info；历史项点击调用 onSelectSession。历史加载错误显示占位，不阻塞。
 *
 * Validates: Requirements 2.1, 2.2, 9.6, 9.7, 11.1
 */

import { useMemo, useRef, useState } from 'react';
import { Alert, Button, Drawer, Empty, Input, Space, Tag, Typography } from 'antd';
import {
  EditOutlined,
  SearchOutlined,
  ApiOutlined,
  ThunderboltOutlined,
  AppstoreOutlined,
  FolderOpenOutlined,
  HistoryOutlined,
  DatabaseOutlined,
  SettingOutlined,
  ApartmentOutlined,
  ReloadOutlined,
  DeleteOutlined,
  InfoCircleOutlined,
} from '@ant-design/icons';
import { DecisionAssistantToggle } from '../../components/DecisionAssistantToggle';
import { CapabilityStatement } from '../../components/CapabilityStatement';
import type { UseSessionListReturn } from '../../hooks/useSessionList';

const { Text } = Typography;

const PRIMARY = '#38618c';

export interface SimpleSidebarProps {
  sessionList: UseSessionListReturn;
  onNewSession: () => void;
  onSelectSession: (sessionId: string) => void;
  activeSessionId?: string;
  sessionId?: string;
  isArchived?: boolean;
  decisionAssistant?: boolean;
  onDecisionAssistantChange?: (enabled: boolean) => void;
  onOpenDatasetUpload?: () => void;
  onOpenSettings?: () => void;
  onOpenProMode?: () => void;
  onUseTemplate?: (prompt: string) => void;
  onDeleteSession?: (sessionId: string) => void | Promise<void>;
  /** Delete all empty shells (0 messages & 0 datasets), keep active session. */
  onPurgeEmptySessions?: () => void | Promise<void>;
}

type PanelKey = 'search' | 'plugins' | 'automation' | 'templates';

const ANALYSIS_TEMPLATES = [
  { title: '线性回归', prompt: '帮我分析结局变量与预测变量之间的线性关系，并给出回归模型。' },
  { title: 'Logistic 回归', prompt: '帮我分析二分类结局的风险因素，并解释优势比和显著性。' },
  { title: '生存分析', prompt: '帮我比较不同分组的生存曲线，并评估 Cox 回归模型。' },
  { title: '数据质量检查', prompt: '请先检查数据集的缺失值、变量类型和异常值，再建议合适的统计分析。' },
];

const NAV_ITEMS = [
  { key: 'new', label: '新对话', icon: <EditOutlined /> },
  { key: 'search', label: '搜索', icon: <SearchOutlined /> },
  { key: 'plugins', label: '插件', icon: <ApiOutlined /> },
  { key: 'automation', label: '自动化', icon: <ThunderboltOutlined /> },
  { key: 'templates', label: '分析模板', icon: <AppstoreOutlined /> },
];

function NavRow({
  icon,
  label,
  onClick,
  accent = false,
}: {
  icon: React.ReactNode;
  label: string;
  onClick: () => void;
  accent?: boolean;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      onClick={onClick}
      className="side-nav-row"
      style={{
        display: 'flex',
        alignItems: 'center',
        gap: 10,
        width: '100%',
        padding: '8px 10px',
        border: 'none',
        background: 'transparent',
        borderRadius: 8,
        cursor: 'pointer',
        color: accent ? PRIMARY : '#3a4654',
        fontSize: 14,
        fontWeight: accent ? 600 : 400,
        textAlign: 'left',
      }}
    >
      <span style={{ fontSize: 15, color: accent ? PRIMARY : '#6a7a8c' }}>{icon}</span>
      {label}
    </button>
  );
}

export function SimpleSidebar({
  sessionList,
  onNewSession,
  onSelectSession,
  activeSessionId,
  sessionId,
  isArchived = false,
  decisionAssistant,
  onDecisionAssistantChange,
  onOpenDatasetUpload,
  onOpenSettings,
  onOpenProMode,
  onUseTemplate,
  onDeleteSession,
  onPurgeEmptySessions,
}: SimpleSidebarProps) {
  const { sessions, error } = sessionList;
  const [activePanel, setActivePanel] = useState<PanelKey | null>(null);
  const [query, setQuery] = useState('');
  const deletingSessionIdsRef = useRef(new Set<string>());
  const [deletingSessionIds, setDeletingSessionIds] = useState<Set<string>>(() => new Set());
  const [deleteError, setDeleteError] = useState<string | null>(null);
  const [purging, setPurging] = useState(false);
  const [capabilitiesOpen, setCapabilitiesOpen] = useState(false);

  const filteredSessions = useMemo(() => {
    const needle = query.trim().toLowerCase();
    // Hide empty shells (0 messages & 0 datasets) unless they are the active session
    // or the user is searching — cuts the "新对话 / 0" flood in the sidebar.
    const base = sessions.filter(
      (session) =>
        session.id === activeSessionId ||
        session.message_count > 0 ||
        session.dataset_count > 0 ||
        needle.length > 0,
    );
    if (!needle) return base;
    return base.filter((session) => session.title.toLowerCase().includes(needle));
  }, [query, sessions, activeSessionId]);

  const closePanel = () => setActivePanel(null);

  const handleSelectSession = (sid: string) => {
    onSelectSession(sid);
    closePanel();
  };

  const handleDeleteSession = async (sid: string): Promise<void> => {
    if (!onDeleteSession || deletingSessionIdsRef.current.has(sid)) {
      return;
    }
    deletingSessionIdsRef.current.add(sid);
    setDeletingSessionIds((prev) => new Set(prev).add(sid));
    setDeleteError(null);
    try {
      await onDeleteSession(sid);
    } catch (err) {
      setDeleteError(
        err instanceof Error && err.message.trim()
          ? err.message
          : '删除会话失败，请稍后重试',
      );
      await sessionList.refresh().catch(() => undefined);
    } finally {
      deletingSessionIdsRef.current.delete(sid);
      setDeletingSessionIds((prev) => {
        const next = new Set(prev);
        next.delete(sid);
        return next;
      });
    }
  };

  return (
    <>
      <div className="stats-sidebar__inner">
      {/* 品牌 */}
      <div className="stats-brand">
        <div className="stats-brand__wordmark">
          <span className="stats-brand__name">Stats Code</span>
          <span className="stats-brand__caption">Evidence Workspace</span>
        </div>
        <span className="stats-brand__edition">LOCAL</span>
      </div>

      {/* 顶部入口 */}
      <div className="stats-nav" style={{ padding: '4px 8px' }}>
        {NAV_ITEMS.map((it) => (
          <NavRow
            key={it.key}
            icon={it.icon}
            label={it.label}
            accent={it.key === 'new'}
            onClick={it.key === 'new' ? onNewSession : () => setActivePanel(it.key as PanelKey)}
          />
        ))}
      </div>

      {/* 历史区 */}
      <div className="stats-sidebar__history" style={{ padding: '8px' }} aria-label="历史会话列表">
        <div className="stats-sidebar__section-label" style={{ display: 'flex', alignItems: 'center', gap: 6, padding: '6px 10px', color: '#9aa7b4', fontSize: 12 }}>
          <FolderOpenOutlined /> 默认项目
        </div>
        <div className="stats-sidebar__section-label" style={{ display: 'flex', alignItems: 'center', gap: 6, padding: '6px 10px', color: '#9aa7b4', fontSize: 12 }}>
          <HistoryOutlined /> 历史会话
        </div>

        {deleteError && activePanel !== 'search' ? (
          <Alert
            type="error"
            showIcon
            closable
            role="alert"
            message={deleteError}
            onClose={() => setDeleteError(null)}
            style={{ margin: '4px 0 8px' }}
          />
        ) : null}

        {error ? (
          <div style={{ padding: '8px 10px' }} role="note">
            <Text type="secondary" style={{ fontSize: 12 }}>
              历史会话加载失败，不影响当前对话
            </Text>
          </div>
        ) : sessions.length === 0 ? (
          <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="暂无历史" style={{ marginTop: 24 }} />
        ) : (
          sessions.map((s) => {
            const active = s.id === activeSessionId;
            const deleting = deletingSessionIds.has(s.id);
            return (
              <div
                key={s.id}
                className="history-session-row"
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: 4,
                  width: '100%',
                  marginBottom: 2,
                }}
              >
                <button
                  type="button"
                  aria-label={`历史会话: ${s.title}`}
                  onClick={() => onSelectSession(s.id)}
                  className={`history-item${active ? ' active' : ''}`}
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'space-between',
                    flex: 1,
                    minWidth: 0,
                    padding: '8px 8px 8px 12px',
                    border: 'none',
                    background: active ? 'rgba(36,79,115,0.10)' : 'transparent',
                    borderRadius: 6,
                    cursor: 'pointer',
                    color: '#3a4654',
                    fontSize: 13,
                    textAlign: 'left',
                  }}
                >
                  <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', maxWidth: 150 }}>
                    {s.title}
                  </span>
                  <Text type="secondary" style={{ fontSize: 11 }}>
                    {s.message_count}
                  </Text>
                </button>
                {onDeleteSession ? (
                  <Button
                    className="history-session-delete"
                    type="text"
                    danger
                    size="small"
                    icon={<DeleteOutlined />}
                    aria-label={`删除会话: ${s.title}`}
                    disabled={deleting}
                    loading={deleting}
                    onClick={() => void handleDeleteSession(s.id)}
                    style={{ flexShrink: 0 }}
                  />
                ) : null}
              </div>
            );
          })
        )}
      </div>

      <div className="stats-sidebar__footer">
        <Button
          type="text"
          block
          icon={<InfoCircleOutlined />}
          aria-label="关于与能力边界"
          onClick={() => setCapabilitiesOpen(true)}
          className="capability-entry"
        >
          关于与能力边界
        </Button>
        <div className="privacy-status" role="status">
          <span className="privacy-status__dot" aria-hidden />
          数据仅在本机处理
        </div>
        {/* Always-visible 辅助决策 — not buried only inside 自动化 drawer */}
        {typeof decisionAssistant === 'boolean' && onDecisionAssistantChange ? (
          <div style={{ padding: '6px 12px 14px' }}>
            <DecisionAssistantToggle
              value={decisionAssistant}
              onChange={onDecisionAssistantChange}
              sessionId={sessionId}
              disabled={isArchived}
            />
          </div>
        ) : null}
      </div>
      </div>

      {/* mask=false so the left nav stays clickable when switching panels */}
      <Drawer
        title="关于与能力边界"
        placement="left"
        width={440}
        open={capabilitiesOpen}
        onClose={() => setCapabilitiesOpen(false)}
      >
        <CapabilityStatement />
      </Drawer>

      <Drawer
        title="搜索历史会话"
        placement="left"
        width={360}
        mask={false}
        open={activePanel === 'search'}
        onClose={closePanel}
        styles={{ wrapper: { boxShadow: '4px 0 16px rgba(0,0,0,0.08)' } }}
      >
        <Space direction="vertical" size={12} style={{ width: '100%' }}>
          <Input.Search
            allowClear
            placeholder="输入会话标题关键词"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            aria-label="搜索历史会话"
          />
          <Button icon={<ReloadOutlined />} onClick={() => void sessionList.refresh()} block>
            刷新历史
          </Button>
          {deleteError ? (
            <Alert
              type="error"
              showIcon
              closable
              role="alert"
              message={deleteError}
              onClose={() => setDeleteError(null)}
            />
          ) : null}
          {filteredSessions.length === 0 ? (
            <Empty description={sessions.length === 0 ? '暂无历史会话' : '没有匹配的会话'} />
          ) : (
            <Space direction="vertical" size={8} style={{ width: '100%' }}>
              {filteredSessions.map((session) => {
                const deleting = deletingSessionIds.has(session.id);
                return (
                  <div
                    key={session.id}
                    style={{
                      display: 'flex',
                      alignItems: 'stretch',
                      gap: 8,
                      width: '100%',
                    }}
                  >
                    <button
                      type="button"
                      aria-label={`搜索结果: ${session.title}`}
                      onClick={() => handleSelectSession(session.id)}
                      style={{
                        flex: 1,
                        minWidth: 0,
                        border: '1px solid #e3e1d8',
                        background: session.id === activeSessionId ? 'rgba(56,97,140,0.10)' : '#fff',
                        borderRadius: 8,
                        padding: '10px 12px',
                        textAlign: 'left',
                        cursor: 'pointer',
                      }}
                    >
                      <Text strong>{session.title}</Text>
                      <div style={{ marginTop: 4 }}>
                        <Text type="secondary" style={{ fontSize: 12 }}>
                          {session.message_count} 条消息 · {session.dataset_count} 个数据集
                        </Text>
                      </div>
                    </button>
                    {onDeleteSession ? (
                      <Button
                        danger
                        icon={<DeleteOutlined />}
                        aria-label={`删除会话: ${session.title}`}
                        disabled={deleting}
                        loading={deleting}
                        onClick={() => void handleDeleteSession(session.id)}
                      />
                    ) : null}
                  </div>
                );
              })}
            </Space>
          )}
        </Space>
      </Drawer>

      <Drawer
        title="插件"
        placement="left"
        width={360}
        mask={false}
        open={activePanel === 'plugins'}
        onClose={closePanel}
        styles={{ wrapper: { boxShadow: '4px 0 16px rgba(0,0,0,0.08)' } }}
      >
        <Space direction="vertical" size={10} style={{ width: '100%' }}>
          <Button
            icon={<DatabaseOutlined />}
            onClick={() => {
              closePanel();
              onOpenDatasetUpload?.();
            }}
            disabled={!onOpenDatasetUpload || isArchived}
            block
          >
            数据集上传与选择
          </Button>
          <Button
            icon={<SettingOutlined />}
            onClick={() => {
              closePanel();
              onOpenSettings?.();
            }}
            disabled={!onOpenSettings}
            block
          >
            LLM / API 设置
          </Button>
          <Button
            icon={<ApartmentOutlined />}
            onClick={() => {
              closePanel();
              onOpenProMode?.();
            }}
            disabled={!onOpenProMode}
            block
          >
            {onOpenProMode ? '打开专业分析工作台' : '专业分析工作台（当前）'}
          </Button>
          <Tag color="blue" style={{ width: 'fit-content' }}>
            当前内置统计引擎、等效代码、报告导出和语音转写
          </Tag>
        </Space>
      </Drawer>

      <Drawer
        title="自动化"
        placement="left"
        width={360}
        mask={false}
        open={activePanel === 'automation'}
        onClose={closePanel}
        styles={{ wrapper: { boxShadow: '4px 0 16px rgba(0,0,0,0.08)' } }}
      >
        <Space direction="vertical" size={14} style={{ width: '100%' }}>
          {typeof decisionAssistant === 'boolean' && onDecisionAssistantChange ? (
            <DecisionAssistantToggle
              value={decisionAssistant}
              onChange={onDecisionAssistantChange}
              sessionId={sessionId}
              disabled={isArchived}
            />
          ) : (
            <Text type="secondary">当前会话暂未加载自动化设置。</Text>
          )}
          <Button icon={<ReloadOutlined />} onClick={() => void sessionList.refresh()} block>
            刷新会话列表
          </Button>
          <Button
            danger
            icon={<DeleteOutlined />}
            disabled={!onPurgeEmptySessions || purging}
            loading={purging}
            block
            onClick={() => {
              if (!onPurgeEmptySessions || purging) return;
              setPurging(true);
              void Promise.resolve(onPurgeEmptySessions()).finally(() => setPurging(false));
            }}
            aria-label="清理空会话"
          >
            清理空会话
          </Button>
          <Text type="secondary" style={{ fontSize: 12 }}>
            辅助决策开启后，Stats 会在回复中主动建议下一步分析、风险检查和结果解释。
            「清理空会话」会删除无消息且无数据集的历史壳（保留当前会话）。
          </Text>
        </Space>
      </Drawer>

      <Drawer
        title="分析模板"
        placement="left"
        width={380}
        mask={false}
        open={activePanel === 'templates'}
        onClose={closePanel}
        styles={{ wrapper: { boxShadow: '4px 0 16px rgba(0,0,0,0.08)' } }}
      >
        <Space direction="vertical" size={10} style={{ width: '100%' }}>
          {ANALYSIS_TEMPLATES.map((template) => (
            <button
              key={template.title}
              type="button"
              disabled={!onUseTemplate || isArchived}
              onClick={() => {
                onUseTemplate?.(template.prompt);
                closePanel();
              }}
              style={{
                width: '100%',
                border: '1px solid #e3e1d8',
                background: '#fff',
                borderRadius: 8,
                padding: '12px',
                textAlign: 'left',
                cursor: !onUseTemplate || isArchived ? 'not-allowed' : 'pointer',
              }}
            >
              <Text strong>{template.title}</Text>
              <div style={{ marginTop: 4 }}>
                <Text type="secondary" style={{ fontSize: 12 }}>
                  {template.prompt}
                </Text>
              </div>
            </button>
          ))}
        </Space>
      </Drawer>
    </>
  );
}

export default SimpleSidebar;
