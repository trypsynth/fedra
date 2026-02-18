use std::{
	env,
	error::Error,
	fmt::{Display, Formatter, Result as FmtResult},
	fs::File,
	io::{Read, Write},
	path::{Path, PathBuf},
	time::Duration,
};

use serde::Deserialize;

const RELEASE_URL: &str = "https://api.github.com/repos/trypsynth/fedra/releases/latest";

#[derive(Debug, Deserialize)]
struct ReleaseAsset {
	name: String,
	browser_download_url: String,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
	tag_name: String,
	body: Option<String>,
	assets: Option<Vec<ReleaseAsset>>,
}

#[derive(Debug)]
pub struct UpdateAvailableResult {
	pub latest_version: String,
	pub download_url: String,
	pub release_notes: String,
}

#[derive(Debug)]
pub enum UpdateCheckOutcome {
	UpdateAvailable(UpdateAvailableResult),
	UpToDate(String),
}

#[derive(Debug)]
pub enum UpdateError {
	InvalidVersion(String),
	HttpError(reqwest::StatusCode),
	NetworkError(String),
	InvalidResponse(String),
	NoDownload(String),
}

impl Display for UpdateError {
	fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
		match self {
			Self::InvalidVersion(msg) => write!(f, "Invalid version: {msg}"),
			Self::HttpError(code) => write!(f, "HTTP error: {code}"),
			Self::NetworkError(msg) => write!(f, "Network error: {msg}"),
			Self::InvalidResponse(msg) => write!(f, "Invalid response: {msg}"),
			Self::NoDownload(msg) => write!(f, "No download: {msg}"),
		}
	}
}

impl Error for UpdateError {}

pub fn download_update_file(url: &str, mut progress_callback: impl FnMut(u64, u64)) -> Result<PathBuf, UpdateError> {
	let client = reqwest::blocking::Client::builder()
		.connect_timeout(Duration::from_secs(30))
		.timeout(Duration::from_secs(600))
		.build()
		.map_err(|e| UpdateError::NetworkError(e.to_string()))?;

	let mut response = client.get(url).send().map_err(|e| UpdateError::NetworkError(e.to_string()))?;

	if !response.status().is_success() {
		return Err(UpdateError::HttpError(response.status()));
	}

	let total_size = response.content_length().unwrap_or(0);
	let fname = url.rsplit('/').next().unwrap_or("update.bin");
	let mut dest_path = if Path::new(fname).extension().is_some_and(|ext| ext.eq_ignore_ascii_case("exe")) {
		env::temp_dir()
	} else if Path::new(fname).extension().is_some_and(|ext| ext.eq_ignore_ascii_case("zip")) {
		env::current_exe()
			.map_err(|e| UpdateError::NoDownload(format!("Failed to determine exe path: {e}")))?
			.parent()
			.ok_or_else(|| UpdateError::NoDownload("Failed to get exe directory".to_string()))?
			.to_path_buf()
	} else {
		env::temp_dir()
	};

	dest_path.push(fname);

	let mut file =
		File::create(&dest_path).map_err(|e| UpdateError::NoDownload(format!("Failed to create file: {e}")))?;

	let mut downloaded: u64 = 0;
	let mut buffer = [0; 8192];
	loop {
		let n = response.read(&mut buffer).map_err(|e| UpdateError::NetworkError(e.to_string()))?;
		if n == 0 {
			break;
		}
		Write::write_all(&mut file, &buffer[..n])
			.map_err(|e| UpdateError::NoDownload(format!("Failed to write to file: {e}")))?;
		downloaded += n as u64;
		progress_callback(downloaded, total_size);
	}

	Ok(dest_path)
}

fn parse_semver_value(value: &str) -> Option<(u64, u64, u64)> {
	let trimmed = value.trim();
	if trimmed.is_empty() {
		return None;
	}
	let normalized = trimmed.trim_start_matches(['v', 'V']);
	let mut parts = normalized.split('.').map(|p| p.split_once('-').map_or(p, |(v, _)| v));
	let major = parts.next()?.parse().ok()?;
	let minor = parts.next().unwrap_or("0").parse().ok()?;
	let patch = parts.next().unwrap_or("0").parse().ok()?;
	Some((major, minor, patch))
}

fn pick_download_url(is_installer: bool, assets: &[ReleaseAsset]) -> Option<String> {
	let preferred_name = if is_installer { "fedra_setup.exe" } else { "fedra.zip" };
	for asset in assets {
		if asset.name.eq_ignore_ascii_case(preferred_name) {
			return Some(asset.browser_download_url.clone());
		}
	}
	None
}

fn fetch_latest_release() -> Result<GithubRelease, UpdateError> {
	let user_agent = format!("fedra/{}", env!("CARGO_PKG_VERSION"));
	let client = reqwest::blocking::Client::builder()
		.timeout(Duration::from_secs(15))
		.user_agent(user_agent)
		.build()
		.map_err(|e| UpdateError::NetworkError(e.to_string()))?;

	let resp = client
		.get(RELEASE_URL)
		.header("Accept", "application/vnd.github+json")
		.send()
		.map_err(|e| UpdateError::NetworkError(e.to_string()))?;

	let status = resp.status();
	if !status.is_success() {
		return Err(UpdateError::HttpError(status));
	}

	resp.json::<GithubRelease>()
		.map_err(|err| UpdateError::InvalidResponse(format!("Failed to parse release JSON: {err}")))
}

