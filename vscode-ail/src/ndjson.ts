/**
 * Line-buffered NDJSON parser.
 *
 * Takes a Node Readable stream (ail stdout) and emits typed AilEvent objects.
 * Handles partial lines, empty lines, and malformed JSON gracefully.
 */

import { Readable } from "stream";
import { AilEvent } from "./types";

export type EventHandler = (event: AilEvent) => void;
export type ErrorHandler = (err: Error) => void;

/**
 * Attach NDJSON parsing to a Readable stream.
 * Calls `onEvent` for each successfully parsed event.
 * Calls `onError` for stream errors (not parse errors — those are logged and skipped).
 */
export function parseNdjsonStream(
  stream: Readable,
  onEvent: EventHandler,
  onError?: ErrorHandler
): void {
  let buffer = "";

  stream.setEncoding("utf-8");

  stream.on("data", (chunk: string) => {
    buffer += chunk;
    const lines = buffer.split("\n");
    // Keep the last (potentially incomplete) line in the buffer
    buffer = lines.pop() ?? "";

    for (const line of lines) {
      const trimmed = line.trim();
      if (!trimmed) continue;

      try {
        const event = JSON.parse(trimmed) as AilEvent;
        onEvent(event);
      } catch {
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
        const event = JSON.parse(trimmed) as AilEvent;
        onEvent(event);
      } catch {
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
