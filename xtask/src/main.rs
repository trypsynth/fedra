#![warn(clippy::all, clippy::pedantic, clippy::nursery)]

use std::{
	env,
	error::Error,
	path::{Path, PathBuf},
};

use shipfitter::{package::cargo_build_release, sign::Minisign};

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
	cargo_build_release(&root, &[])?;
	let target_dir = root.join("target/release");
	let exe_path = target_dir.join(if cfg!(windows) { "fedra.exe" } else { "fedra" });
	if !exe_path.exists() {
		return Err("Executable not found".into());
	}
	// Load the signing keys before building anything, so a broken secret fails fast.
	let minisign = Minisign::from_env()?;
	println!("Packaging binaries and docs...");
	let artifacts = package(&root, &target_dir, &exe_path)?;
	if let Some(minisign) = minisign {
		for artifact in &artifacts {
			println!("Signed {}", minisign.sign(artifact)?.display());
		}
	}
	Ok(())
}

/// Builds the release files, returning the ones to publish.
#[cfg(not(target_os = "macos"))]
fn package(_root: &Path, target_dir: &Path, exe_path: &Path) -> Result<Vec<PathBuf>, Box<dyn Error>> {
	use shipfitter::{
		host_arch_suffix,
		package::{Zip, inno_setup},
	};

	let zip_path = target_dir.join(format!("fedra-{}.zip", host_arch_suffix()));
	let mut zip = Zip::create(&zip_path)?;
	zip.file(exe_path, &exe_path.file_name().unwrap().to_string_lossy())?;
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

/// Builds the release files, returning the ones to publish.
#[cfg(target_os = "macos")]
fn package(root: &Path, target_dir: &Path, exe_path: &Path) -> Result<Vec<PathBuf>, Box<dyn Error>> {
	use shipfitter::{
		host_arch_suffix,
		macos::{DeveloperId, Keychain, MacApp, Notary, dmg, sign_bundle},
		package::crate_version,
	};

	let keychain = Keychain::import_from_env()?;
	let signer = keychain.as_ref().map(Keychain::signer).or_else(DeveloperId::from_env);
	let notary = Notary::from_env()?;
	if notary.is_some() && signer.is_none() {
		return Err("notarizing needs a signed app; set MACOS_CERTIFICATE_P12_BASE64 or MACOS_SIGN_IDENTITY".into());
	}
	let version = crate_version(root, "fedra")?;
	let app = MacApp {
		name: "Fedra",
		identifier: "com.trypsynth.fedra",
		executable: "fedra",
		version: &version,
		..MacApp::default()
	};
	let readme_path = target_dir.join("readme.html");
	let mut resources = Vec::new();
	if readme_path.exists() {
		resources.push(readme_path.as_path());
	} else {
		println!("Warning: readme.html not found, skipping.");
	}
	let bundle = app.bundle(target_dir, exe_path, &[], &resources)?;
	match &signer {
		Some(signer) => sign_bundle(&bundle, &[], signer)?,
		None => println!("No signing certificate, so the app keeps its ad hoc signature."),
	}
	println!("Built app: {}", bundle.display());
	let dmg_path = target_dir.join(format!("fedra-{}.dmg", host_arch_suffix()));
	dmg(&bundle, &dmg_path)?;
	println!("Created DMG: {}", dmg_path.display());
	if let Some(notary) = notary {
		notary.notarize(&dmg_path)?;
	}
	Ok(vec![dmg_path])
}
