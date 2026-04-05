/**
 * App — root React component for the ail Chat webview.
 *
 * Handles postMessage from the extension host (HostToWebviewMessage) and
 * accumulates all chat state: messages, tool calls, thinking blocks,
 * HITL gates, permission requests, and step progress.
 */

import React, { useEffect, useReducer, useRef } from 'react';
import { HostToWebviewMessage, WebviewToHostMessage, SessionSummary } from '../types';
import { ChatMessage } from './components/ChatMessage';
import { ThinkingBlock } from './components/ThinkingBlock';
import { ToolCallCard, ToolCallData } from './components/ToolCallCard';
import { HitlCard, HitlCardState } from './components/HitlCard';
import { PermissionCard, PermissionCardState } from './components/PermissionCard';
import { StepProgress, StepInfo, StepStatus } from './components/StepProgress';
import { ChatInput } from './components/ChatInput';
import { SessionList } from './components/SessionList';
import { StatusBar } from './components/StatusBar';

// ── VS Code API ────────────────────────────────────────────────────────────────

declare function acquireVsCodeApi(): {
  postMessage: (msg: WebviewToHostMessage) => void;
  getState: () => unknown;
  setState: (state: unknown) => void;
};

// Acquire once; do not call acquireVsCodeApi() more than once per session.
// eslint-disable-next-line @typescript-eslint/no-explicit-any
const vscode = (typeof acquireVsCodeApi !== 'undefined' ? acquireVsCodeApi() : null) as any;

function postToHost(msg: WebviewToHostMessage): void {
  // eslint-disable-next-line @typescript-eslint/no-unsafe-call, @typescript-eslint/no-unsafe-member-access
  vscode?.postMessage(msg);
}

// ── Display items ──────────────────────────────────────────────────────────────

/** A single item in the chat display list. */
export type DisplayItem =
  | { kind: 'user-message'; id: string; text: string }
  | { kind: 'assistant-stream'; id: string; text: string; streaming: boolean }
  | { kind: 'thinking'; id: string; text: string }
  | { kind: 'tool-call'; id: string; data: ToolCallData }
  | { kind: 'hitl'; id: string; stepId: string; message?: string; cardState: HitlCardState; resolvedText?: string }
  | { kind: 'permission'; id: string; displayName: string; displayDetail: string; cardState: PermissionCardState; resolvedAllowed?: boolean }
  | { kind: 'error'; id: string; message: string };

// ── State ──────────────────────────────────────────────────────────────────────

interface ChatState {
  items: DisplayItem[];
  steps: StepInfo[];
  totalCostUsd: number;
  totalInputTokens: number;
  totalOutputTokens: number;
  runStartTime: number | null;
  isRunning: boolean;
  sessions: SessionSummary[];
  activeSessionId: string | null;
  showSessions: boolean;
  /** Counter to generate unique IDs for display items. */
  _idCounter: number;
}

const initialState: ChatState = {
  items: [],
  steps: [],
  totalCostUsd: 0,
  totalInputTokens: 0,
  totalOutputTokens: 0,
  runStartTime: null,
  isRunning: false,
  sessions: [],
  activeSessionId: null,
  showSessions: false,
  _idCounter: 0,
};

// ── Actions ────────────────────────────────────────────────────────────────────

type Action =
  | { type: 'HOST_MSG'; msg: HostToWebviewMessage }
  | { type: 'USER_SUBMIT'; text: string }
  | { type: 'HITL_APPROVE'; stepId: string; text?: string }
  | { type: 'HITL_REJECT'; stepId: string }
  | { type: 'PERMISSION_ALLOW' }
  | { type: 'PERMISSION_DENY' }
  | { type: 'STOP' }
  | { type: 'SELECT_SESSION'; id: string }
  | { type: 'NEW_SESSION' }
  | { type: 'TOGGLE_SESSIONS' };

function nextId(state: ChatState): [string, ChatState] {
  const id = String(state._idCounter);
  return [id, { ...state, _idCounter: state._idCounter + 1 }];
}

function updateItem(items: DisplayItem[], id: string, updater: (item: DisplayItem) => DisplayItem): DisplayItem[] {
  return items.map((it) => it.id === id ? updater(it) : it);
}

/** Find the last assistant-stream item id. */
function lastStreamId(items: DisplayItem[]): string | null {
  for (let i = items.length - 1; i >= 0; i--) {
    if (items[i].kind === 'assistant-stream') return items[i].id;
  }
  return null;
}

