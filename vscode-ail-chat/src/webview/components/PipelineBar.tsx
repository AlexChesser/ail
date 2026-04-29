import React from 'react';

interface PipelineBarProps {
  displayName: string | null;
  onLoad: () => void;
  onOpenGraph: () => void;
  onNewSession: () => void;
}

/**
 * Thin bar at the top of the chat panel showing the active pipeline.
 * Clicking the pipeline name or the folder icon opens the file picker.
 * The graph icon opens the pipeline graph visualizer.
 * The "+" icon clears the chat and starts a new session.
 */
export const PipelineBar: React.FC<PipelineBarProps> = ({ displayName, onLoad, onOpenGraph, onNewSession }) => (
  <div className="pipeline-bar">
    <span className="pipeline-bar-icon codicon codicon-symbol-file" />
    <button className="pipeline-bar-name" onClick={onLoad} title="Click to load a different pipeline">
      {displayName ?? 'Passthrough mode'}
    </button>
    <button className="pipeline-bar-load" onClick={onNewSession} title="New session (clear chat)">
      <span className="codicon codicon-add" />
    </button>
    <button className="pipeline-bar-load" onClick={onOpenGraph} title="Open pipeline graph">
      <span className="codicon codicon-type-hierarchy-sub" />
    </button>
    <button className="pipeline-bar-load" onClick={onLoad} title="Load pipeline file">
      <span className="codicon codicon-folder-opened" />
    </button>
  </div>
);
