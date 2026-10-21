//! Disk-backed stdout/stderr capture; bounded independently of game memory.
use anyhow::{ensure, Result};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};
const CHUNK: u64 = 32 * 1024 * 1024;
const KEEP: usize = 8;
static CAPTURE_ERROR: Mutex<Option<String>> = Mutex::new(None);
static WRITER: OnceLock<Arc<Mutex<Writer>>> = OnceLock::new();
fn stamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}
fn owned(p: &Path) -> bool {
    p.file_name().and_then(|s| s.to_str()).is_some_and(|n| {
        n.starts_with("rebirth-")
            && n.ends_with(".txt")
            && n[8..n.len() - 4]
                .bytes()
                .all(|c| c.is_ascii_digit() || c == b'-')
    })
}
struct Writer {
    root: PathBuf,
    file: Option<File>,
    bytes: u64,
    cap: u64,
    keep: usize,
    session: u128,
    sequence: u64,
    paths: Vec<PathBuf>,
}
impl Writer {
    fn new(root: &Path, cap: u64, keep: usize) -> Result<Self> {
        ensure!(cap > 0 && keep > 0, "invalid log bounds");
        fs::create_dir_all(root)?;
        let mut paths = Vec::new();
        for e in fs::read_dir(root)? {
            let e = e?;
            if e.file_type()?.is_file() && owned(&e.path()) {
                paths.push(e.path());
            }
        }
        paths.sort();
        let mut w = Self {
            root: root.to_owned(),
            file: None,
            bytes: 0,
            cap,
            keep,
            session: stamp(),
            sequence: 0,
            paths,
        };
        w.next()?;
        Ok(w)
    }
    fn next(&mut self) -> Result<()> {
        if let Some(f) = self.file.take() {
            f.sync_data()?;
        }
        let path = self.root.join(format!(
            "rebirth-{:020}-{:06}.txt",
            self.session, self.sequence
        ));
        self.sequence += 1;
        self.file = Some(
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?,
        );
        self.bytes = 0;
        self.paths.push(path);
        while self.paths.len() > self.keep {
            fs::remove_file(&self.paths[0])?;
            self.paths.remove(0);
        }
        Ok(())
    }
    fn append(&mut self, mut b: &[u8]) -> Result<()> {
        while !b.is_empty() {
            if self.bytes == self.cap {
                self.next()?;
            }
            let n = b.len().min((self.cap - self.bytes) as usize);
            self.file.as_mut().unwrap().write_all(&b[..n])?;
            self.bytes += n as u64;
            b = &b[n..];
        }
        Ok(())
    }
    fn snapshot(&self) -> Result<Vec<(File, u64)>> {
        self.paths
            .iter()
            .map(|p| {
                let f = File::open(p)?;
                let n = f.metadata()?.len();
                Ok((f, n))
            })
            .collect()
    }
}
fn export_snapshot(files: Vec<(File, u64)>, destination: &Path) -> Result<()> {
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    writeln!(output,"Project Rebirth — Muguet session log. Retention: latest 8 × 32 MiB across launches; older text may be rotated. Export is a snapshot, not proof of rendering or audio output.\n")?;
    let mut buffer = [0u8; 32768];
    for (mut input, mut left) in files {
        while left > 0 {
            let n = left.min(buffer.len() as u64) as usize;
            input.read_exact(&mut buffer[..n])?;
            output.write_all(&buffer[..n])?;
            left -= n as u64;
        }
    }
    output.sync_all()?;
    Ok(())
}
#[cfg(target_os = "ios")]
pub fn start(root: &Path) -> Result<()> {
    use std::os::fd::{AsRawFd, FromRawFd};
    if WRITER.get().is_some() {
        return Ok(());
    }
    let writer = Arc::new(Mutex::new(Writer::new(root, CHUNK, KEEP)?));
    writer.lock().unwrap().append(
        format!(
            "\n=== Project Rebirth / Muguet / original engine {} / session {} ===\n",
            super::layout::ORIGINAL_SHA256,
            stamp()
        )
        .as_bytes(),
    )?;
    let mut fds = [0; 2];
    ensure!(
        unsafe { libc::pipe(fds.as_mut_ptr()) } == 0,
        "log pipe creation failed"
    );
    let mut input = unsafe { File::from_raw_fd(fds[0]) };
    let output = unsafe { File::from_raw_fd(fds[1]) };
    let sink = writer.clone();
    std::thread::Builder::new()
        .name("rebirth-log".into())
        .spawn(move || {
            let mut b = [0u8; 32768];
            loop {
                let n = match input.read(&mut b) {
                    Ok(0) => break,
                    Ok(n) => n,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(e) => {
                        *CAPTURE_ERROR.lock().unwrap() = Some(e.to_string());
                        break;
                    }
                };
                // Keep draining if disk storage fails: never block the engine behind
                // a stopped pipe reader. Export reports the loss explicitly.
                if CAPTURE_ERROR.lock().unwrap().is_none() {
                    if let Err(e) = sink.lock().unwrap().append(&b[..n]) {
                        *CAPTURE_ERROR.lock().unwrap() = Some(e.to_string());
                    }
                }
            }
        })?;
    ensure!(
        unsafe { libc::dup2(output.as_raw_fd(), 1) } >= 0
            && unsafe { libc::dup2(output.as_raw_fd(), 2) } >= 0,
        "log descriptor redirection failed"
    );
    WRITER
        .set(writer)
        .map_err(|_| anyhow::anyhow!("log capture already active"))?;
    Ok(())
}
pub fn export(root: &Path) -> Result<PathBuf> {
    let files = WRITER
        .get()
        .ok_or_else(|| anyhow::anyhow!("session capture is not active"))?
        .lock()
        .unwrap()
        .snapshot()?;
    fs::create_dir_all(root)?;
    let destination = root.join(format!("rebirth-{}-0.txt", stamp()));
    export_snapshot(files, &destination)?;
    if let Some(error) = CAPTURE_ERROR.lock().unwrap().as_ref() {
        writeln!(
            OpenOptions::new().append(true).open(&destination)?,
            "\nDIAGNOSTIC CAPTURE INCOMPLETE: {error}"
        )?;
    }
    // Only our own generated export names, never arbitrary Files documents.
    let mut prior: Vec<_> = fs::read_dir(root)?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()) && owned(&e.path()))
        .map(|e| e.path())
        .collect();
    prior.sort();
    for old in prior.iter().take(prior.len().saturating_sub(2)) {
        fs::remove_file(old)?;
    }
    Ok(destination)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_rotation_and_stable_snapshot() {
        let root = std::env::temp_dir().join(format!("rebirth-logs-{}", stamp()));
        let mut w = Writer::new(&root, 64, 3).unwrap();
        let data: Vec<u8> = (0..200).map(|i| i as u8).collect();
        w.append(&data).unwrap();
        assert_eq!(w.paths.len(), 3);
        assert_eq!(w.bytes, 8);
        let snapshot = w.snapshot().unwrap();
        let out = root.join("export.txt");
        // Further appends cannot extend a snapshot's captured current-file size.
        w.append(b"later").unwrap();
        export_snapshot(snapshot, &out).unwrap();
        let b = fs::read(&out).unwrap();
        assert!(b.ends_with(&data[64..]));
        assert!(!b.ends_with(b"later"));
        fs::write(root.join("unrelated.txt"), b"keep").unwrap();
        drop(w);
        let w = Writer::new(&root, 64, 3).unwrap();
        assert!(root.join("unrelated.txt").is_file());
        assert!(w.paths.len() <= 3);
        drop(w);
        fs::remove_dir_all(root).unwrap();
    }
}
