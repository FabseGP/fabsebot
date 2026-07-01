use std::borrow::Cow;

use anyhow::Result as AResult;
use serenity::all::{
	ButtonStyle, ComponentInteraction, Context as SContext, CreateActionRow, CreateButton,
	CreateComponent, CreateContainer, CreateContainerComponent, CreateInputText,
	CreateInteractionResponse, CreateInteractionResponseMessage, CreateLabel, CreateModal,
	CreateModalComponent, CreateTextDisplay, Error, GuildId, InputText, InputTextStyle, Label,
	LabelComponent, ModalComponent, ModalInteraction, Webhook,
};

use crate::{
	config::types::utils_config,
	utils::{helpers::text_display, webhook::webhook_components},
};

pub const FEEDBACK_BUTTON_CUSTOM_ID: &str = "feedback-modal-button";
pub const FEEDBACK_MODAL_CUSTOM_ID: &str = "feedback-modal";
const FEEDBACK_FREEFORM_CUSTOM_ID: &str = "feedback-modal-freeform";

pub fn build_feedback_action_row<'a>() -> CreateContainerComponent<'a> {
	CreateContainerComponent::ActionRow(CreateActionRow::Buttons(Cow::Owned(vec![
		CreateButton::new(FEEDBACK_BUTTON_CUSTOM_ID)
			.label(format!("Give feedback on {}", utils_config().bot_name))
			.style(ButtonStyle::Secondary),
	])))
}

fn modal_component_feedback_field_predicate(c: &ModalComponent) -> Option<&InputText> {
	match c {
		ModalComponent::Label(Label {
			component: LabelComponent::InputText(txt @ InputText { custom_id, .. }),
			..
		}) if custom_id == FEEDBACK_FREEFORM_CUSTOM_ID => Some(txt),
		_ => None,
	}
}

pub async fn handle_feedback_modal_reply(
	ctx: &SContext,
	interaction: &ModalInteraction,
	guild_id: GuildId,
) -> AResult<()> {
	interaction.defer(&ctx.http).await?;
	let Some(user_text) = interaction
		.data
		.components
		.iter()
		.find_map(modal_component_feedback_field_predicate)
		.map(|c| c.value.clone())
	else {
		interaction
			.create_response(
				&ctx.http,
				CreateInteractionResponse::Message(
					CreateInteractionResponseMessage::default()
						.content("Welp Discord tossed away your message :/")
						.ephemeral(true),
				),
			)
			.await?;
		return Ok(());
	};

	let webhook = Webhook::from_url(&ctx.http, &utils_config().feedback_webhook).await?;
	let text = format!(
		"# New feedback received\n**Author ID:** {}\n**Guild ID:** {}\n{user_text}",
		interaction.user.id.get(),
		guild_id.get()
	);
	let components = [CreateComponent::Container(CreateContainer::new(vec![
		text_display(text),
	]))];

	webhook_components(webhook, ctx, &components).await?;

	Ok(())
}

pub async fn handle_feedback_modal_button(
	ctx: &SContext,
	interaction: &ComponentInteraction,
) -> Result<(), Error> {
	let bot_name = &utils_config().bot_name;
	interaction
		.create_response(
			&ctx.http,
			CreateInteractionResponse::Modal(
				CreateModal::new(
					FEEDBACK_MODAL_CUSTOM_ID,
					format!("Give feedback on {bot_name}"),
				)
				.components(&[
					CreateModalComponent::TextDisplay(CreateTextDisplay::new(format!(
						"Please let us know any issues you've had with {bot_name} or any ideas \
						 you have. 50/50 chance it will be implemented (either it will or it \
						 won't) 🫡",
					))),
					CreateModalComponent::Label(CreateLabel::input_text(
						"Give any feedback here (max 3000 characters)",
						CreateInputText::new(
							InputTextStyle::Paragraph,
							FEEDBACK_FREEFORM_CUSTOM_ID,
						)
						.max_length(3000)
						.required(true),
					)),
				]),
			),
		)
		.await
}
