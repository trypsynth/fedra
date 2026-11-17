//! The time of day as the system is set to show it: 12 or 24 hour, its separator, and its AM and PM, rather than as this
//! code would choose. A hard-coded 12 hour clock was wrong for everyone whose system shows 24.

use chrono::{DateTime, Local};

/// The hour and minute of `when`, without seconds.
pub fn time_of_day(when: DateTime<Local>) -> String {
	system::time_of_day(when).unwrap_or_else(|| fallback(when))
}

/// For a system that won't format the time: a 12 hour clock beats no time at all.
fn fallback(when: DateTime<Local>) -> String {
	when.format("%-I:%M %p").to_string()
}

#[cfg(windows)]
mod system {
	use chrono::{DateTime, Datelike, Local, Timelike};
	use windows::{
		Win32::{
			Foundation::SYSTEMTIME,
			Globalization::{GetTimeFormatEx, TIME_NOSECONDS},
		},
		core::PCWSTR,
	};

	pub fn time_of_day(when: DateTime<Local>) -> Option<String> {
		let system = SYSTEMTIME {
			wYear: u16::try_from(when.year()).ok()?,
			wMonth: u16::try_from(when.month()).ok()?,
			wDayOfWeek: 0,
			wDay: u16::try_from(when.day()).ok()?,
			wHour: u16::try_from(when.hour()).ok()?,
			wMinute: u16::try_from(when.minute()).ok()?,
			wSecond: 0,
			wMilliseconds: 0,
		};
		let mut buffer = [0u16; 128];
		// No locale and no format: the user's own.
		let written = unsafe {
			GetTimeFormatEx(PCWSTR::null(), TIME_NOSECONDS, Some(&raw const system), PCWSTR::null(), Some(&mut buffer))
		};
		// Zero means it failed, and the count includes the terminator.
		let len = usize::try_from(written).ok()?.checked_sub(1)?;
		Some(String::from_utf16_lossy(buffer.get(..len)?))
	}
}

#[cfg(target_os = "macos")]
mod system {
	use chrono::{DateTime, Local};
	use objc2_foundation::{NSDate, NSDateFormatter, NSDateFormatterStyle};

	/// Through the system's own formatter, which knows the 24 hour switch in System Settings as well as the region.
	#[allow(clippy::unnecessary_wraps, reason = "matches the Windows version, which can fail")]
	pub fn time_of_day(when: DateTime<Local>) -> Option<String> {
		#[allow(clippy::cast_precision_loss, reason = "an f64 holds whole seconds exactly for millions of years")]
		let date = NSDate::dateWithTimeIntervalSince1970(when.timestamp() as f64);
		let time = NSDateFormatter::localizedStringFromDate_dateStyle_timeStyle(
			&date,
			NSDateFormatterStyle::NoStyle,
			NSDateFormatterStyle::ShortStyle,
		);
		Some(time.to_string())
	}
}

#[cfg(not(any(windows, target_os = "macos")))]
mod system {
	use chrono::{DateTime, Local};

	pub const fn time_of_day(_: DateTime<Local>) -> Option<String> {
		None
	}
}

#[cfg(test)]
mod tests {
	use chrono::TimeZone;

	use super::*;

	fn at(hour: u32, minute: u32) -> DateTime<Local> {
		Local.with_ymd_and_hms(2026, 9, 17, hour, minute, 0).single().expect("a real local time")
	}

	#[test]
	fn every_hour_formats_with_its_minutes() {
		for hour in 0..24 {
			let time = time_of_day(at(hour, 7));
			assert!(time.contains("07"), "hour {hour} gave {time}");
		}
	}

	#[test]
	fn the_fallback_is_a_12_hour_clock_without_padding() {
		assert_eq!(fallback(at(14, 7)), "2:07 PM");
		assert_eq!(fallback(at(9, 30)), "9:30 AM");
	}
}
