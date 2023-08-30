const { SlashCommandBuilder } = require('discord.js');

module.exports = {
  data: new SlashCommandBuilder()
    .setName('star_platinum')
    .setDescription('Replies to Star Platinum'),
    async execute(client, interaction) {
      await interaction.reply('ZAA WARUDOOOOO');
    },
};

