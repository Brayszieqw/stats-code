/**
 * ModeToggle — simple/pro view-mode switcher (AntD Segmented).
 *
 * Controlled by `mode` / `onChange`. Always usable, even when the active
 * session is read-only (disabled is hard-coded false, R9.4). Carries an
 * accessible label (R1.2, R10.2).
 *
 * Validates: Requirements 1.2, 9.4, 10.2
 */

import { Segmented } from 'antd';
import type { ViewMode } from '../hooks/useModePreference';

export interface ModeToggleProps {
  mode: ViewMode;
  onChange: (m: ViewMode) => void;
}

export function ModeToggle({ mode, onChange }: ModeToggleProps) {
  return (
    <Segmented<ViewMode>
      aria-label="界面模式切换"
      value={mode}
      onChange={(value) => onChange(value)}
      disabled={false}
      options={[
        { label: '简易', value: 'simple' },
        { label: '专业', value: 'pro' },
      ]}
    />
  );
}

export default ModeToggle;
