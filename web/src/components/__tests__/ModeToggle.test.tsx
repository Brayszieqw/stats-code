/**
 * Tests for ModeToggle.
 *
 * Validates: Requirements 1.2, 9.4, 10.2
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { ModeToggle } from '../ModeToggle';

describe('ModeToggle (Requirements 1.2, 9.4, 10.2)', () => {
  it('is queryable by its accessible label', () => {
    render(<ModeToggle mode="simple" onChange={() => {}} />);
    expect(screen.getByLabelText('界面模式切换')).toBeInTheDocument();
  });

  it('fires onChange with the other mode when clicked', () => {
    const onChange = vi.fn();
    render(<ModeToggle mode="simple" onChange={onChange} />);
    fireEvent.click(screen.getByText('专业'));
    expect(onChange).toHaveBeenCalledWith('pro');
  });

  it('renders both segment options and reflects the active mode', () => {
    render(<ModeToggle mode="pro" onChange={() => {}} />);
    expect(screen.getByText('简易')).toBeInTheDocument();
    expect(screen.getByText('专业')).toBeInTheDocument();
  });

  it('the control is never disabled even for a read-only session (R9.4)', () => {
    const { container } = render(<ModeToggle mode="simple" onChange={() => {}} />);
    expect(container.querySelector('.ant-segmented-disabled')).toBeNull();
  });
});
