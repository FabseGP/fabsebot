const { SlashCommandBuilder } = require('discord.js');

module.exports = {
  data: new SlashCommandBuilder()
    .setName('user')
    .setDescription('Leak the users private data'),
    async execute(interaction) {
      await interaction.reply(`This command was run by ${interaction.user.username}, who joined on ${interaction.member.joinedAt}`);
    },
};
