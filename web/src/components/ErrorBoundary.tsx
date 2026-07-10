/**
 * Lightweight error boundary so a single panel crash (e.g. result table)
 * cannot blank the entire SPA.
 */

import { Component, type ErrorInfo, type ReactNode } from 'react';
import { Alert, Button } from 'antd';

export interface ErrorBoundaryProps {
  children: ReactNode;
  /** Optional short title shown above the error message. */
  title?: string;
  /** Optional reset key — when it changes, the boundary recovers. */
  resetKey?: string | number;
}

interface State {
  error: Error | null;
}

export class ErrorBoundary extends Component<ErrorBoundaryProps, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    // Keep a console trail for local debugging; never rethrow.
    console.error('[ErrorBoundary]', error, info.componentStack);
  }

  componentDidUpdate(prev: ErrorBoundaryProps): void {
    if (prev.resetKey !== this.props.resetKey && this.state.error) {
      this.setState({ error: null });
    }
  }

  private handleRetry = (): void => {
    this.setState({ error: null });
  };

  render(): ReactNode {
    const { error } = this.state;
    if (!error) return this.props.children;

    return (
      <Alert
        type="error"
        showIcon
        message={this.props.title ?? '界面渲染出错'}
        description={
          <div>
            <div style={{ marginBottom: 8, wordBreak: 'break-word' }}>
              {error.message || '未知错误'}
            </div>
            <Button size="small" onClick={this.handleRetry}>
              重试此区域
            </Button>
          </div>
        }
        style={{ margin: '8px 0' }}
      />
    );
  }
}

export default ErrorBoundary;
