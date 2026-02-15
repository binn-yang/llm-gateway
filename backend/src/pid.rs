use anyhow::{bail, Context, Result};
use fs2::FileExt;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use tracing::{info, warn};

/// PID file manager with exclusive locking
#[derive(Debug)]
pub struct PidFile {
    path: PathBuf,
    file: File,
}

impl PidFile {
    /// Create and lock a PID file
    ///
    /// This will:
    /// 1. Create the PID file if it doesn't exist
    /// 2. Attempt to acquire an exclusive lock
    /// 3. Check for stale PIDs if lock initially fails
    /// 4. Write the current process ID to the file
    pub fn create(path: Option<PathBuf>) -> Result<Self> {
        let path = path.unwrap_or_else(Self::default_pid_path);

        // Create parent directory if needed
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create PID file directory: {:?}", parent))?;
        }

        // Open file with read/write access
        // Note: We intentionally don't use .truncate(true) here because we need to
        // read the existing PID if the lock fails (to detect stale processes).
        // Truncation happens manually after acquiring the lock via set_len(0).
        #[allow(clippy::suspicious_open_options)]
        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&path)
            .with_context(|| format!("Failed to open PID file: {:?}", path))?;

        // Try to acquire exclusive lock
        match file.try_lock_exclusive() {
            Ok(_) => {
                info!("Acquired PID file lock: {:?}", path);
            }
            Err(_) => {
                // Lock failed - check if it's a stale PID
                match Self::read_pid_from_file(&mut file) {
                    Ok(old_pid) => {
                        if Self::is_process_running(old_pid) {
                            bail!(
                                "Gateway already running (PID: {}). Use 'gateway stop' first.",
                                old_pid
                            );
                        } else {
                            warn!(
                                "Removing stale PID file (old PID: {} is not running)",
                                old_pid
                            );
                            // Stale PID - try lock again
                            file.lock_exclusive().with_context(|| {
                                "Failed to acquire lock even after detecting stale PID"
                            })?;
                        }
                    }
                    Err(e) => {
                        bail!("Failed to read PID from locked file: {}", e);
                    }
                }
            }
        }

        // Write current PID
        let pid = std::process::id();
        file.set_len(0)?;
        file.seek(SeekFrom::Start(0))?;
        writeln!(file, "{}", pid)?;
        file.flush()?;

        info!("PID file created with PID: {}", pid);

        Ok(PidFile { path, file })
    }

    /// Read PID from an existing PID file (for stop/reload commands)
    pub fn read(path: Option<PathBuf>) -> Result<u32> {
        let path = path.unwrap_or_else(Self::default_pid_path);

        if !path.exists() {
            bail!("PID file not found: {:?}. Is the gateway running?", path);
        }

        let mut file = File::open(&path)
            .with_context(|| format!("Failed to open PID file: {:?}", path))?;

        Self::read_pid_from_file(&mut file)
    }

    /// Read PID from a file handle
    fn read_pid_from_file(file: &mut File) -> Result<u32> {
        file.seek(SeekFrom::Start(0))?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;

        contents
            .trim()
            .parse::<u32>()
            .with_context(|| format!("Invalid PID in file: '{}'", contents.trim()))
    }

    /// Check if a process with the given PID is running
    #[cfg(unix)]
    fn is_process_running(pid: u32) -> bool {
        use nix::sys::signal::{kill, Signal};
        use nix::unistd::Pid;

        // Use kill with signal 0 to check if process exists
        // This doesn't actually send a signal, just checks permissions
        match kill(Pid::from_raw(pid as i32), Signal::SIGCONT) {
            Ok(_) => true,
            Err(nix::errno::Errno::ESRCH) => false, // No such process
            Err(nix::errno::Errno::EPERM) => true,  // Process exists but no permission
            Err(_) => false,
        }
    }

    /// Windows implementation (placeholder - not fully supported)
    #[cfg(not(unix))]
    fn is_process_running(_pid: u32) -> bool {
        warn!("Process detection not implemented for this platform");
        false
    }

    /// Determine default PID file path
    ///
    /// Tries in order:
    /// 1. /var/run/llm-gateway.pid (if writable)
    /// 2. ./run/llm-gateway.pid (current directory)
    /// 3. ./llm-gateway.pid (fallback)
    fn default_pid_path() -> PathBuf {
        let candidates = vec![
            PathBuf::from("/var/run/llm-gateway.pid"),
            PathBuf::from("./run/llm-gateway.pid"),
            PathBuf::from("./llm-gateway.pid"),
        ];

        for path in candidates {
            // Check if parent directory is writable
            if let Some(parent) = path.parent() {
                if parent.exists() {
                    // Try to create a test file
                    let test_path = parent.join(".write_test");
                    if std::fs::File::create(&test_path).is_ok() {
                        let _ = std::fs::remove_file(&test_path);
                        return path;
                    }
                } else if std::fs::create_dir_all(parent).is_ok() {
                    return path;
                }
            }
        }

        // Final fallback
        PathBuf::from("./llm-gateway.pid")
    }

    /// Get the path of this PID file
    #[allow(dead_code)]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for PidFile {
    fn drop(&mut self) {
        // Unlock the file
        if let Err(e) = self.file.unlock() {
            warn!("Failed to unlock PID file: {}", e);
        }

        // Remove the PID file
        if let Err(e) = std::fs::remove_file(&self.path) {
            warn!("Failed to remove PID file {:?}: {}", self.path, e);
        } else {
            info!("PID file removed: {:?}", self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_create_pid_file() {
        let temp_dir = std::env::temp_dir();
        let pid_path = temp_dir.join("test_gateway.pid");

        // Clean up any existing file
        let _ = fs::remove_file(&pid_path);

        let pid_file = PidFile::create(Some(pid_path.clone())).unwrap();
        assert!(pid_path.exists());

        // Read back the PID
        let contents = fs::read_to_string(&pid_path).unwrap();
        let written_pid: u32 = contents.trim().parse().unwrap();
        assert_eq!(written_pid, std::process::id());

        // Cleanup happens via Drop
        drop(pid_file);
        assert!(!pid_path.exists());
    }

    #[test]
    fn test_read_pid_file() {
        let temp_dir = std::env::temp_dir();
        let pid_path = temp_dir.join("test_read_gateway.pid");

        // Create a PID file with a known PID
        fs::write(&pid_path, "12345\n").unwrap();

        let pid = PidFile::read(Some(pid_path.clone())).unwrap();
        assert_eq!(pid, 12345);

        // Clean up
        let _ = fs::remove_file(&pid_path);
    }

    #[test]
    fn test_read_nonexistent_pid_file() {
        let pid_path = PathBuf::from("/tmp/nonexistent_gateway.pid");
        let result = PidFile::read(Some(pid_path));
        assert!(result.is_err());
    }

    #[test]
    #[cfg(unix)]
    fn test_is_process_running() {
        // Current process should be running
        let current_pid = std::process::id();
        assert!(PidFile::is_process_running(current_pid));

        // PID 1 should exist on Unix systems (init/systemd)
        assert!(PidFile::is_process_running(1));

        // Very high PID unlikely to exist
        assert!(!PidFile::is_process_running(999999));
    }

    #[test]
    fn test_exclusive_lock_prevents_duplicate() {
        let temp_dir = std::env::temp_dir();
        let pid_path = temp_dir.join("test_exclusive_gateway.pid");

        // Clean up any existing file
        let _ = fs::remove_file(&pid_path);

        // Create first PID file
        let _pid_file1 = PidFile::create(Some(pid_path.clone())).unwrap();

        // Attempt to create second PID file should fail
        let result = PidFile::create(Some(pid_path.clone()));
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("already running"));

        // Clean up
        drop(_pid_file1);
    }
}
