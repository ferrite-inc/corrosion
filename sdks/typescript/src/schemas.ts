import * as v from "valibot";
import type { EndReason } from "./generated.ts";

const EndReasonSchema: v.GenericSchema<EndReason> = v.picklist(["finished", "skipped", "failed"]);

export const PlayerEventSchema = v.object({ guild_id: v.string() });

export const TrackStartedEventSchema = v.object({
  ...PlayerEventSchema.entries,
  item_id: v.string(),
});

export const TrackEndedEventSchema = v.object({
  ...TrackStartedEventSchema.entries,
  reason: EndReasonSchema,
  error: v.nullable(v.string()),
});
