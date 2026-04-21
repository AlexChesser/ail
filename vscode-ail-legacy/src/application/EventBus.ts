/**
 * EventBus — typed pub/sub for RunnerEvents.
 *
 * Subscribers register per event type and receive strongly-typed events.
 * All handlers are called synchronously in registration order.
 */

import { RunnerEvent } from './events';

type Handler<T> = (event: T) => void;

export interface Disposable {
  dispose(): void;
}

export class EventBus {
  private readonly _handlers = new Map<string, Set<Handler<RunnerEvent>>>();

  /** Emit an event to all registered handlers for its type. */
  emit<T extends RunnerEvent>(event: T): void {
    const set = this._handlers.get(event.type);
    if (set) {
      for (const handler of set) {
        handler(event);
      }
    }
  }

  /**
   * Subscribe to events of a specific type.
   * Returns a Disposable that removes the handler.
   */
  on<K extends RunnerEvent['type']>(
    type: K,
    handler: Handler<Extract<RunnerEvent, { type: K }>>,
  ): Disposable {
    if (!this._handlers.has(type)) {
      this._handlers.set(type, new Set());
    }
    // Cast: safe because we only emit events of the registered type
    const typedHandler = handler as Handler<RunnerEvent>;
    this._handlers.get(type)!.add(typedHandler);

    return {
      dispose: () => {
        this._handlers.get(type)?.delete(typedHandler);
      },
    };
  }
}
