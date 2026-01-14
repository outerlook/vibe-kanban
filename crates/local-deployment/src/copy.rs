use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use anyhow::anyhow;
use globwalk::GlobWalkerBuilder;
use services::services::container::ContainerError;

/// Normalize pattern for cross-platform glob matching (convert backslashes to forward slashes)
fn normalize_pattern(pattern: &str) -> String {
    pattern.replace('\\', "/")
}

/// Create a symlink at `link` pointing to `target`.
/// On Unix, creates a symlink directly. On other platforms, attempts to copy the target.
fn create_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link)
    }
    #[cfg(not(unix))]
    {
        // On non-Unix platforms, copy the target if it exists and is a file
        if let Ok(metadata) = fs::metadata(target) {
            if metadata.is_file() {
                fs::copy(target, link)?;
                return Ok(());
            }
        }
        // For directories or non-existent targets, skip
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "symlinks not supported on this platform",
        ))
    }
}

/// Copy project files from source to target directory based on glob patterns.
/// Skips files that already exist at target with same size.
pub(crate) fn copy_project_files_impl(
    source_dir: &Path,
    target_dir: &Path,
    copy_files: &str,
) -> Result<(), ContainerError> {
    let patterns: Vec<&str> = copy_files
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    // Track files to avoid duplicates
    let mut seen = HashSet::new();

    for pattern in patterns {
        let pattern = normalize_pattern(pattern);
        let pattern_path = source_dir.join(&pattern);

        // Check if it's a file or symlink (use symlink_metadata to not follow symlinks)
        let is_file_or_symlink = pattern_path
            .symlink_metadata()
            .map(|m| m.is_file() || m.is_symlink())
            .unwrap_or(false);

        if is_file_or_symlink {
            if let Err(e) = copy_single_entry(&pattern_path, source_dir, target_dir, &mut seen) {
                tracing::warn!(
                    "Failed to copy {} (from {}): {}",
                    pattern,
                    pattern_path.display(),
                    e
                );
            }
            continue;
        }

        let glob_pattern = if pattern_path.is_dir() {
            // For directories, append /** to match all contents recursively
            format!("{pattern}/**")
        } else {
            pattern.clone()
        };

        let walker = match GlobWalkerBuilder::from_patterns(source_dir, &[&glob_pattern])
            .file_type(globwalk::FileType::FILE | globwalk::FileType::SYMLINK)
            .build()
        {
            Ok(w) => w,
            Err(e) => {
                tracing::warn!("Invalid glob pattern '{glob_pattern}': {e}");
                continue;
            }
        };

        for entry in walker.flatten() {
            if let Err(e) = copy_single_entry(entry.path(), source_dir, target_dir, &mut seen) {
                tracing::warn!("Failed to copy {:?}: {e}", entry.path());
            }
        }
    }

    Ok(())
}

