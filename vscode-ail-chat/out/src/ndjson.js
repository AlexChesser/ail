"use strict";
/**
 * Line-buffered NDJSON parser.
 *
 * Takes a Node Readable stream (ail stdout) and emits typed AilEvent objects.
 * Handles partial lines, empty lines, and malformed JSON gracefully.
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.parseNdjsonStream = parseNdjsonStream;
/**
 * Attach NDJSON parsing to a Readable stream.
 * Calls `onEvent` for each successfully parsed event.
 * Calls `onError` for stream errors (not parse errors — those are logged and skipped).
 */
function parseNdjsonStream(stream, onEvent, onError) {
    let buffer = "";
    stream.setEncoding("utf-8");
    stream.on("data", (chunk) => {
        buffer += chunk;
        const lines = buffer.split("\n");
        // Keep the last (potentially incomplete) line in the buffer
        buffer = lines.pop() ?? "";
        for (const line of lines) {
            const trimmed = line.trim();
            if (!trimmed)
                continue;
            try {
                const event = JSON.parse(trimmed);
                onEvent(event);
            }
            catch {
                // Malformed JSON — log to console and skip. Never crash.
                console.warn(`[ail] Skipping malformed NDJSON line: ${trimmed}`);
            }
        }
    });
    stream.on("end", () => {
        // Flush any remaining buffered content
        const trimmed = buffer.trim();
        if (trimmed) {
            try {
                const event = JSON.parse(trimmed);
                onEvent(event);
            }
            catch {
                console.warn(`[ail] Skipping malformed NDJSON at EOF: ${trimmed}`);
            }
        }
        buffer = "";
    });
    stream.on("error", (err) => {
        console.error(`[ail] Stream error: ${err.message}`);
        onError?.(err);
    });
}
//# sourceMappingURL=ndjson.js.map