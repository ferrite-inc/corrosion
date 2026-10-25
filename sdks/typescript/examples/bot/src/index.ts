import { once } from "node:events";

import { Client as CorrosionClient } from "@corrosion/client";
import { VoiceBridge } from "@corrosion/client/djs";
import { Client, Events, InteractionContextType } from "discord.js";

import { commands } from "./commands.ts";

const token = process.env.DISCORD_TOKEN;
if (!token) throw new Error("Set DISCORD_TOKEN in your environment or .env file");

await using client = new Client({
  intents: ["Guilds", "GuildVoiceStates"],
  allowedMentions: { parse: [] },
});
await using resources = new AsyncDisposableStack();

const stopped = Promise.withResolvers<void>();
process.once("SIGINT", () => stopped.resolve());
process.once("SIGTERM", () => stopped.resolve());
client.on(Events.Error, console.error);

await Promise.all([once(client, Events.ClientReady), client.login(token)]);
if (!client.isReady()) throw new Error("Discord did not become ready");

const audio = await CorrosionClient.connect({
  url: "http://localhost:24192",
  applicationId: client.user.id,
});
resources.defer(() => audio.close().catch(console.error));
audio.on("disconnect", console.error);
audio.on("error", console.error);

for (const name of [
  "player_changed",
  "player_destroyed",
  "track_started",
  "track_ended",
] as const) {
  audio.on(name, (event) => console.log(`[corrosion:event] ${name}`, event));
}

const voice = new VoiceBridge(audio);
resources.defer(() => voice.close().catch(console.error));

client.on(Events.InteractionCreate, async (interaction) => {
  if (!interaction.isChatInputCommand() || !interaction.inCachedGuild()) return;

  const command = commands[interaction.commandName];
  if (!command?.execute) return;

  try {
    await interaction.deferReply();
    await interaction.editReply(await command.execute(interaction, { audio, voice }));
  } catch (error) {
    console.error(error);
    if (interaction.deferred) {
      await interaction.editReply("Command failed. Try again.").catch(console.error);
    }
  }
});

await client.application.commands.set(
  Object.entries(commands).map(([name, { execute: _execute, ...data }]) => ({
    name,
    ...data,
    contexts: [InteractionContextType.Guild],
  })),
);
console.log(`Ready as ${client.user.tag}. Join a voice channel, then use /play.`);
await stopped.promise;
