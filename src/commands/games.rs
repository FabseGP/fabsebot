use crate::types::{
    Context,
    Error,
};

/// Get rekt by an another user in rps
#[poise::command(slash_command, prefix_command)]
pub async fn rps(ctx: Context<'_>, #[description = "Your choice: rock, paper, or scissor"] choice: String) -> Result<(), Error> {
    match choice.as_str() {
        "rock" => {ctx.send(|m| m.content("I choose paper, noob!")).await?;}
        "paper" => {ctx.send(|m| m.content("I choose scissor, noob!")).await?;}
        "scissor" => {ctx.send(|m| m.content("I choose rock, noob!")).await?;}
        _ => {ctx.send(|m| m.content("your lack of intelligence is baffling")).await?;}
    }
    Ok(())
}
                