/** Find pending HITL card id by stepId. */
function findHitlId(items: DisplayItem[], stepId: string): string | null {
  for (const it of items) {
    if (it.kind === 'hitl' && it.stepId === stepId) return it.id;
  }
  return null;
}

/** Find pending permission card id. */
function findPermissionId(items: DisplayItem[]): string | null {
  for (const it of items) {
    if (it.kind === 'permission' && it.cardState === 'pending') return it.id;
  }
  return null;
}

/** Find tool call by toolUseId. */
function findToolCallId(items: DisplayItem[], toolUseId: string): string | null {
  for (const it of items) {
    if (it.kind === 'tool-call' && it.data.toolUseId === toolUseId) return it.id;
  }
  return null;
}

function updateStep(steps: StepInfo[], stepId: string, updates: Partial<StepInfo>): StepInfo[] {
  return steps.map((s) => s.stepId === stepId ? { ...s, ...updates } : s);
}

function reducer(state: ChatState, action: Action): ChatState {
  switch (action.type) {
    case 'USER_SUBMIT': {
      const [id, s2] = nextId(state);
      return {
        ...s2,
        isRunning: true,
        items: [...s2.items, { kind: 'user-message', id, text: action.text }],
      };
    }

    case 'HOST_MSG': {
      const msg = action.msg;
      switch (msg.type) {
        case 'runStarted': {
          return { ...state, isRunning: true, steps: [], totalCostUsd: 0, totalInputTokens: 0, totalOutputTokens: 0, runStartTime: Date.now() };
        }

        case 'stepStarted': {
          const existing = state.steps.find((s) => s.stepId === msg.stepId);
          if (existing) {
            return { ...state, steps: updateStep(state.steps, msg.stepId, { status: 'running' }) };
          }
          return {
            ...state,
            steps: [...state.steps, { stepId: msg.stepId, status: 'running' }],
          };
        }

        case 'streamDelta': {
          const streamId = lastStreamId(state.items);
          if (streamId !== null) {
            return {
              ...state,
              items: updateItem(state.items, streamId, (it) =>
                it.kind === 'assistant-stream'
                  ? { ...it, text: it.text + msg.text }
                  : it
              ),
            };
          }
          // Start a new stream item
          const [id, s2] = nextId(state);
          return {
            ...s2,
            items: [...s2.items, { kind: 'assistant-stream', id, text: msg.text, streaming: true }],
          };
        }

        case 'thinking': {
          const [id, s2] = nextId(state);
          return {
            ...s2,
            items: [...s2.items, { kind: 'thinking', id, text: msg.text }],
          };
        }

        case 'toolUse': {
          const [id, s2] = nextId(state);
          return {
            ...s2,
            items: [...s2.items, {
              kind: 'tool-call',
              id,
              data: {
                toolUseId: msg.toolUseId,
                toolName: msg.toolName,
                input: msg.input,
              },
            }],
          };
        }

        case 'toolResult': {
          const tcId = findToolCallId(state.items, msg.toolUseId);
          if (tcId !== null) {
            return {
              ...state,
              items: updateItem(state.items, tcId, (it) =>
                it.kind === 'tool-call'
                  ? { ...it, data: { ...it.data, result: msg.content, isError: msg.isError } }
                  : it
              ),
            };
          }
          return state;
        }

        case 'stepCompleted': {
          const costDelta = msg.costUsd ?? 0;
          return {
            ...state,
            totalCostUsd: state.totalCostUsd + costDelta,
            totalInputTokens: state.totalInputTokens + (msg.inputTokens ?? 0),
            totalOutputTokens: state.totalOutputTokens + (msg.outputTokens ?? 0),
            steps: updateStep(state.steps, msg.stepId, { status: 'completed', costUsd: msg.costUsd ?? undefined }),
            // Mark any pending stream item as no longer streaming
            items: state.items.map((it) =>
              it.kind === 'assistant-stream' && it.streaming ? { ...it, streaming: false } : it
            ),
          };
        }

        case 'stepSkipped':
          return { ...state, steps: updateStep(state.steps, msg.stepId, { status: 'skipped' }) };

        case 'stepFailed': {
          const [id, s2] = nextId(state);
          return {
            ...s2,
            steps: updateStep(s2.steps, msg.stepId, { status: 'failed' }),
            items: [...s2.items, { kind: 'error', id, message: `Step '${msg.stepId}' failed: ${msg.error}` }],
          };
        }

        case 'hitlGate': {
          // Cancel any streaming in progress
          const itemsWithStoppedStream = state.items.map((it) =>
            it.kind === 'assistant-stream' && it.streaming ? { ...it, streaming: false } : it
          );
          const [id, s2] = nextId({ ...state, items: itemsWithStoppedStream });
          return {
            ...s2,
            items: [...s2.items, {
              kind: 'hitl',
              id,
              stepId: msg.stepId,
              message: msg.message,
              cardState: 'pending',
            }],
          };
        }

        case 'permissionRequested': {
          const [id, s2] = nextId(state);
          return {
            ...s2,
            items: [...s2.items, {
              kind: 'permission',
              id,
              displayName: msg.displayName,
              displayDetail: msg.displayDetail,
              cardState: 'pending',
            }],
          };
        }

        case 'pipelineCompleted': {
          // Cancel any pending HITL/permission cards
          const cancelledItems = state.items.map((it) => {
            if (it.kind === 'hitl' && it.cardState === 'pending') {
              return { ...it, cardState: 'cancelled' as HitlCardState };
            }
            if (it.kind === 'permission' && it.cardState === 'pending') {
              return { ...it, cardState: 'resolved' as PermissionCardState };
            }
            if (it.kind === 'assistant-stream' && it.streaming) {
              return { ...it, streaming: false };
            }
            return it;
          });
          return { ...state, isRunning: false, runStartTime: null, items: cancelledItems };
        }

        case 'pipelineError':
        case 'processError': {
          const [id, s2] = nextId(state);
          const errorMsg = msg.type === 'pipelineError' ? msg.error : msg.message;
          return {
            ...s2,
            isRunning: false,
            runStartTime: null,
            items: [...s2.items.map((it) =>
              it.kind === 'assistant-stream' && it.streaming ? { ...it, streaming: false } : it
            ), { kind: 'error', id, message: errorMsg }],
          };
        }

        case 'sessionsUpdated':
          return { ...state, sessions: msg.sessions };

        default:
          return state;
      }
    }

    case 'HITL_APPROVE': {
      const hitlId = findHitlId(state.items, action.stepId);
      if (hitlId === null) return state;
      return {
        ...state,
        items: updateItem(state.items, hitlId, (it) =>
          it.kind === 'hitl' ? { ...it, cardState: 'resolved', resolvedText: action.text ?? 'Approved' } : it
        ),
      };
    }

    case 'HITL_REJECT': {
      const hitlId = findHitlId(state.items, action.stepId);
      if (hitlId === null) return state;
      return {
        ...state,
        isRunning: false,
        items: updateItem(state.items, hitlId, (it) =>
          it.kind === 'hitl' ? { ...it, cardState: 'resolved', resolvedText: 'Rejected' } : it
        ),
      };
    }

    case 'PERMISSION_ALLOW': {
      const pId = findPermissionId(state.items);
      if (pId === null) return state;
      return {
        ...state,
        items: updateItem(state.items, pId, (it) =>
          it.kind === 'permission' ? { ...it, cardState: 'resolved', resolvedAllowed: true } : it
        ),
      };
    }

    case 'PERMISSION_DENY': {
      const pId = findPermissionId(state.items);
      if (pId === null) return state;
      return {
        ...state,
        items: updateItem(state.items, pId, (it) =>
          it.kind === 'permission' ? { ...it, cardState: 'resolved', resolvedAllowed: false } : it
        ),
      };
    }

    case 'STOP':
      return { ...state, isRunning: false, runStartTime: null };

    case 'SELECT_SESSION':
      return { ...state, activeSessionId: action.id };

    case 'NEW_SESSION':
      return { ...state, items: [], steps: [], totalCostUsd: 0, totalInputTokens: 0, totalOutputTokens: 0, runStartTime: null, isRunning: false, activeSessionId: null };

    case 'TOGGLE_SESSIONS':
      return { ...state, showSessions: !state.showSessions };

    default:
      return state;
  }
}

