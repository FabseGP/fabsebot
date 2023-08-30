const { SlashCommandBuilder } = require('discord.js');

module.exports = {
  data: new SlashCommandBuilder()
    .setName('hi')
    .setDescription('Replies to hi'),
    async execute(client, interaction) {
      await interaction.reply('How are you, fine thank you');
    },
};