/// Copy a single file or symlink from source to target.
fn copy_single_entry(
    source_path: &Path,
    source_root: &Path,
    target_root: &Path,
    seen: &mut HashSet<PathBuf>,
) -> Result<bool, ContainerError> {
    // Use symlink_metadata to get info about the path itself, not the target
    let metadata = source_path.symlink_metadata()?;
    let is_symlink = metadata.is_symlink();

    // For deduplication and security validation:
    // - For regular files: use canonical path
    // - For symlinks: use the symlink's own path (don't follow it)
    let key_path = if is_symlink {
        source_path.to_path_buf()
    } else {
        source_path.canonicalize()?
    };

    // Validate path is within source_dir
    let canonical_source = source_root.canonicalize()?;
    if is_symlink {
        // For symlinks, ensure the symlink itself is within the source directory
        // Get the parent directory of the symlink and canonicalize that
        if let Some(parent) = source_path.parent() {
            let canonical_parent = parent.canonicalize()?;
            if !canonical_parent.starts_with(&canonical_source) {
                return Err(ContainerError::Other(anyhow!(
                    "Symlink {source_path:?} is outside project directory"
                )));
            }
        }
    } else {
        // For regular files, validate the canonical path
        if !key_path.starts_with(&canonical_source) {
            return Err(ContainerError::Other(anyhow!(
                "File {source_path:?} is outside project directory"
            )));
        }
    }

    if !seen.insert(key_path) {
        return Ok(false);
    }

    let relative_path = source_path.strip_prefix(source_root).map_err(|e| {
        ContainerError::Other(anyhow!(
            "Failed to get relative path for {source_path:?}: {e}"
        ))
    })?;

    let target_path = target_root.join(relative_path);

    // Check if target already exists (use symlink_metadata to detect symlinks too)
    if target_path.symlink_metadata().is_ok() {
        return Ok(false);
    }

    if let Some(parent) = target_path.parent()
        && !parent.exists()
    {
        fs::create_dir_all(parent)?;
    }

    if is_symlink {
        // Read the symlink target and recreate it
        let link_target = fs::read_link(source_path)?;
        if let Err(e) = create_symlink(&link_target, &target_path) {
            tracing::warn!("Failed to create symlink {:?} -> {:?}: {e}", target_path, link_target);
        }
    } else {
        fs::copy(source_path, &target_path)?;
    }

    Ok(true)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;
    #[test]
    fn test_copy_project_files_mixed_patterns() {
        let source_dir = TempDir::new().unwrap();
        let target_dir = TempDir::new().unwrap();

        fs::write(source_dir.path().join(".env"), "secret").unwrap();
        fs::write(source_dir.path().join("config.json"), "{}").unwrap();

        let src_dir = source_dir.path().join("src");
        fs::create_dir(&src_dir).unwrap();
        fs::write(src_dir.join("main.rs"), "code").unwrap();
        fs::write(src_dir.join("lib.rs"), "lib").unwrap();

        let config_dir = source_dir.path().join("config");
        fs::create_dir(&config_dir).unwrap();
        fs::write(config_dir.join("app.toml"), "config").unwrap();

        copy_project_files_impl(
            source_dir.path(),
            target_dir.path(),
            ".env, *.json, src, config",
        )
        .unwrap();

        assert!(target_dir.path().join(".env").exists());
        assert!(target_dir.path().join("config.json").exists());
        assert!(target_dir.path().join("src/main.rs").exists());
        assert!(target_dir.path().join("src/lib.rs").exists());
        assert!(target_dir.path().join("config/app.toml").exists());
    }

    #[test]
    fn test_copy_project_files_nonexistent_pattern_ok() {
        let source_dir = TempDir::new().unwrap();
        let target_dir = TempDir::new().unwrap();

        let result =
            copy_project_files_impl(source_dir.path(), target_dir.path(), "nonexistent.txt");

        assert!(result.is_ok());
        assert!(!target_dir.path().join("nonexistent.txt").exists());
    }

    #[test]
    fn test_copy_project_files_empty_pattern_ok() {
        let source_dir = TempDir::new().unwrap();
        let target_dir = TempDir::new().unwrap();

        let result = copy_project_files_impl(source_dir.path(), target_dir.path(), "");

        assert!(result.is_ok());
        assert_eq!(fs::read_dir(target_dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn test_copy_project_files_whitespace_handling() {
        let source_dir = TempDir::new().unwrap();
        let target_dir = TempDir::new().unwrap();

        fs::write(source_dir.path().join("test.txt"), "content").unwrap();

        copy_project_files_impl(source_dir.path(), target_dir.path(), "  test.txt  ,  ").unwrap();

        assert!(target_dir.path().join("test.txt").exists());
    }

    #[test]
    fn test_copy_project_files_nested_directory() {
        let source_dir = TempDir::new().unwrap();
        let target_dir = TempDir::new().unwrap();

        let config_dir = source_dir.path().join("config");
        fs::create_dir(&config_dir).unwrap();
        fs::write(config_dir.join("app.json"), "{}").unwrap();

        let nested_dir = config_dir.join("nested");
        fs::create_dir(&nested_dir).unwrap();
        fs::write(nested_dir.join("deep.txt"), "deep").unwrap();

        copy_project_files_impl(source_dir.path(), target_dir.path(), "config").unwrap();

        assert!(target_dir.path().join("config/app.json").exists());
        assert!(target_dir.path().join("config/nested/deep.txt").exists());
    }

    #[test]
    fn test_copy_project_files_outside_source_skips_without_copying() {
        let source_dir = TempDir::new().unwrap();
        let target_dir = TempDir::new().unwrap();

        // Create file outside of source directory (one level up)
        let parent_dir = source_dir.path().parent().unwrap().to_path_buf();
        let outside_file = parent_dir.join("secret.txt");
        fs::write(&outside_file, "secret").unwrap();

        // Pattern referencing parent directory should resolve to outside_file and be rejected
        let result = copy_project_files_impl(source_dir.path(), target_dir.path(), "../secret.txt");

        assert!(result.is_ok());
        assert_eq!(fs::read_dir(target_dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn test_copy_project_files_recursive_glob_extension_filter() {
        let source_dir = TempDir::new().unwrap();
        let target_dir = TempDir::new().unwrap();

        // Create nested directory structure with YAML files
        let config_dir = source_dir.path().join("config");
        fs::create_dir(&config_dir).unwrap();
        fs::write(config_dir.join("app.yml"), "app: config").unwrap();
        fs::write(config_dir.join("db.json"), "{}").unwrap();

        let nested_dir = config_dir.join("nested");
        fs::create_dir(&nested_dir).unwrap();
        fs::write(nested_dir.join("settings.yml"), "settings: value").unwrap();
        fs::write(nested_dir.join("other.txt"), "text").unwrap();

        let deep_dir = nested_dir.join("deep");
        fs::create_dir(&deep_dir).unwrap();
        fs::write(deep_dir.join("deep.yml"), "deep: config").unwrap();

        // Copy all YAML files recursively
        copy_project_files_impl(source_dir.path(), target_dir.path(), "config/**/*.yml").unwrap();

        // Verify only YAML files are copied
        assert!(target_dir.path().join("config/app.yml").exists());
        assert!(
            target_dir
                .path()
                .join("config/nested/settings.yml")
                .exists()
        );
        assert!(
            target_dir
                .path()
                .join("config/nested/deep/deep.yml")
                .exists()
        );

        // Verify non-YAML files are not copied
        assert!(!target_dir.path().join("config/db.json").exists());
        assert!(!target_dir.path().join("config/nested/other.txt").exists());
    }

    #[test]
    fn test_copy_project_files_duplicate_patterns_ok() {
        let source_dir = TempDir::new().unwrap();
        let target_dir = TempDir::new().unwrap();

        // Create source files
        let src_dir = source_dir.path().join("src");
        fs::create_dir(&src_dir).unwrap();
        fs::write(src_dir.join("lib.rs"), "lib code").unwrap();
        fs::write(src_dir.join("main.rs"), "main code").unwrap();

        // Copy with overlapping patterns: glob and specific file
        copy_project_files_impl(source_dir.path(), target_dir.path(), "src/*.rs, src/lib.rs")
            .unwrap();

        // Verify file exists once (deduplication works)
        let target_file = target_dir.path().join("src/lib.rs");
        assert!(target_file.exists());
        assert_eq!(fs::read_to_string(target_file).unwrap(), "lib code");

        // Verify other file from glob is also copied
        assert!(target_dir.path().join("src/main.rs").exists());
    }

    #[test]
    fn test_copy_project_files_single_file_path() {
        let source_dir = TempDir::new().unwrap();
        let target_dir = TempDir::new().unwrap();

        // Create source file
        let src_dir = source_dir.path().join("src");
        fs::create_dir(&src_dir).unwrap();
        fs::write(src_dir.join("lib.rs"), "library code").unwrap();

        // Copy single file by exact path (exercises fast path)
        copy_project_files_impl(source_dir.path(), target_dir.path(), "src/lib.rs").unwrap();

        // Verify file is copied
        let target_file = target_dir.path().join("src/lib.rs");
        assert!(target_file.exists());
        assert_eq!(fs::read_to_string(target_file).unwrap(), "library code");
    }

    #[cfg(unix)]
    #[test]
    fn test_symlink_to_file_is_copied() {
        use std::os::unix::fs::symlink;
        let src = TempDir::new().unwrap();
        let dst = TempDir::new().unwrap();

        // Create a real file and a symlink to it
        fs::write(src.path().join("real.txt"), "content").unwrap();
        symlink("real.txt", src.path().join("link.txt")).unwrap();

        copy_project_files_impl(src.path(), dst.path(), "*.txt").unwrap();

        // Both the file and symlink should be copied
        assert!(dst.path().join("real.txt").exists());
        let link_path = dst.path().join("link.txt");
        assert!(link_path.symlink_metadata().unwrap().is_symlink());
        assert_eq!(fs::read_link(&link_path).unwrap().to_str().unwrap(), "real.txt");
        // Reading through the symlink should work
        assert_eq!(fs::read_to_string(&link_path).unwrap(), "content");
    }

    #[cfg(unix)]
    #[test]
    fn test_symlink_to_directory_is_copied() {
        use std::os::unix::fs::symlink;
        let src = TempDir::new().unwrap();
        let dst = TempDir::new().unwrap();

        // Create a directory with a file, and a symlink to the directory
        let data_dir = src.path().join("data");
        fs::create_dir(&data_dir).unwrap();
        fs::write(data_dir.join("file.txt"), "data").unwrap();
        symlink("data", src.path().join("data-link")).unwrap();

        // Copy the symlink directly
        copy_project_files_impl(src.path(), dst.path(), "data-link").unwrap();

        // The symlink should be recreated
        let link_path = dst.path().join("data-link");
        assert!(link_path.symlink_metadata().unwrap().is_symlink());
        assert_eq!(fs::read_link(&link_path).unwrap().to_str().unwrap(), "data");
    }

    #[cfg(unix)]
    #[test]
    fn test_broken_symlink_is_copied() {
        use std::os::unix::fs::symlink;
        let src = TempDir::new().unwrap();
        let dst = TempDir::new().unwrap();

        // Create a symlink to a non-existent target
        symlink("nonexistent.txt", src.path().join("broken.txt")).unwrap();

        copy_project_files_impl(src.path(), dst.path(), "broken.txt").unwrap();

        // The broken symlink should be copied
        let link_path = dst.path().join("broken.txt");
        assert!(link_path.symlink_metadata().unwrap().is_symlink());
        assert_eq!(fs::read_link(&link_path).unwrap().to_str().unwrap(), "nonexistent.txt");
    }

    #[cfg(unix)]
    #[test]
    fn test_symlink_in_directory_glob() {
        use std::os::unix::fs::symlink;
        let src = TempDir::new().unwrap();
        let dst = TempDir::new().unwrap();

        // Create a directory with files and symlinks
        let config_dir = src.path().join("config");
        fs::create_dir(&config_dir).unwrap();
        fs::write(config_dir.join("base.yml"), "base").unwrap();
        symlink("base.yml", config_dir.join("current.yml")).unwrap();

        // Copy the whole directory
        copy_project_files_impl(src.path(), dst.path(), "config").unwrap();

        // Both file and symlink should be copied
        assert!(dst.path().join("config/base.yml").exists());
        let link_path = dst.path().join("config/current.yml");
        assert!(link_path.symlink_metadata().unwrap().is_symlink());
        assert_eq!(fs::read_link(&link_path).unwrap().to_str().unwrap(), "base.yml");
    }
}
