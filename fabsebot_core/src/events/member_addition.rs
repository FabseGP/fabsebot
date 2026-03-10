use std::sync::Arc;

use anyhow::Result as AResult;
use fabsebot_db::guild::insert_user;
use serenity::all::Member;

use crate::config::types::Data;

pub async fn handle_member_addition(data: Arc<Data>, member: &Member) -> AResult<()> {
	insert_user(i64::from(member.user.id), &mut *data.db.acquire().await?).await?;

	Ok(())
}