// ── App component ──────────────────────────────────────────────────────────────

export const App: React.FC = () => {
  const [state, dispatch] = useReducer(reducer, initialState);
  const messageListRef = useRef<HTMLDivElement>(null);

  // Listen for messages from the extension host
  useEffect(() => {
    const handler = (event: MessageEvent) => {
      const msg = event.data as HostToWebviewMessage;
      dispatch({ type: 'HOST_MSG', msg });
    };
    window.addEventListener('message', handler);
    // Signal readiness to the host
    postToHost({ type: 'ready' });
    return () => window.removeEventListener('message', handler);
  }, []);

  // Auto-scroll when new items arrive
  useEffect(() => {
    if (messageListRef.current) {
      messageListRef.current.scrollTop = messageListRef.current.scrollHeight;
    }
  }, [state.items]);

  const handleSubmit = (text: string) => {
    dispatch({ type: 'USER_SUBMIT', text });
    postToHost({ type: 'submitPrompt', text });
  };

  const handleStop = () => {
    dispatch({ type: 'STOP' });
    postToHost({ type: 'killProcess' });
  };

  const handleHitlApprove = (stepId: string) => {
    dispatch({ type: 'HITL_APPROVE', stepId });
    postToHost({ type: 'hitlResponse', stepId, text: 'Approved' });
  };

  const handleHitlReject = (stepId: string) => {
    dispatch({ type: 'HITL_REJECT', stepId });
    postToHost({ type: 'hitlResponse', stepId, text: 'Rejected' });
  };

  const handlePermissionAllow = () => {
    dispatch({ type: 'PERMISSION_ALLOW' });
    postToHost({ type: 'permissionResponse', allowed: true });
  };

  const handlePermissionDeny = () => {
    dispatch({ type: 'PERMISSION_DENY' });
    postToHost({ type: 'permissionResponse', allowed: false });
  };

  const handleSelectSession = (id: string) => {
    dispatch({ type: 'SELECT_SESSION', id });
    postToHost({ type: 'switchSession', sessionId: id });
  };

  const handleNewSession = () => {
    dispatch({ type: 'NEW_SESSION' });
    postToHost({ type: 'newSession' });
  };

  const hasPendingHitl = state.items.some((it) => it.kind === 'hitl' && it.cardState === 'pending');
  const hasPendingPermission = state.items.some((it) => it.kind === 'permission' && it.cardState === 'pending');
  const inputDisabled = hasPendingHitl || hasPendingPermission;

  return (
    <>
      {state.showSessions && (
        <SessionList
          sessions={state.sessions}
          activeSessionId={state.activeSessionId}
          onSelectSession={handleSelectSession}
          onNewSession={handleNewSession}
        />
      )}
      <div className="chat-panel">
        {state.steps.length > 0 && (
          <StepProgress
            steps={state.steps}
            totalCostUsd={!state.isRunning ? state.totalCostUsd : undefined}
          />
        )}
        <div className="message-list" ref={messageListRef}>
          {state.items.length === 0 && (
            <div className="empty-state">
              <div className="empty-state-title">What would you like to build?</div>
              <div className="empty-state-subtitle">Describe a task to get started with your ail pipeline.</div>
            </div>
          )}
          {state.items.map((item) => {
            switch (item.kind) {
              case 'user-message':
                return <ChatMessage key={item.id} role="user" content={item.text} />;
              case 'assistant-stream':
                return <ChatMessage key={item.id} role="assistant" content={item.text} streaming={item.streaming} />;
              case 'thinking':
                return <ThinkingBlock key={item.id} text={item.text} />;
              case 'tool-call':
                return <ToolCallCard key={item.id} data={item.data} />;
              case 'hitl':
                return (
                  <HitlCard
                    key={item.id}
                    stepId={item.stepId}
                    message={item.message}
                    cardState={item.cardState}
                    resolvedText={item.resolvedText}
                    onApprove={handleHitlApprove}
                    onReject={handleHitlReject}
                  />
                );
              case 'permission':
                return (
                  <PermissionCard
                    key={item.id}
                    displayName={item.displayName}
                    displayDetail={item.displayDetail}
                    cardState={item.cardState}
                    resolvedAllowed={item.resolvedAllowed}
                    onAllow={handlePermissionAllow}
                    onDeny={handlePermissionDeny}
                  />
                );
              case 'error':
                return (
                  <div key={item.id} className="error-message">
                    <span className="error-message-icon codicon codicon-error" />
                    <span>{item.message}</span>
                  </div>
                );
            }
          })}
        </div>
        <StatusBar
          isRunning={state.isRunning}
          startTime={state.runStartTime}
          totalTokens={state.totalInputTokens + state.totalOutputTokens}
          onStop={handleStop}
        />
        <ChatInput
          onSubmit={handleSubmit}
          onStop={handleStop}
          isRunning={state.isRunning}
          disabled={inputDisabled}
          placeholder={
            hasPendingHitl ? 'Waiting for your review…'
            : hasPendingPermission ? 'Waiting for permission response…'
            : undefined
          }
        />
      </div>
    </>
  );
};
