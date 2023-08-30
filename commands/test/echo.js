const { SlashCommandBuilder } = require('discord.js');

module.exports = {
  data: new SlashCommandBuilder()
    .setName('echo')
    .setDescription('echo echo echo')
    .addStringOption(option =>
      option.setName('input')
        .setDescription('The input to echo back')
        .setRequired(true)
        .setMaxLength(2000)),
    async execute(client, interaction) {
      const input = interaction.options.getString("input");
      await interaction.send(input);
    },
};
