const { SlashCommandBuilder } = require('discord.js');

module.exports = {
  data: new SlashCommandBuilder()
    .setName('fabseman')
    .setDescription('was fabseman here?'),
    async execute(client, interaction) {
      const beatu = client.emojis.cache.find(emoji => emoji.name === "fabseman_willbeatu");
      await interaction.reply(`fabseman was here! ${beatu}`);
    },
};
