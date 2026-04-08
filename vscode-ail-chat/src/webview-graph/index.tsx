/**
 * Graph webview entry point — mounts the React Flow pipeline visualizer.
 */

import React from 'react';
import { createRoot } from 'react-dom/client';
import { App } from './App';

interface ErrorBoundaryState {
  hasError: boolean;
  errorMessage: string;
}

class ErrorBoundary extends React.Component<{ children: React.ReactNode }, ErrorBoundaryState> {
  constructor(props: { children: React.ReactNode }) {
    super(props);
    this.state = { hasError: false, errorMessage: '' };
  }

  static getDerivedStateFromError(error: unknown): ErrorBoundaryState {
    const msg = error instanceof Error ? error.message : String(error);
    return { hasError: true, errorMessage: msg };
  }

  retry(): void {
    // Resetting state re-mounts <App />, whose useEffect will post { type: 'ready' }
    // to the host automatically, re-triggering _sendInit() with the current pipeline.
    this.setState({ hasError: false, errorMessage: '' });
  }

  render(): React.ReactNode {
    if (this.state.hasError) {
      return (
        <div
          style={{
            display: 'flex',
            flexDirection: 'column',
            alignItems: 'center',
            justifyContent: 'center',
            height: '100%',
            gap: 12,
            fontFamily: 'var(--vscode-font-family)',
            color: 'var(--vscode-errorForeground)',
          }}
        >
          <div style={{ fontSize: 13 }}>Graph render error: {this.state.errorMessage}</div>
          <button
            onClick={() => this.retry()}
            style={{
              background: 'var(--vscode-button-background)',
              color: 'var(--vscode-button-foreground)',
              border: 'none',
              borderRadius: 3,
              padding: '6px 14px',
              fontSize: 12,
              cursor: 'pointer',
              fontFamily: 'var(--vscode-font-family)',
            }}
          >
            Retry
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}

const rootEl = document.getElementById('root');
if (rootEl) {
  const root = createRoot(rootEl);
  root.render(
    <ErrorBoundary>
      <App />
    </ErrorBoundary>
  );
}
