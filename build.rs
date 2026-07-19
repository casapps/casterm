use std::{fs, path::Path};

fn main() {
    // Re-run when metadata files change
    println!("cargo:rerun-if-changed=release.txt");
    println!("cargo:rerun-if-changed=site.txt");

    // Version from release.txt
    if Path::new("release.txt").exists() {
        let version = fs::read_to_string("release.txt").unwrap();
        println!("cargo:rustc-env=APP_VERSION={}", version.trim());
    }

    // Official site from site.txt
    if Path::new("site.txt").exists() {
        let site = fs::read_to_string("site.txt").unwrap();
        println!("cargo:rustc-env=APP_OFFICIAL_SITE={}", site.trim());
    }

    // Build metadata via vergen
    if let Err(e) = vergen_gitcl::Emitter::default()
        .add_instructions(&vergen_gitcl::BuildBuilder::all_build().unwrap())
        .and_then(|e| e.add_instructions(&vergen_gitcl::CargoBuilder::all_cargo().unwrap()))
        .and_then(|e| e.add_instructions(&vergen_gitcl::RustcBuilder::all_rustc().unwrap()))
        .and_then(|e| e.add_instructions(&vergen_gitcl::GitclBuilder::all_git().unwrap()))
        .and_then(|e| e.emit())
    {
        eprintln!("vergen error: {e}");
    }
}
