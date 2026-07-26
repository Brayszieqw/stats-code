/**
 * StatsTable — 统计表格外壳（只管框，不管内容）。
 *
 * 长表（连续变量多）在会话流里有三个可用性问题：横向滚动条挂在容器底边，
 * 表格越长越摸不到；表头与首列滚出视口后行列失去标签；会话气泡被限制在
 * 820px（index.css 的 assistant-panel__messages 内边距），宽表必然溢出。
 *
 * 本组件提供 bounded viewport（让滚动条随时可达）、密度切换（隐藏每格的
 * 「有效 n / 缺失」副行，长表可省约 40% 高度）、变量筛选与全屏放大。
 * 表格本身仍由 ThreeLineTable 渲染并作为 children 传入 —— 外壳不解析
 * payload，也不改表格 DOM 结构，因此会话气泡与检查器侧栏两处可共用。
 *
 * sticky 与三线的实现在 index.css 的 .three-line-table 规则集内。
 */

import { useMemo, useState, type ReactNode } from 'react';
import { Button, Input, Modal, Segmented, Tooltip } from 'antd';
import {
  ColumnHeightOutlined,
  CompressOutlined,
  FullscreenOutlined,
  SearchOutlined,
} from '@ant-design/icons';

export type TableDensity = 'comfortable' | 'compact';

export interface StatsTableProps {
  /**
   * 渲染好的表格（通常是 <ThreeLineTable>）。
   *
   * 传函数则接收当前筛选关键词——筛选状态由本外壳持有，但过滤逻辑必须由
   * 表格自己执行（外壳不解析 payload，见文件头）。传 ReactNode 时筛选框
   * 收集到的关键词无处可去，所以 `filterable` 与函数式 children 应当成对使用。
   */
  children: ReactNode | ((filterKeyword: string) => ReactNode);
  /** 工具条标题，例如「基线特征表」。 */
  title?: string;
  /** 变量总数，用于工具条计数与筛选可用性判断。 */
  variableCount?: number;
  /** 分组列数，用于工具条计数。 */
  groupCount?: number;
  /**
   * 变量筛选回调。传入时才渲染筛选框：外壳只负责收集关键词，
   * 由调用方决定如何过滤（保持外壳与 payload 解耦）。
   */
  onFilterChange?: (keyword: string) => void;
  /** 关掉筛选框（例如回归系数表按变量筛选没有意义）。 */
  filterable?: boolean;
  /** 无障碍标签。 */
  ariaLabel?: string;
}

interface ShellProps extends StatsTableProps {
  density: TableDensity;
  onDensityChange: (density: TableDensity) => void;
  keyword: string;
  onKeywordChange: (keyword: string) => void;
  /** 全屏态下隐藏放大按钮，避免嵌套 Modal。 */
  onRequestFullscreen?: () => void;
}

function TableShell({
  children,
  title,
  variableCount,
  groupCount,
  filterable = true,
  ariaLabel,
  density,
  onDensityChange,
  keyword,
  onKeywordChange,
  onRequestFullscreen,
}: ShellProps) {
  const counts = [
    typeof variableCount === 'number' ? `${variableCount} 变量` : null,
    typeof groupCount === 'number' ? `${groupCount} 组` : null,
  ].filter(Boolean).join(' × ');

  return (
    <section className="stats-table" data-density={density} aria-label={ariaLabel ?? title ?? '统计表格'}>
      <header className="stats-table__toolbar">
        <div className="stats-table__caption">
          {title ? <strong>{title}</strong> : null}
          {counts ? <small>{counts}</small> : null}
        </div>

        <div className="stats-table__tools">
          {filterable ? (
            <Input
              className="stats-table__filter"
              size="small"
              allowClear
              value={keyword}
              onChange={(event) => onKeywordChange(event.target.value)}
              placeholder="筛选变量"
              prefix={<SearchOutlined />}
              aria-label="按变量名筛选表格行"
            />
          ) : null}

          <Tooltip title="紧凑模式隐藏每格的有效 n / 缺失副行，长表更易通览">
            <Segmented
              className="stats-table__density"
              size="small"
              value={density}
              onChange={(value) => onDensityChange(value as TableDensity)}
              aria-label="表格密度"
              options={[
                { value: 'comfortable', icon: <ColumnHeightOutlined />, title: '标准' },
                { value: 'compact', icon: <CompressOutlined />, title: '紧凑' },
              ]}
            />
          </Tooltip>

          {onRequestFullscreen ? (
            <Tooltip title="全屏查看（宽表推荐）">
              <Button
                size="small"
                type="text"
                icon={<FullscreenOutlined />}
                onClick={onRequestFullscreen}
                aria-label="全屏查看表格"
              />
            </Tooltip>
          ) : null}
        </div>
      </header>

      {typeof children === 'function' ? children(keyword) : children}
    </section>
  );
}

export function StatsTable(props: StatsTableProps) {
  const [density, setDensity] = useState<TableDensity>('comfortable');
  const [keyword, setKeyword] = useState('');
  const [fullscreen, setFullscreen] = useState(false);

  const { onFilterChange } = props;
  const handleKeyword = useMemo(
    () => (next: string) => {
      setKeyword(next);
      onFilterChange?.(next);
    },
    [onFilterChange],
  );

  const shared = {
    ...props,
    density,
    onDensityChange: setDensity,
    keyword,
    onKeywordChange: handleKeyword,
  };

  return (
    <>
      <TableShell {...shared} onRequestFullscreen={() => setFullscreen(true)} />

      <Modal
        open={fullscreen}
        onCancel={() => setFullscreen(false)}
        footer={null}
        width="96vw"
        className="stats-table-modal"
        title={props.title ?? '统计表格'}
        destroyOnHidden
      >
        {/* 密度与筛选状态由父组件持有，因此全屏视图延续当前设置。 */}
        <TableShell {...shared} />
      </Modal>
    </>
  );
}

export default StatsTable;
