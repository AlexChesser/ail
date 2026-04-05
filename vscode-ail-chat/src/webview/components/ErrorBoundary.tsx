import React from 'react';

interface ErrorBoundaryState {
  hasError: boolean;
  message: string;
}

interface ErrorBoundaryProps {
  children: React.ReactNode;
}

/**
 * Catches render errors in child components and shows a fallback instead of
 * unmounting the entire app tree.
 */
export class ErrorBoundary extends React.Component<ErrorBoundaryProps, ErrorBoundaryState> {
  constructor(props: ErrorBoundaryProps) {
    super(props);
    this.state = { hasError: false, message: '' };
  }

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { hasError: true, message: error.message };
  }

  override render(): React.ReactNode {
    if (this.state.hasError) {
      return (
        <div className="error-message">
          <span className="error-message-icon codicon codicon-error" />
          <span>Render error: {this.state.message}</span>
        </div>
      );
    }
    return this.props.children;
  }
}
