"use strict";
/**
 * Tests for the AilProcessManager event mapping.
 * Tests mapAilEventToMessages (pure function) without spawning any process.
 */
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || (function () {
    var ownKeys = function(o) {
        ownKeys = Object.getOwnPropertyNames || function (o) {
            var ar = [];
            for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
            return ar;
        };
        return ownKeys(o);
    };
    return function (mod) {
        if (mod && mod.__esModule) return mod;
        var result = {};
        if (mod != null) for (var k = ownKeys(mod), i = 0; i < k.length; i++) if (k[i] !== "default") __createBinding(result, mod, k[i]);
        __setModuleDefault(result, mod);
        return result;
    };
})();
Object.defineProperty(exports, "__esModule", { value: true });
const vitest_1 = require("vitest");
const ail_process_manager_1 = require("../src/ail-process-manager");
const fs = __importStar(require("fs"));
const path = __importStar(require("path"));
function loadFixture(name) {
    const content = fs.readFileSync(path.join(__dirname, 'fixtures', name), 'utf-8');
    const events = [];
    // Parse synchronously line by line
    for (const line of content.split('\n')) {
        const trimmed = line.trim();
        if (!trimmed)
            continue;
        try {
            events.push(JSON.parse(trimmed));
        }
        catch {
            // skip malformed
        }
    }
    return events;
}
function fixtureToMessages(name) {
    const events = loadFixture(name);
    return events.flatMap(ail_process_manager_1.mapAilEventToMessages);
}
(0, vitest_1.describe)('mapAilEventToMessages', () => {
    (0, vitest_1.it)('maps run_started → runStarted', () => {
        const event = { type: 'run_started', run_id: 'r1', pipeline_source: null, total_steps: 2 };
        const msgs = (0, ail_process_manager_1.mapAilEventToMessages)(event);
        (0, vitest_1.expect)(msgs).toHaveLength(1);
        (0, vitest_1.expect)(msgs[0]).toEqual({ type: 'runStarted', runId: 'r1', totalSteps: 2 });
    });
    (0, vitest_1.it)('maps step_started → stepStarted', () => {
        const event = {
            type: 'step_started', step_id: 'invocation', step_index: 0, total_steps: 3,
            resolved_prompt: 'hello',
        };
        const msgs = (0, ail_process_manager_1.mapAilEventToMessages)(event);
        (0, vitest_1.expect)(msgs[0]).toMatchObject({ type: 'stepStarted', stepId: 'invocation', stepIndex: 0, totalSteps: 3 });
    });
    (0, vitest_1.it)('maps step_completed → stepCompleted with cost and tokens', () => {
        const event = {
            type: 'step_completed', step_id: 'invocation', cost_usd: 0.0012,
            input_tokens: 10, output_tokens: 8, response: 'hi',
        };
        const msgs = (0, ail_process_manager_1.mapAilEventToMessages)(event);
        (0, vitest_1.expect)(msgs[0]).toEqual({
            type: 'stepCompleted', stepId: 'invocation',
            costUsd: 0.0012, inputTokens: 10, outputTokens: 8,
        });
    });
    (0, vitest_1.it)('maps step_skipped → stepSkipped', () => {
        const event = { type: 'step_skipped', step_id: 'check' };
        (0, vitest_1.expect)((0, ail_process_manager_1.mapAilEventToMessages)(event)).toEqual([{ type: 'stepSkipped', stepId: 'check' }]);
    });
    (0, vitest_1.it)('maps step_failed → stepFailed', () => {
        const event = { type: 'step_failed', step_id: 'check', error: 'oops' };
        (0, vitest_1.expect)((0, ail_process_manager_1.mapAilEventToMessages)(event)).toEqual([{ type: 'stepFailed', stepId: 'check', error: 'oops' }]);
    });
    (0, vitest_1.it)('maps hitl_gate_reached → hitlGate', () => {
        const event = { type: 'hitl_gate_reached', step_id: 'review', message: 'Please confirm' };
        const msgs = (0, ail_process_manager_1.mapAilEventToMessages)(event);
        (0, vitest_1.expect)(msgs[0]).toEqual({ type: 'hitlGate', stepId: 'review', message: 'Please confirm' });
    });
    (0, vitest_1.it)('maps pipeline_completed → pipelineCompleted', () => {
        const event = { type: 'pipeline_completed', outcome: 'completed' };
        (0, vitest_1.expect)((0, ail_process_manager_1.mapAilEventToMessages)(event)).toEqual([{ type: 'pipelineCompleted' }]);
    });
    (0, vitest_1.it)('maps pipeline_error → pipelineError', () => {
        const event = { type: 'pipeline_error', error: 'not found', error_type: 'PIPELINE_NOT_FOUND' };
        (0, vitest_1.expect)((0, ail_process_manager_1.mapAilEventToMessages)(event)).toEqual([{ type: 'pipelineError', error: 'not found' }]);
    });
    (0, vitest_1.it)('maps runner_event/stream_delta → streamDelta', () => {
        const event = { type: 'runner_event', event: { type: 'stream_delta', text: 'hello' } };
        (0, vitest_1.expect)((0, ail_process_manager_1.mapAilEventToMessages)(event)).toEqual([{ type: 'streamDelta', text: 'hello' }]);
    });
    (0, vitest_1.it)('maps runner_event/thinking → thinking', () => {
        const event = { type: 'runner_event', event: { type: 'thinking', text: 'I think...' } };
        (0, vitest_1.expect)((0, ail_process_manager_1.mapAilEventToMessages)(event)).toEqual([{ type: 'thinking', text: 'I think...' }]);
    });
    (0, vitest_1.it)('maps runner_event/tool_use → toolUse', () => {
        const event = {
            type: 'runner_event',
            event: { type: 'tool_use', tool_name: 'bash', tool_use_id: 'tu-1', input: { command: 'ls' } },
        };
        const msgs = (0, ail_process_manager_1.mapAilEventToMessages)(event);
        (0, vitest_1.expect)(msgs[0]).toMatchObject({ type: 'toolUse', toolName: 'bash', toolUseId: 'tu-1' });
        (0, vitest_1.expect)(msgs[0].input).toEqual({ command: 'ls' });
    });
    (0, vitest_1.it)('maps runner_event/tool_result → toolResult', () => {
        const event = {
            type: 'runner_event',
            event: { type: 'tool_result', tool_name: 'bash', tool_use_id: 'tu-1', content: 'ok', is_error: false },
        };
        const msgs = (0, ail_process_manager_1.mapAilEventToMessages)(event);
        (0, vitest_1.expect)(msgs[0]).toEqual({ type: 'toolResult', toolUseId: 'tu-1', content: 'ok', isError: false });
    });
    (0, vitest_1.it)('maps runner_event/permission_requested → permissionRequested', () => {
        const event = {
            type: 'runner_event',
            event: { type: 'permission_requested', display_name: 'Delete files', display_detail: 'rm /tmp/foo' },
        };
        const msgs = (0, ail_process_manager_1.mapAilEventToMessages)(event);
        (0, vitest_1.expect)(msgs[0]).toEqual({
            type: 'permissionRequested', displayName: 'Delete files', displayDetail: 'rm /tmp/foo',
        });
    });
    (0, vitest_1.it)('returns empty array for unknown runner_event subtypes', () => {
        const event = {
            type: 'runner_event',
            event: { type: 'cost_update', cost_usd: 0.001, input_tokens: 5, output_tokens: 3 },
        };
        (0, vitest_1.expect)((0, ail_process_manager_1.mapAilEventToMessages)(event)).toHaveLength(0);
    });
    (0, vitest_1.describe)('fixture round-trips', () => {
        (0, vitest_1.it)('simple-prompt: produces runStarted, streamDelta, stepCompleted, pipelineCompleted', () => {
            const msgs = fixtureToMessages('simple-prompt.ndjson');
            const types = msgs.map((m) => m.type);
            (0, vitest_1.expect)(types).toContain('runStarted');
            (0, vitest_1.expect)(types).toContain('streamDelta');
            (0, vitest_1.expect)(types).toContain('stepCompleted');
            (0, vitest_1.expect)(types).toContain('pipelineCompleted');
        });
        (0, vitest_1.it)('tool-calls: produces toolUse and toolResult messages', () => {
            const msgs = fixtureToMessages('tool-calls.ndjson');
            (0, vitest_1.expect)(msgs.some((m) => m.type === 'toolUse')).toBe(true);
            (0, vitest_1.expect)(msgs.some((m) => m.type === 'toolResult')).toBe(true);
        });
        (0, vitest_1.it)('thinking-blocks: produces thinking messages', () => {
            const msgs = fixtureToMessages('thinking-blocks.ndjson');
            (0, vitest_1.expect)(msgs.some((m) => m.type === 'thinking')).toBe(true);
        });
        (0, vitest_1.it)('hitl-gate: produces hitlGate message', () => {
            const msgs = fixtureToMessages('hitl-gate.ndjson');
            (0, vitest_1.expect)(msgs.some((m) => m.type === 'hitlGate')).toBe(true);
        });
        (0, vitest_1.it)('permission-request: produces permissionRequested message', () => {
            const msgs = fixtureToMessages('permission-request.ndjson');
            (0, vitest_1.expect)(msgs.some((m) => m.type === 'permissionRequested')).toBe(true);
        });
        (0, vitest_1.it)('step-failure: produces stepFailed message', () => {
            const msgs = fixtureToMessages('step-failure.ndjson');
            (0, vitest_1.expect)(msgs.some((m) => m.type === 'stepFailed')).toBe(true);
        });
        (0, vitest_1.it)('pipeline-error: produces pipelineError message', () => {
            const msgs = fixtureToMessages('pipeline-error.ndjson');
            (0, vitest_1.expect)(msgs.some((m) => m.type === 'pipelineError')).toBe(true);
        });
        (0, vitest_1.it)('multi-step: produces 3 stepStarted and 3 stepCompleted messages', () => {
            const msgs = fixtureToMessages('multi-step-pipeline.ndjson');
            (0, vitest_1.expect)(msgs.filter((m) => m.type === 'stepStarted')).toHaveLength(3);
            (0, vitest_1.expect)(msgs.filter((m) => m.type === 'stepCompleted')).toHaveLength(3);
        });
    });
    (0, vitest_1.describe)('CLAUDECODE env removal', () => {
        (0, vitest_1.it)('AilProcessManager does not pass CLAUDECODE to spawned process', () => {
            // We verify the logic statically: the start() method deletes CLAUDECODE.
            // Read the source and confirm the delete is present.
            const source = fs.readFileSync(path.join(__dirname, '..', 'src', 'ail-process-manager.ts'), 'utf-8');
            (0, vitest_1.expect)(source).toContain("delete env['CLAUDECODE']");
        });
    });
});
//# sourceMappingURL=ail-process-manager.test.js.map