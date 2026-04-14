pub mod error;
pub mod mcp;
pub mod middleware;
pub mod routes;

pub type DeploymentImpl = local_deployment::LocalDeployment;

/// Waits for shutdown signals (Ctrl+C or SIGTERM on Unix).
pub async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(e) = tokio::signal::ctrl_c().await {
            tracing::error!("Failed to install Ctrl+C handler: {e}");
        }
    };

    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};

        let terminate = async {
            if let Ok(mut sigterm) = signal(SignalKind::terminate()) {
                sigterm.recv().await;
            } else {
                tracing::error!("Failed to install SIGTERM handler");
                std::future::pending::<()>().await;
            }
        };

        tokio::select! {
            _ = ctrl_c => {},
            _ = terminate => {},
        }
    }

    #[cfg(not(unix))]
    {
        ctrl_c.await;
    }
}

/// Performs cleanup actions on server shutdown.
pub async fn perform_cleanup_actions(deployment: &DeploymentImpl) {
    use deployment::Deployment;
    use services::services::container::ContainerService;
    deployment
        .container()
        .kill_all_running_processes()
        .await
        .expect("Failed to cleanly kill running execution processes");
}

#[cfg(test)]
pub(crate) struct TestDbLock {
    inner: std::sync::Mutex<()>,
}

#[cfg(test)]
pub(crate) struct TestDbLockGuard<'a> {
    _inner: std::sync::MutexGuard<'a, ()>,
    lock_dir: std::path::PathBuf,
}

#[cfg(test)]
impl TestDbLock {
    fn new() -> Self {
        Self {
            inner: std::sync::Mutex::new(()),
        }
    }

    pub(crate) fn lock(&self) -> std::io::Result<TestDbLockGuard<'_>> {
        let inner = self
            .inner
            .lock()
            .map_err(|_| std::io::Error::other("test DB mutex poisoned"))?;
        let lock_dir = utils::assets::asset_dir().join("test-db.lock");

        loop {
            match std::fs::create_dir(&lock_dir) {
                Ok(()) => {
                    return Ok(TestDbLockGuard {
                        _inner: inner,
                        lock_dir,
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                Err(error) => return Err(error),
            }
        }
    }
}

#[cfg(test)]
impl Drop for TestDbLockGuard<'_> {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir(&self.lock_dir);
    }
}

#[cfg(test)]
pub(crate) static TEST_DB_LOCK: std::sync::LazyLock<TestDbLock> =
    std::sync::LazyLock::new(TestDbLock::new);
