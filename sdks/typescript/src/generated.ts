// Automatically generated types. DO NOT EDIT!

export type Resolver = "direct";

export type ResolveInput = { resolver: Resolver; input: string };

export type QueueItem = { id: string; resolver: Resolver; input: string };

export type VoiceParameters = {
  channel_id: string;
  endpoint: string;
  session_id: string;
  token: string;
};

export type VoiceState = "disconnected" | "connecting" | "connected" | "failed";

export type PlaybackState =
  | "idle"
  | "resolving"
  | "loading"
  | "playing"
  | "paused"
  | "waiting_for_voice";

export type PlayerSnapshot = {
  guild_id: string;
  voice: VoiceState;
  playback: PlaybackState;
  current: QueueItem | null;
  queue: Array<QueueItem>;
  last_error: string | null;
};

export type EndReason = "finished" | "skipped" | "failed";
