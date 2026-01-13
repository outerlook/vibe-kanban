use std::{
    env,
    path::{Path, PathBuf},
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::fs;

const DEFAULT_APP_NAME: &str = "vibe-kanban";

#[derive(Debug, Serialize, Deserialize)]
struct PortFileContent {
    port: u16,
    pid: u32,
    started_at: DateTime<Utc>,
}

pub async fn write_port_file(port: u16) -> std::io::Result<PathBuf> {
    let path = port_file_path(DEFAULT_APP_NAME);
    let dir = path
        .parent()
        .ok_or_else(|| std::io::Error::other("Missing port file dir"))?;
    cleanup_stale_port_file(&path).await;

    tracing::debug!("Writing port {} to {:?}", port, path);
    fs::create_dir_all(dir).await?;

    let content = PortFileContent {
        port,
        pid: std::process::id(),
        started_at: Utc::now(),
    };
    let serialized = serde_json::to_string(&content)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let temp_path = dir.join(format!(
        "{}.{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("vibe-kanban.port"),
        content.pid
    ));
    fs::write(&temp_path, serialized).await?;
    #[cfg(windows)]
    {
        let _ = fs::remove_file(&path).await;
    }
    fs::rename(&temp_path, &path).await?;
    Ok(path)
}

pub async fn read_port_file(app_name: &str) -> std::io::Result<u16> {
    let path = port_file_path(app_name);
    tracing::debug!("Reading port from {:?}", path);

    let content = match read_port_file_content(&path).await {
        Ok(content) => content,
        Err(err) => {
            if err.kind() == std::io::ErrorKind::InvalidData {
                let _ = fs::remove_file(&path).await;
            }
            return Err(err);
        }
    };
    if !is_process_running(content.pid) {
        let _ = fs::remove_file(&path).await;
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Server process no longer running",
        ));
    }

    Ok(content.port)
}

fn port_file_path(app_name: &str) -> PathBuf {
    env::temp_dir()
        .join(app_name)
        .join(format!("{app_name}.port"))
}

async fn read_port_file_content(path: &Path) -> std::io::Result<PortFileContent> {
    let content = fs::read_to_string(path).await?;
    serde_json::from_str(&content)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

async fn cleanup_stale_port_file(path: &Path) {
    match read_port_file_content(path).await {
        Ok(content) => {
            if !is_process_running(content.pid) {
                tracing::debug!("Removing stale port file at {:?}", path);
                let _ = fs::remove_file(path).await;
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::InvalidData => {
            tracing::debug!("Removing invalid port file at {:?}", path);
            let _ = fs::remove_file(path).await;
        }
        Err(_) => {}
    }
}

#[cfg(unix)]
fn is_process_running(pid: u32) -> bool {
    let pid = pid as libc::pid_t;
    let result = unsafe { libc::kill(pid, 0) };
    if result == 0 {
        return true;
    }
    matches!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(code) if code == libc::EPERM
    )
}

#[cfg(windows)]
fn is_process_running(pid: u32) -> bool {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, STILL_ACTIVE},
        System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    };

    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle == 0 {
            return false;
        }
        let mut code: u32 = 0;
        let ok = GetExitCodeProcess(handle, &mut code) != 0;
        CloseHandle(handle);
        ok && code == STILL_ACTIVE
    }
}

#[cfg(not(any(unix, windows)))]
fn is_process_running(_: u32) -> bool {
    true
}
