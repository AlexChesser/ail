"use strict";
/**
 * Tests for the NDJSON stream parser.
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
const stream_1 = require("stream");
const ndjson_1 = require("../src/ndjson");
function streamFrom(chunks) {
    const r = new stream_1.Readable({ read() { } });
    for (const c of chunks)
        r.push(c);
    r.push(null);
    return r;
}
function collect(chunks) {
    return new Promise((resolve) => {
        const events = [];
        const r = streamFrom(chunks);
        (0, ndjson_1.parseNdjsonStream)(r, (e) => events.push(e));
        r.on('end', () => resolve(events));
    });
}
(0, vitest_1.describe)('parseNdjsonStream', () => {
    (0, vitest_1.it)('parses a single valid event', async () => {
        const events = await collect(['{"type":"run_started","run_id":"abc","pipeline_source":null,"total_steps":1}\n']);
        (0, vitest_1.expect)(events).toHaveLength(1);
        (0, vitest_1.expect)(events[0].type).toBe('run_started');
    });
    (0, vitest_1.it)('parses multiple events in one chunk', async () => {
        const events = await collect([
            '{"type":"run_started","run_id":"x","pipeline_source":null,"total_steps":2}\n' +
                '{"type":"pipeline_completed","outcome":"completed"}\n',
        ]);
        (0, vitest_1.expect)(events).toHaveLength(2);
        (0, vitest_1.expect)(events[0].type).toBe('run_started');
        (0, vitest_1.expect)(events[1].type).toBe('pipeline_completed');
    });
    (0, vitest_1.it)('handles partial lines split across chunks', async () => {
        const events = await collect([
            '{"type":"run_sta',
            'rted","run_id":"abc","pipeline_source":null,"total_steps":0}\n',
        ]);
        (0, vitest_1.expect)(events).toHaveLength(1);
        (0, vitest_1.expect)(events[0].type).toBe('run_started');
    });
    (0, vitest_1.it)('skips empty lines', async () => {
        const events = await collect(['\n\n{"type":"pipeline_completed","outcome":"completed"}\n\n']);
        (0, vitest_1.expect)(events).toHaveLength(1);
    });
    (0, vitest_1.it)('skips malformed JSON without throwing', async () => {
        const events = await collect(['not-json\n{"type":"pipeline_completed","outcome":"completed"}\n']);
        (0, vitest_1.expect)(events).toHaveLength(1);
        (0, vitest_1.expect)(events[0].type).toBe('pipeline_completed');
    });
    (0, vitest_1.it)('parses all events from simple-prompt fixture', async () => {
        const fs = await Promise.resolve().then(() => __importStar(require('fs')));
        const path = await Promise.resolve().then(() => __importStar(require('path')));
        const fixturePath = path.join(__dirname, 'fixtures', 'simple-prompt.ndjson');
        const content = fs.readFileSync(fixturePath, 'utf-8');
        const events = await collect([content]);
        (0, vitest_1.expect)(events.length).toBeGreaterThan(0);
        (0, vitest_1.expect)(events[0].type).toBe('run_started');
        (0, vitest_1.expect)(events[events.length - 1].type).toBe('pipeline_completed');
    });
    (0, vitest_1.it)('parses multi-step fixture and emits correct step sequence', async () => {
        const fs = await Promise.resolve().then(() => __importStar(require('fs')));
        const path = await Promise.resolve().then(() => __importStar(require('path')));
        const fixturePath = path.join(__dirname, 'fixtures', 'multi-step-pipeline.ndjson');
        const content = fs.readFileSync(fixturePath, 'utf-8');
        const events = await collect([content]);
        const stepStarted = events.filter((e) => e.type === 'step_started');
        const stepCompleted = events.filter((e) => e.type === 'step_completed');
        (0, vitest_1.expect)(stepStarted).toHaveLength(3);
        (0, vitest_1.expect)(stepCompleted).toHaveLength(3);
    });
    (0, vitest_1.it)('parses malformed-lines fixture: skips 2 bad lines, still gets valid events', async () => {
        const fs = await Promise.resolve().then(() => __importStar(require('fs')));
        const path = await Promise.resolve().then(() => __importStar(require('path')));
        const fixturePath = path.join(__dirname, 'fixtures', 'malformed-lines.ndjson');
        const content = fs.readFileSync(fixturePath, 'utf-8');
        const events = await collect([content]);
        // 4 valid lines: run_started, step_started, runner_event(stream_delta), step_completed, pipeline_completed
        (0, vitest_1.expect)(events.filter((e) => e.type === 'run_started')).toHaveLength(1);
        (0, vitest_1.expect)(events.filter((e) => e.type === 'pipeline_completed')).toHaveLength(1);
    });
});
//# sourceMappingURL=ndjson.test.js.map