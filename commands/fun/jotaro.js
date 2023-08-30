const { SlashCommandBuilder } = require('discord.js');

module.exports = {
  data: new SlashCommandBuilder()
    .setName('jotaro')
    .setDescription('Replies to Jotaro'),
    async execute(client, interaction) {
      await interaction.reply('Dio');
    },
};