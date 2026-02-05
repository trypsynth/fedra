use std::{
	env, fs,
	path::{Path, PathBuf},
	process::Command,
};

use embed_manifest::{
	embed_manifest,
	manifest::{ActiveCodePage, DpiAwareness, HeapType, Setting, SupportedOS::*},
	new_manifest,
};
use winres::WindowsResource;

fn main() {
	println!("cargo:rerun-if-changed=build.rs");
	println!("cargo:rerun-if-changed=Cargo.toml");
	println!("cargo:rerun-if-changed=fedra.iss.in");

	build_docs();
	configure_installer();

	let target = env::var("TARGET").unwrap_or_default();
	if target.contains("windows") {
		let manifest = new_manifest("Fedra")
			.supported_os(Windows7..=Windows10)
			.active_code_page(ActiveCodePage::Utf8)
			.heap_type(HeapType::SegmentHeap)
			.dpi_awareness(DpiAwareness::PerMonitorV2)
			.long_path_aware(Setting::Enabled);
		if let Err(e) = embed_manifest(manifest) {
			println!("cargo:warning=Failed to embed manifest: {}", e);
			println!("cargo:warning=The application will still work but may lack optimal Windows theming");
		}
		embed_version_info();
	}
}

fn embed_version_info() {
	let version = env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".to_string());
	// let description = env::var("CARGO_PKG_DESCRIPTION").unwrap_or_default();
	let mut res = WindowsResource::new();
	res.set("ProductName", "Fedra")
		.set("FileDescription", "Fedra")
		.set("LegalCopyright", "Copyright © 2026 Quin Gillespie")
		.set("CompanyName", "Quin Gillespie")
		.set("OriginalFilename", "fedra.exe")
		.set("ProductVersion", &version)
		.set("FileVersion", &version);
	if let Err(e) = res.compile() {
		println!("cargo:warning=Failed to embed version info: {}", e);
	}
}

fn target_profile_dir() -> Option<PathBuf> {
	let profile = env::var("PROFILE").ok()?;
	if let Ok(target_dir) = env::var("CARGO_TARGET_DIR") {
		let mut dir = PathBuf::from(target_dir);
		dir.push(profile);
		return Some(dir);
	}
	let out_dir = PathBuf::from(env::var("OUT_DIR").ok()?);
	out_dir.ancestors().nth(3).map(Path::to_path_buf)
}

fn build_docs() {
	let target_dir = match target_profile_dir() {
		Some(dir) => dir,
		None => {
			println!("cargo:warning=Could not determine target directory for docs.");
			return;
		}
	};
	let doc_dir = PathBuf::from("doc");
	let readme = doc_dir.join("readme.md");
	let config = doc_dir.join("pandoc.yaml");
	println!("cargo:rerun-if-changed={}", readme.display());
	println!("cargo:rerun-if-changed={}", config.display());
	let pandoc_check = Command::new("pandoc").arg("--version").output();
	if pandoc_check.is_err() {
		println!("cargo:warning=Pandoc not found. Documentation will not be generated.");
		return;
	}
	let output = target_dir.join("readme.html");
	let status = Command::new("pandoc")
		.arg(format!("--defaults={}", config.display()))
		.arg(&readme)
		.arg("-o")
		.arg(&output)
		.status();
	match status {
		Ok(s) if s.success() => {}
		_ => println!("cargo:warning=Failed to generate documentation."),
	}
}

fn configure_installer() {
	let target_dir = match target_profile_dir() {
		Some(dir) => dir,
		None => return,
	};
	let input_path = PathBuf::from("fedra.iss.in");
	if !input_path.exists() {
		return;
	}
	let content = match fs::read_to_string(&input_path) {
		Ok(c) => c,
		Err(e) => {
			println!("cargo:warning=Failed to read installer script: {}", e);
			return;
		}
	};
	let version = env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".to_string());
	let new_content = content.replace("@PROJECT_VERSION@", &version);
	let output_path = target_dir.join("fedra.iss");
	if let Err(e) = fs::write(&output_path, new_content) {
		println!("cargo:warning=Failed to write installer script: {}", e);
	}
}
