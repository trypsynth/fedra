use rsntp::SntpClient;

const BETA_EXPIRATION_DATE: u64 = 1772323200; // March 1, 2026 00:00:00 UTC
const NTP_SERVER: &str = "time.google.com:123";

fn get_ntp_time() -> Option<u64> {
	let client = SntpClient::new();
	let result = client.synchronize(NTP_SERVER).ok()?;
	let datetime = result.datetime();
	let duration = datetime.unix_timestamp().ok()?;
	Some(duration.as_secs())
}

pub fn check_beta_expiration() -> Result<(), String> {
	let current_time = get_ntp_time().ok_or_else(|| {
		"Unable to verify beta status.\nPlease check your internet connection and try again.".to_string()
	})?;
	if current_time > BETA_EXPIRATION_DATE {
		return Err("This beta version of Fedra has expired. Please download a newer version to continue using the application.".to_string());
	}
	Ok(())
}
