use crate::{ViewError, ViewResult};
use std::path::{Path, PathBuf};

/// A private temporary directory for transient artifacts.
pub struct ScratchDir {
    path: PathBuf,
}

impl ScratchDir {
    pub fn create(label: &str) -> ViewResult<Self> {
        let base = std::env::temp_dir();
        let unique = format!(
            "marqdo-view-{}-{}-{}",
            std::process::id(),
            unique_counter(),
            sanitize_label(label)
        );
        let path = base.join(unique);
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder
            .create(&path)
            .map_err(|err| ViewError::io("create private scratch directory", err))?;
        Ok(Self { path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn file(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn unique_counter() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    seq ^ nanos.rotate_left(17)
}

fn sanitize_label(label: &str) -> String {
    label
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .take(32)
        .collect()
}
