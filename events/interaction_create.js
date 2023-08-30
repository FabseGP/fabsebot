const { Events } = require('discord.js');

module.exports = {
  name: Events.InteractionCreate,
  async execute(interaction) {
    if (!interaction.isChatInputCommand()) return;
      const command = interaction.client.commands.get(interaction.commandName);
      if (!command) {
        console.error(`No command matching ${interaction.commandName} was found.`);
        return;
      }
      if (interaction.commandName === 'riny') {
        await interaction.reply('we hate rin-rin');
        await interaction.followUp('fr, useless rice cooker');
      }
      else if (interaction.commandName === 'ping') {
        await interaction.reply('Pong!');
        await interaction.followUp('Pong again!');
      }
      try {
        await command.execute(interaction.client, interaction);
      }
      catch (error) {
        console.error(`Error executing ${interaction.commandName}`);
        console.error(error);
      }
    },
};
