import React from 'react';

interface PipelineBarProps {
  displayName: string | null;
  onLoad: () => void;
}

/**
 * Thin bar at the top of the chat panel showing the active pipeline.
 * Clicking the pipeline name or the folder icon opens the file picker.
 */
export const PipelineBar: React.FC<PipelineBarProps> = ({ displayName, onLoad }) => (
  <div className="pipeline-bar">
    <span className="pipeline-bar-icon codicon codicon-symbol-file" />
    <button className="pipeline-bar-name" onClick={onLoad} title="Click to load a different pipeline">
      {displayName ?? 'Passthrough mode'}
    </button>
    <button className="pipeline-bar-load" onClick={onLoad} title="Load pipeline file">
      <span className="codicon codicon-folder-opened" />
    </button>
  </div>
);
