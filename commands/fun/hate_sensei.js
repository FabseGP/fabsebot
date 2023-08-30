const { SlashCommandBuilder } = require('discord.js');

module.exports = {
  data: new SlashCommandBuilder()
    .setName('xsensei')
    .setDescription('his role here'),
    async execute(client, interaction) {
      const currentDate = new Date();
      await interaction.reply('we hate sensei');
    },
};
