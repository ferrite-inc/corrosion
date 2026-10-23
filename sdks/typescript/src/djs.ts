import { CorrosionError, type Client } from "./client.ts";
import {
  GatewayOpcodes,
  type GatewayVoiceServerUpdateDispatchData,
  type GatewayVoiceStateUpdateDispatchData,
  type Guild,
} from "discord.js";

type Voice = {
  guild: Guild;
  channelId: string;
  adapter: ReturnType<Guild["voiceAdapterCreator"]>;
  ready: PromiseWithResolvers<void>;
  state: GatewayVoiceStateUpdateDispatchData | undefined;
  server: GatewayVoiceServerUpdateDispatchData | undefined;
  updates: Promise<void>;
  revision: number;
};

export class VoiceBridge {
  private readonly voices = new Map<string, Voice>();
  private readonly operations = new Map<string, Promise<void>>();
  private stopped = false;

  private readonly audio: Client;

  constructor(audio: Client) {
    this.audio = audio;
  }

  join(guild: Guild, channelId: string): Promise<void> {
    return this.enqueue(guild, () => this.joinGuild(guild, channelId));
  }

  leave(guild: Guild): Promise<void> {
    return this.enqueue(guild, () => this.leaveGuild(guild));
  }

  async close(): Promise<void> {
    this.stopped = true;
    for (const voice of this.voices.values()) {
      voice.ready.reject(new Error("Voice bridge closed"));
    }
    await Promise.allSettled(this.operations.values());
    const results = await Promise.allSettled(
      Array.from(this.voices.values(), ({ guild }) => this.leaveGuild(guild)),
    );
    const errors = results.filter((result) => result.status === "rejected");
    if (errors.length) {
      throw new AggregateError(
        errors.map((result) => result.reason),
        "Voice cleanup failed",
      );
    }
  }

  private enqueue(guild: Guild, operation: () => Promise<void>): Promise<void> {
    const previous = this.operations.get(guild.id) ?? Promise.resolve();
    const result = previous
      .catch(() => undefined)
      .then(() => {
        if (this.stopped) throw new Error("Voice bridge closed");
        return operation();
      });
    this.operations.set(guild.id, result);
    const release = () => {
      if (this.operations.get(guild.id) === result) this.operations.delete(guild.id);
    };
    void result.then(release, release);
    return result;
  }

  private async joinGuild(guild: Guild, channelId: string): Promise<void> {
    const previous = this.voices.get(guild.id);
    if (previous?.channelId === channelId) return previous.ready.promise;
    await previous?.updates;
    previous?.adapter.destroy();
    const ready = Promise.withResolvers<void>();
    void ready.promise.catch(() => undefined);
    const voice: Voice = {
      guild,
      channelId,
      ready,
      state: undefined,
      server: undefined,
      updates: Promise.resolve(),
      revision: 0,
      adapter: guild.voiceAdapterCreator({
        onVoiceStateUpdate: (state) => {
          if (this.voices.get(guild.id) !== voice) return;
          if (state.channel_id !== channelId) {
            if (!voice.state) return;
            ready.reject(new Error("Application left or moved voice channels"));
            void this.leave(guild).catch(console.error);
            return;
          }
          voice.state = state;
          this.forward(voice);
        },
        onVoiceServerUpdate: (server) => {
          if (this.voices.get(guild.id) !== voice) return;
          voice.server = server.endpoint ? server : undefined;
          ++voice.revision;
          this.forward(voice);
        },
        destroy: () => {
          if (this.voices.get(guild.id) !== voice) return;
          ready.reject(new Error("Discord voice gateway disconnected"));
          void this.leave(guild).catch(console.error);
        },
      }),
    };
    this.voices.set(guild.id, voice);
    const timeout = setTimeout(() => ready.reject(new Error("Voice join timed out")), 15_000);
    try {
      if (!voice.adapter.sendPayload(this.payload(guild, channelId))) {
        throw new Error("Discord gateway unavailable");
      }
      await ready.promise;
    } catch (error) {
      await this.leaveGuild(guild).catch(console.error);
      throw error;
    } finally {
      clearTimeout(timeout);
    }
  }

  private forward(voice: Voice): void {
    const { state, server, guild } = voice;
    if (!state || !server?.endpoint) return;
    const revision = ++voice.revision;
    const parameters = {
      channel_id: voice.channelId,
      endpoint: server.endpoint,
      session_id: state.session_id,
      token: server.token,
    };
    voice.updates = voice.updates
      .then(async () => {
        if (this.voices.get(guild.id) !== voice || revision !== voice.revision) return;
        await this.audio.updateVoice(guild.id, parameters);
        if (this.voices.get(guild.id) === voice && revision === voice.revision) {
          voice.ready.resolve();
        }
      })
      .catch((error: unknown) => {
        voice.ready.reject(error);
        console.error(error);
      });
  }

  private async leaveGuild(guild: Guild): Promise<void> {
    const voice = this.voices.get(guild.id);
    this.voices.delete(guild.id);
    voice?.ready.reject(new Error("Voice disconnected"));
    try {
      if (voice) voice.adapter.sendPayload(this.payload(guild, null));
    } finally {
      voice?.adapter.destroy();
      await voice?.updates;
      await this.audio.destroyPlayer(guild.id).catch((error: unknown) => {
        if (!(error instanceof CorrosionError) || error.code !== "player_not_found") throw error;
      });
    }
  }

  private payload(guild: Guild, channelId: string | null) {
    return {
      op: GatewayOpcodes.VoiceStateUpdate,
      d: { guild_id: guild.id, channel_id: channelId, self_mute: false, self_deaf: true },
    };
  }
}
