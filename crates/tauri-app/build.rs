use std::{env, fs, path::PathBuf};

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    // Navigate from OUT_DIR to the tauri-app crate root
    // OUT_DIR is typically: target/<profile>/build/<crate>/out
    let tauri_app_dir = out_dir
        .ancestors()
        .find(|p| p.ends_with("tauri-app"))
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| {
            // Fallback: use CARGO_MANIFEST_DIR
            PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
        });

    let binaries_dir = tauri_app_dir.join("binaries");
    fs::create_dir_all(&binaries_dir).expect("Failed to create binaries directory");

    // Determine target triple and profile
    let target = env::var("TARGET").unwrap();
    let profile = env::var("PROFILE").unwrap();

    // Binary name with platform-specific extension
    let binary_name = if target.contains("windows") {
        format!("mcp_task_server-{}.exe", target)
    } else {
        format!("mcp_task_server-{}", target)
    };

    // Source binary location in cargo target directory
    let workspace_root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();

    let source_binary = workspace_root
        .join("target")
        .join(&target)
        .join(&profile)
        .join(if target.contains("windows") {
            "mcp_task_server.exe"
        } else {
            "mcp_task_server"
        });

    // Also check non-cross-compiled location
    let source_binary_alt =
        workspace_root
            .join("target")
            .join(&profile)
            .join(if target.contains("windows") {
                "mcp_task_server.exe"
            } else {
                "mcp_task_server"
            });

    let dest_binary = binaries_dir.join(&binary_name);

    // Try to copy the binary if it exists
    let source = if source_binary.exists() {
        Some(source_binary)
    } else if source_binary_alt.exists() {
        Some(source_binary_alt)
    } else {
        None
    };

    if let Some(src) = source {
        fs::copy(&src, &dest_binary).expect("Failed to copy mcp_task_server binary");
        println!("cargo:warning=Copied mcp_task_server to {:?}", dest_binary);
    } else {
        // During development, the binary might not exist yet
        // Print a warning but don't fail the build
        println!(
            "cargo:warning=mcp_task_server binary not found. Build it first with: cargo build -p server --bin mcp_task_server"
        );
    }

    // Tell Cargo to rerun this script if the source binary changes
    println!("cargo:rerun-if-changed=binaries/");
    if let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") {
        println!(
            "cargo:rerun-if-changed={}",
            PathBuf::from(manifest_dir).join("binaries").display()
        );
    }

    tauri_build::build()
}
