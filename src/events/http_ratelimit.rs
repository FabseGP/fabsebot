use crate::config::types::Error;
use poise::serenity_prelude::RatelimitInfo;
use tracing::warn;

pub async fn handle_ratelimit(ratelimit_data: &RatelimitInfo) -> Result<(), Error> {
    warn!(
        "http-request of {}-type failed at {}, current timeout at {:?} & current limit at {:?}",
        ratelimit_data.method.reqwest_method().to_string(),
        ratelimit_data.path,
        ratelimit_data.timeout,
        ratelimit_data.limit
    );
    Ok(())
}
