use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use anyhow::Result;

pub struct FileWatcher {
    path: PathBuf,
    last_mtime: SystemTime,
}

impl FileWatcher {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let mtime = std::fs::metadata(&path)?.modified()?;
        
        Ok(Self { path, last_mtime: mtime })
    }

    /// Poll for changes. Returns Ok(()) if changed, or errors if check fails.
    /// Calls should be made in a loop with a sleep.
    pub fn check_for_change(&mut self) -> Result<bool> {
        let mtime = std::fs::metadata(&self.path)?.modified()?;
        if mtime != self.last_mtime {
            self.last_mtime = mtime;
            return Ok(true);
        }
        Ok(false)
    }

    /// Helper to wait in a loop (blocking)
    pub fn wait_for_change(&mut self, poll_interval_ms: u64) -> Result<()> {
        let interval = Duration::from_millis(poll_interval_ms);
        loop {
            if self.check_for_change()? {
                // Debounce: wait a bit more and check again
                std::thread::sleep(Duration::from_millis(300));
                let _ = self.check_for_change();
                return Ok(());
            }
            std::thread::sleep(interval);
        }
    }
}
