use executors::profile::{ExecutorConfigs, ExecutorProfileId};

use crate::error::ApiError;

pub(crate) fn validate_exact_coding_agent_profile(
    executor_profile_id: &ExecutorProfileId,
) -> Result<(), ApiError> {
    if ExecutorConfigs::get_cached()
        .get_coding_agent(executor_profile_id)
        .is_some()
    {
        return Ok(());
    }

    Err(ApiError::BadRequest(format!(
        "Invalid executor profile: {}",
        executor_profile_id
    )))
}
