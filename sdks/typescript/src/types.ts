import type { InferOutput } from "valibot";
import type {
  PlayerEventSchema,
  TrackEndedEventSchema,
  TrackStartedEventSchema,
} from "./schemas.ts";

export type {
  EndReason,
  PlaybackState,
  PlayerSnapshot,
  QueueItem,
  ResolveInput,
  Resolver,
  VoiceParameters,
  VoiceState,
} from "./generated.ts";

export interface ClientOptions {
  url: string | URL;
  applicationId: string;
  token?: string;
}

export type PlayerEvent = InferOutput<typeof PlayerEventSchema>;
export type TrackStartedEvent = InferOutput<typeof TrackStartedEventSchema>;
export type TrackEndedEvent = InferOutput<typeof TrackEndedEventSchema>;

export interface ClientEvents {
  connect: [];
  disconnect: [error: Error];
  error: [error: Error];
  player_changed: [event: PlayerEvent];
  player_destroyed: [event: PlayerEvent];
  track_started: [event: TrackStartedEvent];
  track_ended: [event: TrackEndedEvent];
}
