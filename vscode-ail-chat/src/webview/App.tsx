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
import { AskUserQuestionCard, AskUserQuestion, AskUserCardState } from './components/AskUserQuestionCard';
import { ErrorBoundary } from './components/ErrorBoundary';
import { ToolCallGroup } from './components/ToolCallGroup';
import { StepProgress, StepInfo } from './components/StepProgress';
import { ChatInput } from './components/ChatInput';
import { SessionList } from './components/SessionList';
import { StatusBar } from './components/StatusBar';
import { PipelineBar } from './components/PipelineBar';

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
  | { kind: 'user-message'; id: string; text: string; timestamp: number }
  | { kind: 'assistant-stream'; id: string; text: string; streaming: boolean; stepId?: string; timestamp: number }
  | { kind: 'thinking'; id: string; text: string }
  | { kind: 'tool-call'; id: string; data: ToolCallData }
  | { kind: 'hitl'; id: string; stepId: string; message?: string; cardState: HitlCardState; resolvedText?: string }
  | { kind: 'permission'; id: string; displayName: string; displayDetail: string; cardState: PermissionCardState; resolvedAllowed?: boolean }
  | { kind: 'ask-user-question'; id: string; questions: AskUserQuestion[]; cardState: AskUserCardState; resolvedAnswer?: string }
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
  /** Active pipeline path and display name, or null for passthrough mode. */
  activePipeline: { path: string; displayName: string } | null;
  /** Counter to generate unique IDs for display items. */
  _idCounter: number;
  /** The step ID currently being processed — set on stepStarted, cleared on completion. */
  currentStepId: string | null;
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
  activePipeline: null,
  _idCounter: 0,
  currentStepId: null,
};

// ── Actions ────────────────────────────────────────────────────────────────────

type Action =
  | { type: 'HOST_MSG'; msg: HostToWebviewMessage }
  | { type: 'USER_SUBMIT'; text: string }
  | { type: 'HITL_APPROVE'; stepId: string; text?: string }
  | { type: 'HITL_REJECT'; stepId: string }
  | { type: 'PERMISSION_ALLOW' }
  | { type: 'PERMISSION_ALLOW_SESSION' }
  | { type: 'PERMISSION_DENY' }
  | { type: 'ASK_USER_SUBMIT'; answer: string }
  | { type: 'ASK_USER_DENY' }
  | { type: 'STOP' }
  | { type: 'SELECT_SESSION'; id: string }
  | { type: 'NEW_SESSION' }
  | { type: 'TOGGLE_SESSIONS' }
  | { type: 'LOAD_PIPELINE' };

/**
 * Normalise a raw `AskUserQuestion` options value into `AskUserQuestionOption[]`.
 * Handles: proper array of objects, string array, JSON-encoded string, null/undefined.
 */
function normalizeOptions(raw: unknown): AskUserQuestion['options'] {
  let arr: unknown = raw;
  if (typeof arr === 'string') {
    try { arr = JSON.parse(arr); } catch { arr = []; }
  }
  if (!Array.isArray(arr)) return [];
  return arr.map((opt) => {
    if (opt == null) return { label: '' };
    if (typeof opt === 'string') return { label: opt };
    if (typeof opt === 'object') {
      const o = opt as Record<string, unknown>;
      return { label: String(o['label'] ?? ''), description: o['description'] != null ? String(o['description']) : undefined };
    }
    return { label: String(opt) };
  });
}

/**
 * Normalise a raw `AskUserQuestion` question item into a clean `AskUserQuestion`.
 * Handles string/boolean coercions for multiSelect and stringified options.
 */
function normalizeQuestion(raw: unknown): AskUserQuestion | null {
  if (raw == null || typeof raw !== 'object') return null;
  const q = raw as Record<string, unknown>;
  const question = typeof q['question'] === 'string' ? q['question'] : '';
  if (!question) return null;
  const multiSelect = q['multiSelect'] === true || String(q['multiSelect']).toLowerCase() === 'true';
  return {
    header: typeof q['header'] === 'string' ? q['header'] : '',
    question,
    multiSelect,
    options: normalizeOptions(q['options']),
  };
}

/**
 * Extract a `AskUserQuestion[]` from the raw `toolInput` of a permissionRequested event.
 * Handles both the canonical `{questions:[...]}` wrapper and flat `{question, options}` forms.
 * Returns null when the input is not recognisable as an AskUserQuestion payload.
 */
