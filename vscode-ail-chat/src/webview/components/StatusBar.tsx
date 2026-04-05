import React, { useState, useEffect } from 'react';

export interface StatusBarProps {
  isRunning: boolean;
  startTime: number | null;
  totalTokens: number;
  onStop: () => void;
}

export const StatusBar: React.FC<StatusBarProps> = ({ isRunning, startTime, totalTokens, onStop }) => {
  const [elapsed, setElapsed] = useState(0);

  useEffect(() => {
    if (!isRunning || startTime === null) {
      setElapsed(0);
      return;
    }

    setElapsed(Math.floor((Date.now() - startTime) / 1000));

    const interval = setInterval(() => {
      setElapsed(Math.floor((Date.now() - startTime) / 1000));
    }, 1000);

    return () => clearInterval(interval);
  }, [isRunning, startTime]);

  if (!isRunning) return null;

  const parts: string[] = [];
  if (elapsed > 0) parts.push(`${elapsed}s`);
  if (totalTokens > 0) parts.push(`${totalTokens.toLocaleString()} tokens`);
  const metrics = parts.length > 0 ? ` (${parts.join(' \u00B7 ')})` : '';

  return (
    <div className="chat-input-status">
      <span className="chat-input-status-sparkle codicon codicon-sparkle" />
      <span className="chat-input-status-label">Working...{metrics}</span>
      <button className="chat-input-status-stop" onClick={onStop}>esc to interrupt</button>
    </div>
  );
};
