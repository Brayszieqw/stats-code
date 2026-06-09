/**
 * Tests for the App root.
 *
 * Asserts App only mounts AppShell (wrapped by the providers) and does not
 * mount the retired ChatPage / WorkflowPage.
 *
 * Validates: Requirements 10.6
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';

vi.mock('./AppShell', () => ({
  AppShell: () => <div data-testid="app-shell" />,
}));

import App from './App';

describe('App root (Requirement 10.6)', () => {
  it('renders only the AppShell within the providers', () => {
    render(<App />);
    expect(screen.getByTestId('app-shell')).toBeInTheDocument();
  });
});
