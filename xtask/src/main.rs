#![warn(clippy::all, clippy::pedantic, clippy::nursery)]

use std::{
	env,
	error::Error,
	path::{Path, PathBuf},
};
#[cfg(target_os = "macos")]
use std::{fs, process::Command};

use shipfitter::sign::Minisign;

fn main() -> Result<(), Box<dyn Error>> {
	if env::args().nth(1).as_deref() == Some("release") {
		return release();
	}
	println!("Tasks:");
	println!("	release	Build release binaries and package them");
	Ok(())
}

fn project_root() -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(1).unwrap().to_path_buf()
}

fn release() -> Result<(), Box<dyn Error>> {
	let root = project_root();
	// Load the signing keys before building anything, so a broken secret fails fast.
	let minisign = Minisign::from_env()?;
	let artifacts = package(&root)?;
	if let Some(minisign) = minisign {
		for artifact in &artifacts {
			println!("Signed {}", minisign.sign(artifact)?.display());
		}
	}
	Ok(())
}

/// Builds the release files, returning the ones to publish.
#[cfg(not(target_os = "macos"))]
fn package(root: &Path) -> Result<Vec<PathBuf>, Box<dyn Error>> {
	use shipfitter::{
		host_arch_suffix,
		package::{Zip, cargo_build_release, inno_setup},
	};

	cargo_build_release(root, &[])?;
	let target_dir = root.join("target/release");
	let exe_path = target_dir.join(if cfg!(windows) { "fedra.exe" } else { "fedra" });
	if !exe_path.exists() {
		return Err("Executable not found".into());
	}
	println!("Packaging binaries and docs...");
	let zip_path = target_dir.join(format!("fedra-{}.zip", host_arch_suffix()));
	let mut zip = Zip::create(&zip_path)?;
	zip.file(&exe_path, &exe_path.file_name().unwrap().to_string_lossy())?;
	let readme_path = target_dir.join("readme.html");
	if readme_path.exists() {
		zip.file(&readme_path, "readme.html")?;
	} else {
		println!("Warning: readme.html not found, skipping.");
	}
	zip.finish()?;
	println!("Created zip: {}", zip_path.display());
	let mut artifacts = vec![zip_path];
	if cfg!(windows) && inno_setup(&target_dir.join("fedra.iss"))? {
		println!("Installer created successfully.");
		artifacts.push(target_dir.join(format!("fedra_setup-{}.exe", host_arch_suffix())));
	}
	Ok(artifacts)
}

/// A DMG for each architecture, the one the updater looks for on that Mac.
#[cfg(target_os = "macos")]
fn package(root: &Path) -> Result<Vec<PathBuf>, Box<dyn Error>> {
	use shipfitter::{
		macos::{DeveloperId, Keychain, MacApp, Notary, dmg, sign_bundle},
		package::crate_version,
	};

	const TARGETS: [(&str, &str); 2] = [("aarch64-apple-darwin", "arm64"), ("x86_64-apple-darwin", "x64")];
	let keychain = Keychain::import_from_env()?;
	let signer = keychain.as_ref().map(Keychain::signer).or_else(DeveloperId::from_env);
	let notary = Notary::from_env()?;
	if notary.is_some() && signer.is_none() {
		return Err("notarizing needs a signed app; set MACOS_CERTIFICATE_P12_BASE64 or MACOS_SIGN_IDENTITY".into());
	}
	let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
	let version = crate_version(root, "fedra")?;
	let app = MacApp {
		name: "Fedra",
		identifier: "com.trypsynth.fedra",
		executable: "fedra",
		version: &version,
		..MacApp::default()
	};
	// Every DMG goes in one place, where the workflows pick them up.
	let output_dir = root.join("target/release");
	fs::create_dir_all(&output_dir)?;
	let mut artifacts = Vec::new();
	for (target, arch) in TARGETS {
		run(Command::new(&cargo).current_dir(root).args(["build", "--release", "-p", "fedra", "--target", target]))?;
		let target_dir = root.join("target").join(target).join("release");
		let readme_path = target_dir.join("readme.html");
		let mut resources = Vec::new();
		if readme_path.exists() {
			resources.push(readme_path.as_path());
		} else {
			println!("Warning: readme.html not found, skipping.");
		}
		let bundle = app.bundle(&target_dir, &target_dir.join("fedra"), &[], &resources)?;
		match &signer {
			Some(signer) => sign_bundle(&bundle, &[], signer)?,
			None => println!("No signing certificate, so the app keeps its ad hoc signature."),
		}
		println!("Built app: {}", bundle.display());
		let dmg_path = output_dir.join(format!("fedra-{arch}.dmg"));
		dmg(&bundle, &dmg_path)?;
		println!("Created DMG: {}", dmg_path.display());
		if let Some(notary) = &notary {
			notary.notarize(&dmg_path)?;
		}
		artifacts.push(dmg_path);
	}
	Ok(artifacts)
}

#[cfg(target_os = "macos")]
fn run(command: &mut Command) -> Result<(), Box<dyn Error>> {
	if command.status()?.success() { Ok(()) } else { Err(format!("{command:?} failed").into()) }
}
