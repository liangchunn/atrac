use std::ffi::{OsStr, OsString};
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const TEMP_CREATE_ATTEMPTS: u64 = 128;
static TEMP_OUTPUT_COUNTER: AtomicU64 = AtomicU64::new(0);

pub struct PendingOutput {
    path: PathBuf,
    committed: bool,
}

impl PendingOutput {
    pub fn commit(mut self, output: &Path) -> io::Result<()> {
        fs::rename(&self.path, output)?;
        self.committed = true;
        Ok(())
    }

    #[cfg(test)]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for PendingOutput {
    fn drop(&mut self) {
        if !self.committed {
            let _ = fs::remove_file(&self.path);
        }
    }
}

pub fn create_pending_output(
    output: &Path,
    codec_tag: &str,
) -> Result<(File, PendingOutput), String> {
    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    let output_name = output.file_name().unwrap_or_else(|| OsStr::new("output"));
    for _ in 0..TEMP_CREATE_ATTEMPTS {
        let sequence = TEMP_OUTPUT_COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut temp_name = OsString::from(".");
        temp_name.push(output_name);
        temp_name.push(format!(
            ".{codec_tag}-{}-{sequence}.tmp",
            std::process::id()
        ));
        let path = parent.join(temp_name);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => {
                return Ok((
                    file,
                    PendingOutput {
                        path,
                        committed: false,
                    },
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "failed to create temporary output beside `{}`: {error}",
                    output.display()
                ));
            }
        }
    }
    Err(format!(
        "failed to create a unique temporary output beside `{}` after {TEMP_CREATE_ATTEMPTS} attempts",
        output.display()
    ))
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    static NEXT_DIR: AtomicU64 = AtomicU64::new(0);

    fn temp_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "atrac-pending-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn dropping_pending_output_preserves_destination_and_removes_temp() {
        let dir = temp_dir();
        let output = dir.join("output.at3");
        fs::write(&output, b"existing").unwrap();
        let (mut file, pending) = create_pending_output(&output, "test").unwrap();
        let temp = pending.path().to_owned();
        file.write_all(b"partial").unwrap();
        drop(file);
        drop(pending);
        assert_eq!(fs::read(&output).unwrap(), b"existing");
        assert!(!temp.exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn commit_atomically_replaces_destination() {
        let dir = temp_dir();
        let output = dir.join("output.at3");
        fs::write(&output, b"existing").unwrap();
        let (mut file, pending) = create_pending_output(&output, "test").unwrap();
        file.write_all(b"complete").unwrap();
        file.flush().unwrap();
        drop(file);
        pending.commit(&output).unwrap();
        assert_eq!(fs::read(&output).unwrap(), b"complete");
        fs::remove_dir_all(dir).unwrap();
    }
}
