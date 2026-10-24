import type { Client } from "@corrosion/client";
import type { VoiceBridge } from "@corrosion/client/djs";
import {
  ApplicationCommandOptionType,
  ChannelType,
  escapeMarkdown,
  type ChatInputApplicationCommandData,
  type ChatInputCommandInteraction,
} from "discord.js";

type Command = Omit<ChatInputApplicationCommandData, "name"> & {
  execute: (
    interaction: ChatInputCommandInteraction<"cached">,
    context: { audio: Client; voice: VoiceBridge },
  ) => Promise<string>;
};

export const commands: Record<string, Command> = {
  play: {
    description: "Join your voice channel and queue an audio URL",
    options: [
      {
        type: ApplicationCommandOptionType.String,
        name: "url",
        description: "An HTTP(S) audio URL",
        required: true,
      },
    ],
    async execute(interaction, { audio, voice }) {
      const { guild, user, options } = interaction;
      const member = await guild.members.fetch(user.id);
      const channel = member.voice.channel;
      if (channel?.type !== ChannelType.GuildVoice) return "Join a voice channel first.";

      await voice.join(guild, channel.id);
      await audio.enqueue(guild.id, {
        resolver: "direct",
        input: options.getString("url", true).trim(),
      });
      return `Track queued in <#${channel.id}>.`;
    },
  },
  queue: {
    description: "Show the current track and queue",
    async execute(interaction, { audio }) {
      const { current, queue, playback } = await audio.getPlayer(interaction.guildId);
      const status = playback === "paused" ? "Paused" : "Current track";
      const lines = [
        current
          ? `**${status}:** ${escapeMarkdown(current.input.slice(0, 80))}`
          : "Nothing is playing.",
        ...queue
          .slice(0, 10)
          .map(({ input }, index) => `${index + 1}. ${escapeMarkdown(input.slice(0, 80))}`),
      ];
      if (!queue.length) lines.push("The queue is empty.");
      if (queue.length > 10) lines.push(`…and ${queue.length - 10} more.`);
      return lines.join("\n");
    },
  },
  pause: {
    description: "Pause playback",
    async execute(interaction, { audio }) {
      await audio.setPaused(interaction.guildId, true);
      return "Playback paused.";
    },
  },
  resume: {
    description: "Resume playback",
    async execute(interaction, { audio }) {
      await audio.setPaused(interaction.guildId, false);
      return "Playback resumed.";
    },
  },
  skip: {
    description: "Skip the current track",
    async execute(interaction, { audio }) {
      const { current } = await audio.getPlayer(interaction.guildId);
      if (!current) return "Nothing is playing.";

      await audio.skip(interaction.guildId, current.id);
      return "Skipped the current track.";
    },
  },
  stop: {
    description: "Stop playback and leave voice",
    async execute(interaction, { voice }) {
      await voice.leave(interaction.guild);
      return "Playback stopped. Left the voice channel.";
    },
  },
};
