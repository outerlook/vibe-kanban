use std::{env, fs, path::PathBuf};

fn copy_binary(
    workspace_root: &PathBuf,
    binaries_dir: &PathBuf,
    target: &str,
    profile: &str,
    source_name: &str,
    dest_name: &str,
) {
    let is_windows = target.contains("windows");
    let ext = if is_windows { ".exe" } else { "" };

    let dest_binary_name = format!("{}-{}{}", dest_name, target, ext);
    let source_file = format!("{}{}", source_name, ext);

    let source_binary = workspace_root
        .join("target")
        .join(target)
        .join(profile)
        .join(&source_file);

    let source_binary_alt = workspace_root
        .join("target")
        .join(profile)
        .join(&source_file);

    let dest_binary = binaries_dir.join(&dest_binary_name);

    let source = if source_binary.exists() {
        Some(source_binary)
    } else if source_binary_alt.exists() {
        Some(source_binary_alt)
    } else {
        None
    };

    if let Some(src) = source {
        let should_copy = if dest_binary.exists() {
            let src_meta = fs::metadata(&src).ok();
            let dest_meta = fs::metadata(&dest_binary).ok();
            match (src_meta, dest_meta) {
                (Some(s), Some(d)) => s.modified().ok() > d.modified().ok() || s.len() != d.len(),
                _ => true,
            }
        } else {
            true
        };

        if should_copy {
            fs::copy(&src, &dest_binary)
                .unwrap_or_else(|_| panic!("Failed to copy {} binary", source_name));
            println!("cargo:warning=Copied {} to {:?}", source_name, dest_binary);
        }
    } else {
        println!(
            "cargo:warning={} binary not found. Build it first with: cargo build -p server --bin {}",
            source_name, source_name
        );
    }
}

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let tauri_app_dir = out_dir
        .ancestors()
        .find(|p| p.ends_with("tauri-app"))
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()));

    let binaries_dir = tauri_app_dir.join("binaries");
    fs::create_dir_all(&binaries_dir).expect("Failed to create binaries directory");

    let target = env::var("TARGET").unwrap();
    let profile = env::var("PROFILE").unwrap();

    let workspace_root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();

    copy_binary(
        &workspace_root,
        &binaries_dir,
        &target,
        &profile,
        "mcp_task_server",
        "mcp_task_server",
    );

    copy_binary(
        &workspace_root,
        &binaries_dir,
        &target,
        &profile,
        "server",
        "vibe-kanban-server",
    );

    println!("cargo:rerun-if-changed=binaries/");
    if let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") {
        println!(
            "cargo:rerun-if-changed={}",
            PathBuf::from(manifest_dir).join("binaries").display()
        );
    }

    tauri_build::build()
}
