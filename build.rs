use std::{env, fs, path::Path, process::Command};

fn main() {
    // Re-run when metadata files change
    println!("cargo:rerun-if-changed=release.txt");
    println!("cargo:rerun-if-changed=site.txt");

    compile_terminfo();

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

/// Compile the bundled `casterm` terminfo source with `tic` and embed the
/// resulting compiled entry (via `include_bytes!` in `src/support/terminfo.rs`)
/// so the release binary can advertise itself without any host-side install
/// step. If `tic` is unavailable, an empty placeholder is written instead and
/// the runtime falls back to `TERM=xterm-256color`.
fn compile_terminfo() {
    let src = "assets/terminfo/casterm.terminfo";
    println!("cargo:rerun-if-changed={src}");

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR set by cargo");
    let compiled_dir = Path::new(&out_dir).join("terminfo_build");
    let dest = Path::new(&out_dir).join("casterm.terminfo.bin");

    let compiled = Command::new("tic")
        .args(["-x", "-o"])
        .arg(&compiled_dir)
        .arg(src)
        .status()
        .ok()
        .filter(|status| status.success())
        .and_then(|_| fs::read(compiled_dir.join("c").join("casterm")).ok());

    match compiled {
        Some(bytes) => {
            fs::write(&dest, bytes).expect("write compiled terminfo entry to OUT_DIR");
        }
        None => {
            println!(
                "cargo:warning=tic not available or compilation failed; casterm terminfo entry \
                 will not be embedded, runtime falls back to TERM=xterm-256color"
            );
            fs::write(&dest, []).expect("write empty terminfo placeholder to OUT_DIR");
        }
    }
}
