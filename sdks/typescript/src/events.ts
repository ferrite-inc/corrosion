import type { EventEmitter } from "node:events";
import { EventSource } from "eventsource";
import * as v from "valibot";
import { PlayerEventSchema, TrackEndedEventSchema, TrackStartedEventSchema } from "./schemas.ts";
import type { ClientEvents } from "./types.ts";

export function subscribe(
  url: URL,
  headers: Headers,
  emitter: EventEmitter<ClientEvents>,
): Promise<EventSource> {
  return new Promise((resolve, reject) => {
    const source = new EventSource(url, {
      fetch: (input, init) => {
        const requestHeaders = new Headers(init.headers);
        for (const [name, value] of headers) {
          requestHeaders.set(name, value);
        }
        return fetch(input, { ...init, headers: requestHeaders });
      },
    });
    let opened = false;
    const timeout = setTimeout(() => {
      source.close();
      reject(new Error("Timed out connecting to Corrosion events"));
    }, 10_000);

    source.onopen = () => {
      opened = true;
      clearTimeout(timeout);
      resolve(source);
      emitter.emit("connect");
    };
    source.onerror = (event) => {
      const error = new Error(event.message || "Corrosion event stream disconnected");
      if (!opened) {
        clearTimeout(timeout);
        source.close();
        reject(error);
      }
      emitter.emit("disconnect", error);
    };

    listen("player_changed", PlayerEventSchema, (event) => emitter.emit("player_changed", event));
    listen("player_destroyed", PlayerEventSchema, (event) =>
      emitter.emit("player_destroyed", event),
    );
    listen("track_started", TrackStartedEventSchema, (event) =>
      emitter.emit("track_started", event),
    );
    listen("track_ended", TrackEndedEventSchema, (event) => emitter.emit("track_ended", event));

    function listen<T>(
      name: string,
      schema: v.GenericSchema<unknown, T>,
      emit: (value: T) => void,
    ): void {
      source.addEventListener(name, (event) => {
        let payload: T;
        try {
          payload = v.parse(schema, JSON.parse(event.data));
        } catch (cause) {
          if (emitter.listenerCount("error") > 0) {
            emitter.emit("error", new Error(`Invalid ${name} event`, { cause }));
          }
          return;
        }
        emit(payload);
      });
    }
  });
}
