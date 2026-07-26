import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { StatsTable } from './StatsTable';

function renderShell(props: Partial<React.ComponentProps<typeof StatsTable>> = {}) {
  return render(
    <StatsTable title="基线特征表" variableCount={18} groupCount={4} {...props}>
      <table>
        <tbody>
          <tr><td>年龄</td></tr>
        </tbody>
      </table>
    </StatsTable>,
  );
}

describe('StatsTable', () => {
  it('renders the caption with variable and group counts', () => {
    renderShell();
    expect(screen.getByText('基线特征表')).toBeInTheDocument();
    expect(screen.getByText('18 变量 × 4 组')).toBeInTheDocument();
  });

  it('renders children verbatim', () => {
    renderShell();
    expect(screen.getByText('年龄')).toBeInTheDocument();
  });

  it('defaults to comfortable density so existing visuals are unchanged', () => {
    const { container } = renderShell();
    expect(container.querySelector('.stats-table')).toHaveAttribute('data-density', 'comfortable');
  });

  it('switches density to compact, which is what shortens long tables', () => {
    const { container } = renderShell();
    // Segmented exposes its options as radio inputs; the compact one is second.
    const options = container.querySelectorAll('.ant-segmented-item-input');
    fireEvent.click(options[1]!);
    expect(container.querySelector('.stats-table')).toHaveAttribute('data-density', 'compact');
  });

  it('reports filter keywords to the caller without touching the table itself', () => {
    const onFilterChange = vi.fn();
    renderShell({ onFilterChange });
    fireEvent.change(screen.getByLabelText('按变量名筛选表格行'), { target: { value: '年龄' } });
    expect(onFilterChange).toHaveBeenCalledWith('年龄');
    // The shell must not remove rows on its own — filtering is the caller's call.
    expect(screen.getByText('年龄')).toBeInTheDocument();
  });

  it('hides the filter box when filtering makes no sense (e.g. coefficient tables)', () => {
    renderShell({ filterable: false });
    expect(screen.queryByLabelText('按变量名筛选表格行')).not.toBeInTheDocument();
  });

  it('opens a fullscreen dialog to escape the 820px conversation column', () => {
    renderShell();
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    fireEvent.click(screen.getByLabelText('全屏查看表格'));
    expect(screen.getByRole('dialog')).toBeInTheDocument();
  });

  it('does not offer a nested fullscreen button inside the dialog', () => {
    renderShell();
    fireEvent.click(screen.getByLabelText('全屏查看表格'));
    // One trigger stays behind the modal; the copy inside the dialog must not add another.
    expect(screen.getAllByLabelText('全屏查看表格')).toHaveLength(1);
  });

  it('carries the current density into the fullscreen view', () => {
    const { container } = renderShell();
    const options = container.querySelectorAll('.ant-segmented-item-input');
    fireEvent.click(options[1]!);
    fireEvent.click(screen.getByLabelText('全屏查看表格'));
    const shells = document.querySelectorAll('.stats-table');
    expect(shells.length).toBeGreaterThan(1);
    for (const shell of shells) {
      expect(shell).toHaveAttribute('data-density', 'compact');
    }
  });
});

describe('StatsTable filter keyword plumbing', () => {
  it('passes the current keyword to function children', () => {
    render(
      <StatsTable title="基线特征表">
        {(keyword) => <div data-testid="kw">{keyword === '' ? '(empty)' : keyword}</div>}
      </StatsTable>,
    );
    expect(screen.getByTestId('kw')).toHaveTextContent('(empty)');

    fireEvent.change(screen.getByLabelText('按变量名筛选表格行'), { target: { value: 'bmi' } });
    expect(screen.getByTestId('kw')).toHaveTextContent('bmi');
  });

  it('still renders plain node children unchanged', () => {
    render(
      <StatsTable title="回归系数表" filterable={false}>
        <div data-testid="plain">系数表</div>
      </StatsTable>,
    );
    expect(screen.getByTestId('plain')).toHaveTextContent('系数表');
  });

  it('keeps the keyword when the table is reopened in fullscreen', () => {
    render(
      <StatsTable title="基线特征表">
        {(keyword) => <div data-testid="kw">{keyword || '(empty)'}</div>}
      </StatsTable>,
    );
    fireEvent.change(screen.getByLabelText('按变量名筛选表格行'), { target: { value: 'age' } });
    fireEvent.click(screen.getByLabelText('全屏查看表格'));
    // 全屏视图与内联视图共享同一份 keyword 状态，两处都应显示筛选结果
    expect(screen.getAllByTestId('kw').map((n) => n.textContent)).toEqual(['age', 'age']);
  });
});
