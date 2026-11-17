//! Instance metadata lookup.

use anyhow::Result;

use crate::mastodon::{
	InstanceInfo, Language, MastodonClient, PollLimits,
	instance::{InstanceResponse, InstanceV2Response, media_description_limit},
};

impl MastodonClient {
	pub fn get_instance_info(&self) -> Result<InstanceInfo> {
		let v2_config = self
			.base_url
			.join("api/v2/instance")
			.ok()
			.and_then(|url| Self::send_json::<InstanceV2Response>(self.http.get(url), "fetch instance v2 info").ok())
			.and_then(|info| info.configuration);

		let url = self.base_url.join("api/v1/instance")?;
		let info: InstanceResponse = Self::send_json(self.http.get(url), "fetch instance info")?;
		let max_chars =
			info.configuration.as_ref().and_then(|c| c.statuses.as_ref()).and_then(|s| s.max_characters).unwrap_or(500)
				as usize;
		let max_media_description_chars = media_description_limit(v2_config.as_ref(), info.configuration.as_ref());
		let poll_limits =
			info.configuration.as_ref().and_then(|c| c.polls.as_ref()).map(PollLimits::from_config).unwrap_or_default();
		let streaming_url = info.urls.and_then(|u| u.streaming_api);
		let mut languages: Vec<Language> = self
			.base_url
			.join("api/v1/instance/languages")
			.ok()
			.and_then(|url| Self::send_json(self.http.get(url), "fetch the instance's languages").ok())
			.unwrap_or_default();
		languages.sort_by_cached_key(|language| language.name.to_lowercase());
		Ok(InstanceInfo {
			max_post_chars: max_chars,
			max_media_description_chars,
			poll_limits,
			streaming_url,
			languages,
		})
	}

	/// How many custom profile fields the instance allows, if it says.
	///
	/// Mastodon 4.6 and later report it on the v2 instance endpoint; Pleroma
	/// and Akkoma report it on v1. Anything else, including older Mastodon,
	/// reports nothing.
	pub fn get_max_profile_fields(&self) -> Option<usize> {
		let from_v2 = self
			.base_url
			.join("api/v2/instance")
			.ok()
			.and_then(|url| Self::send_json::<InstanceV2Response>(self.http.get(url), "fetch instance info").ok())
			.and_then(|info| info.configuration?.accounts?.max_profile_fields);
		let max = from_v2.or_else(|| {
			let url = self.base_url.join("api/v1/instance").ok()?;
			let info: InstanceResponse = Self::send_json(self.http.get(url), "fetch instance info").ok()?;
			info.pleroma?.metadata?.fields_limits?.max_fields
		})?;
		usize::try_from(max).ok()
	}
}
