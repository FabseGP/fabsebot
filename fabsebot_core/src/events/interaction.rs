use std::borrow::Cow;

use anyhow::Result as AResult;
use serenity::all::{
	ButtonStyle, ComponentInteraction, Context as SContext, CreateActionRow, CreateButton,
	CreateComponent, CreateContainer, CreateContainerComponent, CreateInputText,
	CreateInteractionResponse, CreateInteractionResponseMessage, CreateLabel, CreateModal,
	CreateModalComponent, CreateTextDisplay, ExecuteWebhook, GuildId, InputText, InputTextStyle,
	Label, LabelComponent, MessageFlags, ModalComponent, ModalInteraction, Webhook,
};

use crate::config::types::utils_config;

pub const FEEDBACK_BUTTON_CUSTOM_ID: &str = "feedback-modal-button";
pub const FEEDBACK_MODAL_CUSTOM_ID: &str = "feedback-modal";
const FEEDBACK_FREEFORM_CUSTOM_ID: &str = "feedback-modal-freeform";

pub fn build_feedback_action_row<'a>() -> CreateContainerComponent<'a> {
	CreateContainerComponent::ActionRow(CreateActionRow::Buttons(Cow::Owned(vec![
		CreateButton::new(FEEDBACK_BUTTON_CUSTOM_ID)
			.label(format!("Give feedback on {}", &utils_config().bot_name))
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
		.and_then(|c| c.value.clone())
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

	webhook
		.execute(
			&ctx.http,
			false,
			ExecuteWebhook::default()
				.with_components(true)
				.flags(MessageFlags::IS_COMPONENTS_V2)
				.components(&[
					CreateComponent::Container(CreateContainer::new(&[
						CreateContainerComponent::TextDisplay(CreateTextDisplay::new(format!(
							"# New feedback received\nAuthor ID: {}\nGuild ID: {}",
							interaction.user.id.get(),
							guild_id.get()
						))),
					])),
					CreateComponent::Container(CreateContainer::new(&[
						CreateContainerComponent::TextDisplay(CreateTextDisplay::new(user_text)),
					])),
				]),
		)
		.await?;

	Ok(())
}

pub async fn handle_feedback_modal_button(
	ctx: &SContext,
	interaction: &ComponentInteraction,
) -> AResult<()> {
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
		.await?;

	Ok(())
}
