import { EventEmitter } from "node:events";
import type { EventSource } from "eventsource";
import { subscribe } from "./events.ts";
import type {
  ClientEvents,
  ClientOptions,
  PlayerSnapshot,
  QueueItem,
  ResolveInput,
  VoiceParameters,
} from "./types.ts";

export class Client extends EventEmitter<ClientEvents> {
  readonly sessionId: string;
  private readonly url: URL;
  private readonly headers: Headers;
  private events: EventSource | undefined;
  private closing: Promise<void> | undefined;
  private closed = false;

  private constructor(url: URL, headers: Headers, sessionId: string) {
    super();
    this.url = url;
    this.headers = headers;
    this.sessionId = sessionId;
  }

  static async connect(options: ClientOptions): Promise<Client> {
    const url = new URL(options.url);
    if (
      !["http:", "https:"].includes(url.protocol) ||
      url.username ||
      url.password ||
      url.search ||
      url.hash
    ) {
      throw new TypeError(
        "Server URL must use HTTP(S) without credentials, a query, or a fragment",
      );
    }
    const base = new URL("/api/v1/", url);
    const headers = new Headers();
    if (options.token) headers.set("Authorization", `Bearer ${options.token}`);
    const session = await request<{ id: string }>(base, headers, "POST", "sessions", {
      application_id: options.applicationId,
    });
    if (typeof session.id !== "string" || !session.id) {
      throw new Error("Server returned an invalid session");
    }
    const client = new Client(base, headers, session.id);
    try {
      client.events = await subscribe(
        new URL(`sessions/${encodeURIComponent(session.id)}/events`, base),
        headers,
        client,
      );
      return client;
    } catch (error) {
      try {
        await client.close();
      } catch (cleanupError) {
        throw new AggregateError(
          [error, cleanupError],
          "Failed to connect and clean up Corrosion session",
        );
      }
      throw error;
    }
  }

  listPlayers(): Promise<PlayerSnapshot[]> {
    return this.request("GET", `${this.sessionPath()}/players`);
  }

  getPlayer(guildId: string): Promise<PlayerSnapshot> {
    return this.request("GET", this.playerPath(guildId));
  }

  updateVoice(guildId: string, parameters: VoiceParameters): Promise<void> {
    return this.request("PUT", `${this.playerPath(guildId)}/voice`, parameters);
  }

  enqueue(guildId: string, input: ResolveInput): Promise<QueueItem> {
    return this.request("POST", `${this.playerPath(guildId)}/queue`, input);
  }

  setPaused(guildId: string, paused: boolean): Promise<void> {
    return this.request("PATCH", this.playerPath(guildId), { paused });
  }

  skip(guildId: string, expectedCurrentItemId: string): Promise<void> {
    return this.request("POST", `${this.playerPath(guildId)}/skip`, {
      expected_current_item_id: expectedCurrentItemId,
    });
  }

  destroyPlayer(guildId: string): Promise<void> {
    return this.request("DELETE", this.playerPath(guildId));
  }

  close(): Promise<void> {
    this.closed = true;
    this.events?.close();
    this.closing ??= this.dispose().catch((error: unknown) => {
      this.closing = undefined;
      throw error;
    });
    return this.closing;
  }

  private async dispose(): Promise<void> {
    try {
      await request<void>(this.url, this.headers, "DELETE", this.sessionPath());
    } catch (error) {
      if (!(error instanceof CorrosionError) || error.code !== "session_not_found") throw error;
    }
  }

  private request<T>(method: string, path: string, body?: unknown): Promise<T> {
    if (this.closed) return Promise.reject(new Error("Client is closed"));
    return request<T>(this.url, this.headers, method, path, body);
  }

  private sessionPath(): string {
    return `sessions/${encodeURIComponent(this.sessionId)}`;
  }

  private playerPath(guildId: string): string {
    return `${this.sessionPath()}/players/${encodeURIComponent(guildId)}`;
  }
}

export class CorrosionError extends Error {
  readonly status: number;
  readonly code: string;

  constructor(status: number, code: string) {
    super(`Corrosion returned ${status}: ${code}`);
    this.name = "CorrosionError";
    this.status = status;
    this.code = code;
  }
}

async function request<T>(
  url: URL,
  headers: Headers,
  method: string,
  path: string,
  body?: unknown,
): Promise<T> {
  const requestHeaders = new Headers(headers);
  const init: RequestInit = {
    method,
    headers: requestHeaders,
    signal: AbortSignal.timeout(10_000),
  };
  if (body !== undefined) {
    requestHeaders.set("Content-Type", "application/json");
    init.body = JSON.stringify(body);
  }
  const response = await fetch(new URL(path, url), init);
  if (!response.ok) {
    const body: unknown = await response.json().catch(() => undefined);
    const code =
      typeof body === "object" && body !== null && "error" in body && typeof body.error === "string"
        ? body.error
        : response.statusText;
    throw new CorrosionError(response.status, code);
  }
  if (response.status === 204) return undefined as T;
  return response.json() as Promise<T>;
}
