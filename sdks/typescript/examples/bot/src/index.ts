import { once } from "node:events";
import { Client } from "@corrosion/client";
import {
  ApplicationCommandOptionType,
  ChannelType,
  Client as DiscordClient,
  InteractionContextType,
  type ChatInputApplicationCommandData,
  type ChatInputCommandInteraction,
} from "discord.js";
import { VoiceBridge } from "@corrosion/client/djs";

type Command = ChatInputApplicationCommandData & {
  execute(this: void, interaction: ChatInputCommandInteraction<"cached">): Promise<unknown>;
};

const token = process.env.DISCORD_TOKEN;
if (!token) throw new Error("Set DISCORD_TOKEN in your environment or .env file");

const bot = new DiscordClient({
  intents: ["Guilds", "GuildVoiceStates"],
  allowedMentions: { parse: [] },
});
bot.on("error", console.error);
const stopped = Promise.withResolvers<void>();
process.once("SIGINT", () => stopped.resolve());
process.once("SIGTERM", () => stopped.resolve());
let audio: Client | undefined;
let voice: VoiceBridge | undefined;

try {
  await Promise.all([once(bot, "clientReady"), bot.login(token)]);
  if (!bot.isReady()) throw new Error("Discord did not become ready");
  const corrosionToken = process.env.CORROSION_TOKEN;
  const client = await Client.connect({
    url: process.env.CORROSION_URL ?? "http://localhost:24192",
    applicationId: bot.user.id,
    ...(corrosionToken ? { token: corrosionToken } : {}),
  });
  audio = client;
  const bridge = new VoiceBridge(client);
  voice = bridge;
  client.on("connect", () => console.log("Corrosion event stream connected"));
  client.on("disconnect", console.error);
  client.on("error", console.error);
  for (const event of [
    "player.changed",
    "player.destroyed",
    "track.started",
    "track.ended",
  ] as const) {
    client.on(event, (data) => console.log(event, data));
  }

  const commands: Command[] = [
    {
      name: "play",
      description: "Join your voice channel and queue an audio URL",
      options: [
        {
          type: ApplicationCommandOptionType.String,
          name: "url",
          description: "An HTTP(S) audio URL",
          required: true,
        },
      ],
      async execute({ guild, user, options }) {
        const member = await guild.members.fetch(user.id);
        const channel = member.voice.channel;
        if (channel?.type !== ChannelType.GuildVoice) throw new Error("Join a voice channel first");
        await bridge.join(guild, channel.id);
        return client.enqueue(guild.id, {
          resolver: "direct",
          input: options.getString("url", true).trim(),
        });
      },
    },
    {
      name: "pause",
      description: "Pause playback",
      execute: ({ guildId }) => client.setPaused(guildId, true),
    },
    {
      name: "resume",
      description: "Resume playback",
      execute: ({ guildId }) => client.setPaused(guildId, false),
    },
    {
      name: "skip",
      description: "Skip the current track",
      async execute({ guildId }) {
        const { current } = await client.getPlayer(guildId);
        if (!current) throw new Error("Nothing is playing");
        return client.skip(guildId, current.id);
      },
    },
    {
      name: "queue",
      description: "Show the current track and queue",
      execute: ({ guildId }) => client.getPlayer(guildId),
    },
    {
      name: "stop",
      description: "Stop playback and leave voice",
      execute: ({ guild }) => bridge.leave(guild),
    },
  ];

  bot.on("interactionCreate", async (interaction) => {
    if (!interaction.isChatInputCommand() || !interaction.inCachedGuild()) return;
    const command = commands.find(({ name }) => name === interaction.commandName);
    if (!command) return;
    try {
      await interaction.deferReply();
      await interaction.editReply(formatResponse(await command.execute(interaction)));
    } catch (error) {
      console.error(error);
      if (interaction.deferred) {
        await interaction.editReply(String(error).slice(0, 2_000)).catch(console.error);
      }
    }
  });

  await bot.application.commands.set(
    commands.map(({ execute: _execute, ...definition }) => ({
      ...definition,
      contexts: [InteractionContextType.Guild],
    })),
  );
  console.log(`Ready as ${bot.user.tag}. Join a voice channel, then use /play.`);
  await stopped.promise;
} finally {
  await voice?.close().catch(console.error);
  await audio?.close().catch(console.error);
  await bot.destroy();
}

function formatResponse(value: unknown) {
  const json = JSON.stringify(value ?? "OK", null, 2).replaceAll("`", "\\u0060");
  const content = `\`\`\`json\n${json}\n\`\`\``;
  return content.length <= 2_000
    ? { content }
    : { files: [{ attachment: Buffer.from(json), name: "response.json" }] };
}
