use anyhow::Result as AResult;
use serenity::{
	all::{
		ComponentInteraction, Context as SContext, CreateComponent, CreateContainer,
		CreateInputText, CreateInteractionResponse, CreateLabel, CreateModal, CreateModalComponent,
		CreateTextDisplay, Error, GuildId, InputText, InputTextStyle, Label, LabelComponent,
		ModalComponent, ModalInteraction, Webhook,
	},
	http::Http,
};

use crate::{
	config::types::utils_config,
	utils::{helpers::text_display, webhook::webhook_components},
};

pub const FEEDBACK_BUTTON_CUSTOM_ID: &str = "feedback-modal-button";
pub const FEEDBACK_MODAL_CUSTOM_ID: &str = "feedback-modal";
pub const FEEDBACK_FREEFORM_CUSTOM_ID: &str = "feedback-modal-freeform";

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
	http: &Http,
	interaction: &ModalInteraction,
	guild_id: GuildId,
) -> AResult<()> {
	interaction.defer(http).await?;
	let user_text = interaction
		.data
		.components
		.iter()
		.find_map(modal_component_feedback_field_predicate)
		.map(|c| c.value.as_str())
		.unwrap();

	let webhook = Webhook::from_url(http, &utils_config().feedback_webhook).await?;
	let text = format!(
		"# New feedback received\n**Author ID:** {}\n**Guild ID:** {}\n{user_text}",
		interaction.user.id.get(),
		guild_id.get()
	);

	let display = [text_display(text)];

	let components = [CreateComponent::Container(CreateContainer::new(&display))];

	webhook_components(webhook, http, &components).await?;

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
