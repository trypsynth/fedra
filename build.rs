#![warn(clippy::all, clippy::pedantic, clippy::nursery)]

use std::path::Path;

use shipfitter::{
	build::{configure_file, embed_commit_info, target_profile_dir},
	docs::{Page, convert},
	windows::{VersionInfo, embed_manifest},
};

fn main() {
	println!("cargo:rerun-if-changed=build.rs");
	println!("cargo:rerun-if-changed=Cargo.toml");
	println!("cargo:rerun-if-changed=doc");
	embed_commit_info("FEDRA");
	if let Some(target_dir) = target_profile_dir() {
		build_docs(&target_dir);
		if let Err(e) = configure_file(Path::new("fedra.iss.in"), &target_dir.join("fedra.iss"), &[]) {
			println!("cargo:warning=Failed to configure the installer script: {e}");
		}
	}
	if let Err(e) = embed_manifest("Fedra") {
		println!("cargo:warning=Failed to embed manifest: {e}");
	}
	let version_info = VersionInfo {
		product_name: "Fedra",
		company: "Quin Gillespie",
		copyright: "Copyright © 2026 Quin Gillespie",
		original_filename: "fedra.exe",
		..VersionInfo::default()
	};
	if let Err(e) = version_info.embed() {
		println!("cargo:warning=Failed to embed version info: {e}");
	}
}

fn build_docs(target_dir: &Path) {
	let page = Page { title: "Fedra Documentation", ..Page::default() };
	if let Err(e) = convert(Path::new("doc/readme.md"), &target_dir.join("readme.html"), &page) {
		println!("cargo:warning=Failed to generate the readme: {e}");
	}
}
