"use strict";
/**
 * Tests for the App reducer logic.
 *
 * We test the reducer directly without rendering, since all display logic
 * flows through the reducer via useReducer.
 */
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
// @vitest-environment jsdom
const vitest_1 = require("vitest");
// Import the reducer and types from App without JSX
// We re-implement a minimal version of the reducer types here to avoid
// pulling in React rendering in this test — the reducer is a pure function.
// Instead: render a minimal test that exercises the key state transitions
// by importing and calling the reducer directly.
// Since App.tsx is a React module we can still import it; we only call the
// reducer, not render the component.
// We extract the reducer by re-exporting it in a testable way.
// For now, we test the DisplayItem accumulation via rendering.
const react_1 = __importDefault(require("react"));
const react_2 = require("@testing-library/react");
const App_1 = require("../../src/webview/App");
// Stub acquireVsCodeApi globally for jsdom
globalThis['acquireVsCodeApi'] = () => ({
    postMessage: () => { },
    getState: () => null,
    setState: () => { },
});
function postMessage(msg) {
    (0, react_2.act)(() => {
        window.dispatchEvent(new MessageEvent('message', { data: msg }));
    });
}
(0, vitest_1.describe)('App webview', () => {
    (0, vitest_1.it)('renders empty state banner initially', () => {
        (0, react_2.render)(react_1.default.createElement(App_1.App, null));
        (0, vitest_1.expect)(react_2.screen.getByText(/Send a prompt to get started/i)).toBeTruthy();
    });
    (0, vitest_1.it)('shows streamed text after streamDelta messages', () => {
        (0, react_2.render)(react_1.default.createElement(App_1.App, null));
        postMessage({ type: 'runStarted', runId: 'r1', totalSteps: 1 });
        postMessage({ type: 'stepStarted', stepId: 'invocation', stepIndex: 0, totalSteps: 1 });
        postMessage({ type: 'streamDelta', text: 'Hello' });
        postMessage({ type: 'streamDelta', text: ' world' });
        (0, vitest_1.expect)(react_2.screen.getByText('Hello world')).toBeTruthy();
    });
    (0, vitest_1.it)('accumulates multiple stream deltas into one message block', () => {
        (0, react_2.render)(react_1.default.createElement(App_1.App, null));
        postMessage({ type: 'runStarted', runId: 'r1', totalSteps: 1 });
        postMessage({ type: 'streamDelta', text: 'Part 1 ' });
        postMessage({ type: 'streamDelta', text: 'Part 2' });
        // Should be one element with combined text
        (0, vitest_1.expect)(react_2.screen.getByText('Part 1 Part 2')).toBeTruthy();
    });
    (0, vitest_1.it)('shows error message on pipelineError', () => {
        (0, react_2.render)(react_1.default.createElement(App_1.App, null));
        postMessage({ type: 'pipelineError', error: 'Pipeline not found' });
        (0, vitest_1.expect)(react_2.screen.getByText('Pipeline not found')).toBeTruthy();
    });
    (0, vitest_1.it)('shows error message on processError', () => {
        (0, react_2.render)(react_1.default.createElement(App_1.App, null));
        postMessage({ type: 'processError', message: 'spawn failed' });
        (0, vitest_1.expect)(react_2.screen.getByText('spawn failed')).toBeTruthy();
    });
    (0, vitest_1.it)('shows HITL card when hitlGate received', () => {
        (0, react_2.render)(react_1.default.createElement(App_1.App, null));
        postMessage({ type: 'hitlGate', stepId: 'review', message: 'Review required' });
        (0, vitest_1.expect)(react_2.screen.getByText(/Review required/i)).toBeTruthy();
        (0, vitest_1.expect)(react_2.screen.getByText('Approve')).toBeTruthy();
        (0, vitest_1.expect)(react_2.screen.getByText('Reject')).toBeTruthy();
    });
    (0, vitest_1.it)('shows permission card when permissionRequested received', () => {
        (0, react_2.render)(react_1.default.createElement(App_1.App, null));
        postMessage({
            type: 'permissionRequested',
            displayName: 'Delete files',
            displayDetail: 'rm /tmp/foo',
        });
        (0, vitest_1.expect)(react_2.screen.getByText(/Delete files/i)).toBeTruthy();
        (0, vitest_1.expect)(react_2.screen.getByText('Allow')).toBeTruthy();
        (0, vitest_1.expect)(react_2.screen.getByText('Deny')).toBeTruthy();
    });
    (0, vitest_1.it)('shows step progress when steps arrive', () => {
        (0, react_2.render)(react_1.default.createElement(App_1.App, null));
        postMessage({ type: 'runStarted', runId: 'r1', totalSteps: 2 });
        postMessage({ type: 'stepStarted', stepId: 'invocation', stepIndex: 0, totalSteps: 2 });
        postMessage({ type: 'stepStarted', stepId: 'check', stepIndex: 1, totalSteps: 2 });
        // Step IDs should appear in the progress panel
        (0, vitest_1.expect)(react_2.screen.getAllByText('invocation').length).toBeGreaterThan(0);
        (0, vitest_1.expect)(react_2.screen.getAllByText('check').length).toBeGreaterThan(0);
    });
    (0, vitest_1.it)('marks pipeline as done on pipelineCompleted', () => {
        (0, react_2.render)(react_1.default.createElement(App_1.App, null));
        postMessage({ type: 'runStarted', runId: 'r1', totalSteps: 1 });
        postMessage({ type: 'streamDelta', text: 'done' });
        postMessage({ type: 'pipelineCompleted' });
        // Input should no longer show "Running…" placeholder (isRunning = false)
        const textarea = react_2.screen.getByRole('textbox');
        (0, vitest_1.expect)(textarea.disabled).toBe(false);
    });
    (0, vitest_1.it)('disables input while HITL gate is pending', () => {
        (0, react_2.render)(react_1.default.createElement(App_1.App, null));
        postMessage({ type: 'hitlGate', stepId: 'review' });
        const textarea = react_2.screen.getByRole('textbox');
        (0, vitest_1.expect)(textarea.disabled).toBe(true);
    });
    (0, vitest_1.it)('shows thinking block', () => {
        (0, react_2.render)(react_1.default.createElement(App_1.App, null));
        postMessage({ type: 'thinking', text: 'Deep thought here' });
        (0, vitest_1.expect)(react_2.screen.getByText('Thinking')).toBeTruthy();
    });
});
//# sourceMappingURL=app-reducer.test.js.map