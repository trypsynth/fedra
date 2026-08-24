//! Instance metadata lookup.

use anyhow::Result;

use crate::mastodon::{InstanceInfo, MastodonClient, PollLimits, instance::InstanceResponse};

impl MastodonClient {
	pub fn get_instance_info(&self) -> Result<InstanceInfo> {
		let url = self.base_url.join("api/v1/instance")?;
		let info: InstanceResponse = Self::send_json(self.http.get(url), "fetch instance info")?;
		let max_chars =
			info.configuration.as_ref().and_then(|c| c.statuses.as_ref()).and_then(|s| s.max_characters).unwrap_or(500)
				as usize;
		let poll_limits =
			info.configuration.as_ref().and_then(|c| c.polls.as_ref()).map(PollLimits::from_config).unwrap_or_default();
		let streaming_url = info.urls.and_then(|u| u.streaming_api);
		Ok(InstanceInfo { max_post_chars: max_chars, poll_limits, streaming_url })
	}
}