function parseAskUserQuestions(toolInput: unknown): AskUserQuestion[] | null {
  if (toolInput == null || typeof toolInput !== 'object') return null;
  const inp = toolInput as Record<string, unknown>;

  // Canonical format: { questions: [...] }
  if (Array.isArray(inp['questions']) && inp['questions'].length > 0) {
    const questions = inp['questions'].map(normalizeQuestion).filter((q): q is AskUserQuestion => q !== null);
    if (questions.length > 0) return questions;
  }

  // Flat format: { question: "...", options: [...] }
  if (typeof inp['question'] === 'string' && inp['question']) {
    const q = normalizeQuestion(inp);
    if (q) return [q];
  }

  return null;
}

function nextId(state: ChatState): [string, ChatState] {
  const id = String(state._idCounter);
  return [id, { ...state, _idCounter: state._idCounter + 1 }];
}

function updateItem(items: DisplayItem[], id: string, updater: (item: DisplayItem) => DisplayItem): DisplayItem[] {
  return items.map((it) => it.id === id ? updater(it) : it);
}

/** Find the last assistant-stream item id that is still actively streaming. */
function lastStreamId(items: DisplayItem[], stepId?: string | null): string | null {
  for (let i = items.length - 1; i >= 0; i--) {
    const item = items[i];
    if (item.kind === 'assistant-stream' && item.streaming) {
      if (stepId && item.stepId && item.stepId !== stepId) continue;
      return item.id;
    }
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

/** Find pending ask-user-question card id. */
function findAskUserId(items: DisplayItem[]): string | null {
  for (const it of items) {
    if (it.kind === 'ask-user-question' && it.cardState === 'pending') return it.id;
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
        items: [...s2.items, { kind: 'user-message', id, text: action.text, timestamp: Date.now() }],
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
            return { ...state, currentStepId: msg.stepId, steps: updateStep(state.steps, msg.stepId, { status: 'running' }) };
          }
          return {
            ...state,
            currentStepId: msg.stepId,
            steps: [...state.steps, { stepId: msg.stepId, status: 'running' }],
          };
        }

        case 'streamDelta': {
          const streamId = lastStreamId(state.items, state.currentStepId);
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
            items: [...s2.items, { kind: 'assistant-stream', id, text: msg.text, streaming: true, stepId: state.currentStepId ?? undefined, timestamp: Date.now() }],
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
          const hasActiveStream = state.items.some((it) => it.kind === 'assistant-stream' && it.streaming);
          const closedItems = state.items.map((it) =>
            it.kind === 'assistant-stream' && it.streaming ? { ...it, streaming: false } : it
          );
          // Fallback: if no streaming events arrived but the step has a response, add it now.
          let finalState: ChatState = state;
          let finalItems: DisplayItem[] = closedItems;
          if (!hasActiveStream && msg.response) {
            const [id, s2] = nextId(state);
            finalState = s2;
            finalItems = [...closedItems, { kind: 'assistant-stream' as const, id, text: msg.response, streaming: false, timestamp: Date.now() }];
          }
          return {
            ...finalState,
            totalCostUsd: state.totalCostUsd + costDelta,
            totalInputTokens: state.totalInputTokens + (msg.inputTokens ?? 0),
            totalOutputTokens: state.totalOutputTokens + (msg.outputTokens ?? 0),
            steps: updateStep(state.steps, msg.stepId, { status: 'completed', costUsd: msg.costUsd ?? undefined }),
            items: finalItems,
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
          // Detect AskUserQuestion tool by displayName — intercept as structured question UI
          if (msg.displayName === 'AskUserQuestion') {
            const questions = parseAskUserQuestions(msg.toolInput);
            if (questions !== null) {
              const [id, s2] = nextId(state);
              return {
                ...s2,
                items: [...s2.items, {
                  kind: 'ask-user-question',
                  id,
                  questions,
                  cardState: 'pending' as AskUserCardState,
                }],
              };
            }
          }
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
          // Cancel any pending HITL/permission/ask-user cards
          const cancelledItems = state.items.map((it) => {
            if (it.kind === 'hitl' && it.cardState === 'pending') {
              return { ...it, cardState: 'cancelled' as HitlCardState };
            }
            if (it.kind === 'permission' && it.cardState === 'pending') {
              return { ...it, cardState: 'resolved' as PermissionCardState };
            }
            if (it.kind === 'ask-user-question' && it.cardState === 'pending') {
              return { ...it, cardState: 'resolved' as AskUserCardState };
            }
            if (it.kind === 'assistant-stream' && it.streaming) {
              return { ...it, streaming: false };
            }
            return it;
          });
          return { ...state, isRunning: false, runStartTime: null, currentStepId: null, items: cancelledItems };
        }

        case 'pipelineError':
        case 'processError': {
          const [id, s2] = nextId(state);
          const errorMsg = msg.type === 'pipelineError' ? msg.error : msg.message;
          return {
            ...s2,
            isRunning: false,
            runStartTime: null,
            currentStepId: null,
            items: [...s2.items.map((it) =>
              it.kind === 'assistant-stream' && it.streaming ? { ...it, streaming: false } : it
            ), { kind: 'error', id, message: errorMsg }],
          };
        }

        case 'sessionsUpdated':
          return { ...state, sessions: msg.sessions };

        case 'pipelineChanged':
          return {
            ...state,
            activePipeline: msg.path ? { path: msg.path, displayName: msg.displayName ?? msg.path } : null,
          };

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

    case 'PERMISSION_ALLOW_SESSION': {
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

    case 'ASK_USER_SUBMIT': {
      const auId = findAskUserId(state.items);
      if (auId === null) return state;
      return {
        ...state,
        items: updateItem(state.items, auId, (it) =>
          it.kind === 'ask-user-question' ? { ...it, cardState: 'resolved', resolvedAnswer: action.answer } : it
        ),
      };
    }

    case 'ASK_USER_DENY': {
      const auId = findAskUserId(state.items);
      if (auId === null) return state;
      return {
        ...state,
        items: updateItem(state.items, auId, (it) =>
          it.kind === 'ask-user-question' ? { ...it, cardState: 'resolved' } : it
        ),
      };
    }

    case 'STOP': {
      const stoppedItems = state.items.map((it) => {
        if (it.kind === 'tool-call' && it.data.result === undefined) {
          return { ...it, data: { ...it.data, isStopped: true } };
        }
        if (it.kind === 'assistant-stream' && it.streaming) {
          return { ...it, streaming: false };
        }
        return it;
      });
      return { ...state, isRunning: false, runStartTime: null, currentStepId: null, items: stoppedItems };
    }

    case 'SELECT_SESSION':
      return { ...state, activeSessionId: action.id };

    case 'NEW_SESSION':
      return { ...state, items: [], steps: [], totalCostUsd: 0, totalInputTokens: 0, totalOutputTokens: 0, runStartTime: null, isRunning: false, activeSessionId: null, currentStepId: null };

    case 'TOGGLE_SESSIONS':
      return { ...state, showSessions: !state.showSessions };

    case 'LOAD_PIPELINE':
      return state; // Side-effected in the component via postToHost

    default:
      return state;
  }
}

// ── Render-time grouping ───────────────────────────────────────────────────────

type GroupedItem =
  | { kind: 'single'; item: DisplayItem }
  | { kind: 'tool-group'; items: DisplayItem[]; allResolved: boolean };

function isToolGroupable(item: DisplayItem): boolean {
  if (item.kind === 'tool-call') return true;
  // Pending permissions stay standalone so the approval UI is always visible.
  // Once resolved, they join the group on the next render.
  if (item.kind === 'permission') return item.cardState === 'resolved';
  return false;
}

function isGroupResolved(items: DisplayItem[]): boolean {
  return items.every((it) => {
    if (it.kind === 'tool-call') return it.data.result !== undefined;
    if (it.kind === 'permission') return it.cardState === 'resolved';
    return true;
  });
}

function groupItems(items: DisplayItem[]): GroupedItem[] {
  const result: GroupedItem[] = [];
  let i = 0;
  while (i < items.length) {
    if (isToolGroupable(items[i])) {
      const start = i;
      while (i < items.length && isToolGroupable(items[i])) {
        i++;
      }
      const group = items.slice(start, i);
      if (group.length === 1) {
        result.push({ kind: 'single', item: group[0] });
      } else {
        result.push({ kind: 'tool-group', items: group, allResolved: isGroupResolved(group) });
      }
    } else {
      result.push({ kind: 'single', item: items[i] });
      i++;
    }
  }
  return result;
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

  const handlePermissionAllowForSession = () => {
    dispatch({ type: 'PERMISSION_ALLOW_SESSION' });
    postToHost({ type: 'permissionResponse', allowed: true, allowForSession: true });
  };

  const handlePermissionDeny = () => {
    dispatch({ type: 'PERMISSION_DENY' });
    postToHost({ type: 'permissionResponse', allowed: false });
  };

  const handleAskUserSubmit = (answer: string) => {
    dispatch({ type: 'ASK_USER_SUBMIT', answer });
    postToHost({ type: 'permissionResponse', allowed: false, reason: answer });
  };

  const handleAskUserDeny = () => {
    dispatch({ type: 'ASK_USER_DENY' });
    postToHost({ type: 'permissionResponse', allowed: false, reason: 'User dismissed the question' });
  };

  const handleSelectSession = (id: string) => {
    dispatch({ type: 'SELECT_SESSION', id });
    postToHost({ type: 'switchSession', sessionId: id });
  };

  const handleNewSession = () => {
    dispatch({ type: 'NEW_SESSION' });
    postToHost({ type: 'newSession' });
  };

  const handleLoadPipeline = () => {
    dispatch({ type: 'LOAD_PIPELINE' });
    postToHost({ type: 'loadPipeline' });
  };

  const handleOpenGraph = () => {
    postToHost({ type: 'openPipelineGraph' });
  };

  const hasPendingHitl = state.items.some((it) => it.kind === 'hitl' && it.cardState === 'pending');
  const hasPendingPermission = state.items.some((it) => it.kind === 'permission' && it.cardState === 'pending');
  const hasPendingAskUser = state.items.some((it) => it.kind === 'ask-user-question' && it.cardState === 'pending');
  const inputDisabled = hasPendingHitl || hasPendingPermission || hasPendingAskUser;

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
        <PipelineBar
          displayName={state.activePipeline?.displayName ?? null}
          onLoad={handleLoadPipeline}
          onOpenGraph={handleOpenGraph}
        />
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
          {(() => {
            const renderItem = (item: DisplayItem): React.ReactNode => {
              switch (item.kind) {
                case 'user-message':
                  return <ChatMessage role="user" content={item.text} timestamp={item.timestamp} />;
                case 'assistant-stream':
                  return <ChatMessage role="assistant" content={item.text} streaming={item.streaming} timestamp={item.timestamp} />;
                case 'thinking':
                  return <ThinkingBlock text={item.text} />;
                case 'tool-call':
                  return <ToolCallCard data={item.data} />;
                case 'hitl':
                  return (
                    <HitlCard
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
                      displayName={item.displayName}
                      displayDetail={item.displayDetail}
                      cardState={item.cardState}
                      resolvedAllowed={item.resolvedAllowed}
                      onAllow={handlePermissionAllow}
                      onAllowForSession={handlePermissionAllowForSession}
                      onDeny={handlePermissionDeny}
                    />
                  );
                case 'ask-user-question':
                  return (
                    <AskUserQuestionCard
                      questions={item.questions}
                      cardState={item.cardState}
                      resolvedAnswer={item.resolvedAnswer}
                      onSubmit={handleAskUserSubmit}
                      onDeny={handleAskUserDeny}
                    />
                  );
                case 'error':
                  return (
                    <div className="error-message">
                      <span className="error-message-icon codicon codicon-error" />
                      <span>{item.message}</span>
                    </div>
                  );
              }
            };
            return groupItems(state.items).map((grouped) => {
              if (grouped.kind === 'single') {
                return <ErrorBoundary key={grouped.item.id}>{renderItem(grouped.item)}</ErrorBoundary>;
              }
              return (
                <ErrorBoundary key={`group-${grouped.items[0].id}`}>
                  <ToolCallGroup
                    items={grouped.items}
                    allResolved={grouped.allResolved}
                    renderItem={renderItem}
                  />
                </ErrorBoundary>
              );
            });
          })()}
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
            : hasPendingAskUser ? 'Waiting for your answer…'
            : undefined
          }
        />
      </div>
    </>
  );
};