fn fetch_release_by_tag(tag: &str) -> Result<GithubRelease, UpdateError> {
	let user_agent = format!("fedra/{}", env!("CARGO_PKG_VERSION"));
	let client = reqwest::blocking::Client::builder()
		.timeout(Duration::from_secs(15))
		.user_agent(user_agent)
		.build()
		.map_err(|e| UpdateError::NetworkError(e.to_string()))?;

	let url = format!("https://api.github.com/repos/trypsynth/fedra/releases/tags/{tag}");
	let resp = client
		.get(&url)
		.header("Accept", "application/vnd.github+json")
		.send()
		.map_err(|e| UpdateError::NetworkError(e.to_string()))?;

	let status = resp.status();
	if !status.is_success() {
		return Err(UpdateError::HttpError(status));
	}

	resp.json::<GithubRelease>()
		.map_err(|err| UpdateError::InvalidResponse(format!("Failed to parse release JSON: {err}")))
}

pub fn check_for_updates(
	current_version: &str,
	current_commit: &str,
	is_installer: bool,
	channel: crate::config::UpdateChannel,
) -> Result<UpdateCheckOutcome, UpdateError> {
	match channel {
		crate::config::UpdateChannel::Stable => check_for_stable_updates(current_version, is_installer),
		crate::config::UpdateChannel::Dev => check_for_dev_updates(current_commit, is_installer),
	}
}

fn check_for_dev_updates(current_commit: &str, is_installer: bool) -> Result<UpdateCheckOutcome, UpdateError> {
	let release = fetch_release_by_tag("latest")?;
	let raw_notes = release.body.unwrap_or_default();
	let commit_lines: Vec<&str> = raw_notes.lines().filter(|line| line.trim().starts_with("- ")).collect();

	if commit_lines.is_empty() {
		let short_local_hash = if current_commit.len() > 7 { &current_commit[..7] } else { current_commit };
		return Ok(UpdateCheckOutcome::UpToDate(format!("dev-{short_local_hash}")));
	}

	let latest_remote_hash = commit_lines.first().and_then(|line| line.split_whitespace().nth(1)).unwrap_or("latest");

	let short_current_commit = if current_commit.len() > 7 { &current_commit[..7] } else { current_commit };
	if short_current_commit == latest_remote_hash {
		return Ok(UpdateCheckOutcome::UpToDate(format!("dev-{latest_remote_hash}")));
	}

	let current_commit_position = commit_lines.iter().position(|line| line.contains(short_current_commit));

	if let Some(position) = current_commit_position {
		if position > 0 {
			let new_notes = commit_lines[..position].join("\n");
			let download_url = match release.assets.as_ref() {
				Some(list) if !list.is_empty() => pick_download_url(is_installer, list).ok_or_else(|| {
					UpdateError::NoDownload("Update is available but no matching download asset was found.".to_string())
				})?,
				_ => {
					return Err(UpdateError::NoDownload(
						"Latest release does not include downloadable assets.".to_string(),
					));
				}
			};
			Ok(UpdateCheckOutcome::UpdateAvailable(UpdateAvailableResult {
				latest_version: format!("dev-{latest_remote_hash}"),
				download_url,
				release_notes: new_notes,
			}))
		} else {
			Ok(UpdateCheckOutcome::UpToDate(format!("dev-{latest_remote_hash}")))
		}
	} else {
		// Commit not in recent history, assume it's old and offer full update.
		let download_url = match release.assets.as_ref() {
			Some(list) if !list.is_empty() => pick_download_url(is_installer, list).ok_or_else(|| {
				UpdateError::NoDownload("Update is available but no matching download asset was found.".to_string())
			})?,
			_ => {
				return Err(UpdateError::NoDownload("Latest release does not include downloadable assets.".to_string()));
			}
		};
		Ok(UpdateCheckOutcome::UpdateAvailable(UpdateAvailableResult {
			latest_version: format!("dev-{latest_remote_hash}"),
			download_url,
			release_notes: raw_notes,
		}))
	}
}

fn check_for_stable_updates(current_version: &str, is_installer: bool) -> Result<UpdateCheckOutcome, UpdateError> {
	let current = parse_semver_value(current_version)
		.ok_or_else(|| UpdateError::InvalidVersion("Current version was not a valid semantic version.".to_string()))?;
	let release = fetch_latest_release()?;
	let latest_semver = parse_semver_value(&release.tag_name).ok_or_else(|| {
		UpdateError::InvalidResponse("Latest release tag does not contain a valid semantic version.".to_string())
	})?;
	if current >= latest_semver {
		return Ok(UpdateCheckOutcome::UpToDate(release.tag_name));
	}
	let download_url = match release.assets.as_ref() {
		Some(list) if !list.is_empty() => pick_download_url(is_installer, list).ok_or_else(|| {
			UpdateError::NoDownload("Update is available but no matching download asset was found.".to_string())
		})?,
		_ => return Err(UpdateError::NoDownload("Latest release does not include downloadable assets.".to_string())),
	};
	Ok(UpdateCheckOutcome::UpdateAvailable(UpdateAvailableResult {
		latest_version: release.tag_name,
		download_url,
		release_notes: release.body.unwrap_or_default(),
	}))
}
