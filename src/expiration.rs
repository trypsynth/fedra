use rsntp::SntpClient;

const XOR_MASK: u64 = 0xDEADBEEFCAFEBABE;

const fn date_to_timestamp(year: u32, month: u32, day: u32) -> u64 {
	let mut days: u64 = 0;
	let mut y = 1970;
	while y < year {
		let leap = (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0);
		days += if leap { 366 } else { 365 };
		y += 1;
	}
	let is_leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
	let mut m = 1;
	while m < month {
		days += match m {
			1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
			4 | 6 | 9 | 11 => 30,
			2 => {
				if is_leap { 29 } else { 28 }
			}
			_ => 0,
		};
		m += 1;
	}
	days += (day - 1) as u64;
	days * 86400
}

const fn obfuscate(timestamp: u64) -> u64 {
	timestamp ^ XOR_MASK
}

const BETA_EXPIRATION_XOR: u64 = obfuscate(date_to_timestamp(2026, 3, 1));

const NTP_SERVER: &str = "time.google.com:123";

#[inline(never)]
fn get_expiration_date() -> u64 {
	BETA_EXPIRATION_XOR ^ XOR_MASK
}

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
	if current_time > get_expiration_date() {
		return Err("This beta version of Fedra has expired. Please download a newer version to continue using the application.".to_string());
	}
	Ok(())
}
