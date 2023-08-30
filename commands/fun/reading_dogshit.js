const { SlashCommandBuilder } = require('discord.js');

module.exports = {
  data: new SlashCommandBuilder()
    .setName('status')
    .setDescription('wise xsensei saying'),
    async execute(client, interaction) {
      const currentDate = new Date();
      const firstDay = Math.floor(Math.random() * 100) + 1;
      const secondDay = firstDay + 1;
      const thirdDay = firstDay + 2;
      const message = `day : ${firstDay} of reading dogshit!\n`
                    + `day : ${secondDay} of reading dogshit!\n`
                    + `day : ${thirdDay} of reading dogshit!\n`;
      await interaction.reply(message);
    },
};
