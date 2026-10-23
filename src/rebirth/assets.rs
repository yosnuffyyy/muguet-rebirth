//! In-place, read-only adapter for the existing selected web directory.
//! No ZIP import, move, download, save writes, or directory scans.
use anyhow::{bail, ensure, Context, Result};
use std::collections::HashMap;

pub const SCRATCH_BYTES: usize = 64 * 1024;
pub const OPEN_FILES: usize = 8;
pub const MANIFEST_LIMIT: usize = 8 * 1024 * 1024;
#[derive(Debug)]
pub struct Entry {
    pub path: String,
    pub bytes: u64,
}
pub struct Index {
    pub entries: Vec<Entry>,
    exact: HashMap<String, usize>,
    folded: HashMap<String, Option<usize>>,
}
pub fn valid_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 2048
        && path
            .bytes()
            .all(|b| (32..=126).contains(&b) && !b"\\\":\0".contains(&b))
        && path
            .split('/')
            .all(|p| !p.is_empty() && p != "." && p != "..")
}
fn manifest_integer(value: &serde_json::Value) -> Result<u64> {
    let n = value.as_f64().context("non-numeric manifest value")?;
    ensure!(
        n.is_finite() && (0.0..=9_007_199_254_740_991.0).contains(&n) && n.floor() == n,
        "invalid manifest integer"
    );
    Ok(n as u64)
}
impl Index {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        ensure!(
            !bytes.is_empty() && bytes.len() <= MANIFEST_LIMIT,
            "manifest size outside bounds"
        );
        let v: serde_json::Value = serde_json::from_slice(bytes)?;
        ensure!(v["mount"] == "/game/", "wrong game mount");
        let rows = v["files"].as_array().context("missing files array")?;
        ensure!(
            !rows.is_empty() && rows.len() <= 20_000,
            "manifest entry count outside bounds"
        );
        let mut result = Self {
            entries: Vec::with_capacity(rows.len()),
            exact: HashMap::new(),
            folded: HashMap::new(),
        };
        for row in rows {
            let row = row.as_array().context("invalid entry")?;
            ensure!(row.len() == 3, "expected original path/size/metadata tuple");
            let path = row[0].as_str().context("non-string path")?;
            ensure!(valid_path(path), "invalid asset path");
            let size = manifest_integer(&row[1])?;
            manifest_integer(&row[2])?;
            let id = result.entries.len();
            ensure!(
                result.exact.insert(path.into(), id).is_none(),
                "duplicate asset path"
            );
            result
                .folded
                .entry(path.to_ascii_lowercase())
                .and_modify(|v| *v = None)
                .or_insert(Some(id));
            result.entries.push(Entry {
                path: path.into(),
                bytes: size,
            });
        }
        Ok(result)
    }
    /// Exact names take precedence. Ambiguous case-folds fail instead of
    /// selecting another archive. Original manifest order defines numeric IDs.
    pub fn lookup(&self, guest: &str, insensitive: bool) -> Result<usize> {
        let path = guest
            .strip_prefix("/game/")
            .context("path outside game mount")?;
        ensure!(valid_path(path), "invalid guest asset path");
        if let Some(id) = self.exact.get(path) {
            return Ok(*id);
        }
        if insensitive {
            if let Some(id) = self.folded.get(&path.to_ascii_lowercase()) {
                return id.context("ambiguous case-folded path");
            }
        }
        bail!("asset not present in original manifest")
    }
    /// The worker receives numeric manifest IDs. Keep those entries in original
    /// order, releasing the two validation/path-lookup maps after parsing.
    fn into_worker_entries(self) -> Vec<Entry> {
        let retained: usize = self.entries.iter().map(|e| e.path.capacity()).sum();
        let released: usize = self.exact.keys().map(String::capacity).sum::<usize>()
            + self.folded.keys().map(String::capacity).sum::<usize>();
        eprintln!("[io] manifest index: {} original IDs, retained path capacity {retained} bytes; released validation lookup path capacity {released} bytes plus {} map entries; ownership bytes, not process-footprint savings", self.entries.len(), self.exact.len() + self.folded.len());
        self.entries
    }
}
pub fn save_namespace(mode: u32) -> Result<&'static str> {
    match mode {
        1 => Ok("NativeRuntimeProbe"),
        2 => Ok("NativeSandboxProbe"),
        _ => bail!("unknown launch mode"),
    }
}

#[cfg(unix)]
mod unix {
    use super::*;
    use std::{
        ffi::CString,
        fs::File,
        io::{ErrorKind, Read},
        os::fd::{AsRawFd, FromRawFd},
        os::unix::fs::FileExt,
        path::Path,
        sync::atomic::{AtomicBool, Ordering},
    };
    pub struct Reader {
        root: File,
        entries: Vec<Entry>,
        manifest: std::sync::Arc<[u8]>,
        cache: Vec<(usize, File, u64)>,
        scratch: Box<[u8; SCRATCH_BYTES]>,
        stamp: u64,
        pub delivered: u64,
        pub opens: u64,
        pub cache_hits: u64,
    }
    fn case_name(parent: &File, part: &str) -> Result<CString> {
        // Only invoked after ENOENT. Enumerate this one authorized directory,
        // not the game tree; the resulting opened archive is cached by file ID.
        let dot = CString::new(".").unwrap();
        let fd = unsafe {
            libc::openat(
                parent.as_raw_fd(),
                dot.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
            )
        };
        ensure!(fd >= 0, "cannot inspect asset directory");
        let dir = unsafe { libc::fdopendir(fd) };
        if dir.is_null() {
            unsafe { libc::close(fd) };
            bail!("cannot enumerate asset directory");
        }
        struct Directory(*mut libc::DIR);
        impl Drop for Directory {
            fn drop(&mut self) {
                unsafe { libc::closedir(self.0) };
            }
        }
        let dir = Directory(dir);
        let mut found = None;
        let mut count = 0;
        loop {
            let e = unsafe { libc::readdir(dir.0) };
            if e.is_null() {
                break;
            }
            count += 1;
            ensure!(count <= 20_000, "asset directory exceeds entry bound");
            let name = unsafe { std::ffi::CStr::from_ptr((*e).d_name.as_ptr()) };
            if name.to_bytes().eq_ignore_ascii_case(part.as_bytes()) {
                ensure!(found.is_none(), "ambiguous physical asset path component");
                found = Some(name.to_owned());
            }
        }
        found.context("asset path component missing")
    }
    fn relative(root: &File, path: &str, directory: bool) -> Result<File> {
        ensure!(valid_path(path), "invalid relative path");
        let mut parent = None::<File>;
        let mut parts = path.split('/').peekable();
        while let Some(part) = parts.next() {
            let name = CString::new(part)?;
            let flags = libc::O_RDONLY
                | libc::O_NOFOLLOW
                | libc::O_CLOEXEC
                | if parts.peek().is_some() || directory {
                    libc::O_DIRECTORY
                } else {
                    libc::O_NONBLOCK
                };
            // Directory-descriptor walk prevents symlink/rename escape races.
            let mut fd = unsafe {
                libc::openat(
                    parent.as_ref().unwrap_or(root).as_raw_fd(),
                    name.as_ptr(),
                    flags,
                )
            };
            if fd < 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ENOENT) {
                let matched = case_name(parent.as_ref().unwrap_or(root), part)?;
                fd = unsafe {
                    libc::openat(
                        parent.as_ref().unwrap_or(root).as_raw_fd(),
                        matched.as_ptr(),
                        flags,
                    )
                };
            }
            if fd < 0 {
                return Err(std::io::Error::last_os_error().into());
            }
            parent = Some(unsafe { File::from_raw_fd(fd) });
        }
        let file = parent.context("empty path")?;
        ensure!(
            directory || file.metadata()?.is_file(),
            "asset must be a regular file"
        );
        Ok(file)
    }
    impl Reader {
        /// The root is the existing selected web folder, already authorized by
        /// UIKit's security-scoped bookmark. This layer never modifies it.
        pub fn open(root: &Path) -> Result<Self> {
            Self::open_data(&root.join("data"))
        }
        pub fn open_data(root: &Path) -> Result<Self> {
            Self::open_frozen(root, None)
        }
        pub fn open_frozen(root: &Path, frozen: Option<&[u8]>) -> Result<Self> {
            Self::open_shared_frozen(root, frozen.map(std::sync::Arc::from))
        }
        /// Share the launch's immutable manifest; the worker cannot observe a
        /// replacement file with reordered archive IDs during an active launch.
        pub fn open_shared_frozen(root: &Path, frozen: Option<std::sync::Arc<[u8]>>) -> Result<Self> {
            use std::os::unix::fs::OpenOptionsExt;
            let data = std::fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(root)?;
            let bytes = if let Some(frozen) = frozen {
                ensure!(frozen.len() <= MANIFEST_LIMIT, "manifest exceeds bound");
                frozen
            } else {
                let mut manifest = relative(&data, "manifest.json", false)?;
                let size = manifest.metadata()?.len();
                ensure!(
                    size > 0 && size <= MANIFEST_LIMIT as u64,
                    "manifest size outside bounds"
                );
                let mut bytes = Vec::with_capacity(size as usize);
                (&mut manifest)
                    .take(MANIFEST_LIMIT as u64 + 1)
                    .read_to_end(&mut bytes)?;
                std::sync::Arc::from(bytes)
            };
            let entries = Index::parse(&bytes)?.into_worker_entries();
            Ok(Self {
                root: data,
                entries,
                manifest: bytes,
                cache: Vec::with_capacity(OPEN_FILES),
                scratch: Box::new([0; SCRATCH_BYTES]),
                stamp: 0,
                delivered: 0,
                opens: 0,
                cache_hits: 0,
            })
        }
        pub fn cached_files(&self) -> usize {
            self.cache.len()
        }
        /// Original bytes, including metadata and file-ID order, for publication
        /// to the engine. No transformed or fabricated manifest entries.
        pub fn manifest_bytes(&self) -> &[u8] {
            &self.manifest
        }
        /// Synchronous bounded transfer. Sink must copy bytes before returning.
        /// A failure/cancellation is an error even if earlier chunks were sent;
        /// callers must never publish success for an incomplete request.
        pub fn read(
            &mut self,
            id: usize,
            offset: u64,
            length: usize,
            stop: &AtomicBool,
            mut sink: impl FnMut(&[u8]) -> Result<()>,
        ) -> Result<usize> {
            ensure!(
                length <= i32::MAX as usize && offset <= i64::MAX as u64,
                "request outside protocol limits"
            );
            ensure!(!stop.load(Ordering::Acquire), "asset read cancelled");
            let entry = self.entries.get(id).context("unknown asset ID")?;
            if self.stamp == u64::MAX {
                self.cache.clear();
                self.stamp = 0;
            }
            self.stamp += 1;
            let at = if let Some(at) = self.cache.iter().position(|v| v.0 == id) {
                self.cache_hits += 1;
                at
            } else {
                if self.cache.len() == OPEN_FILES {
                    let at = self
                        .cache
                        .iter()
                        .enumerate()
                        .min_by_key(|(_, v)| v.2)
                        .unwrap()
                        .0;
                    self.cache.swap_remove(at);
                }
                let file = relative(&self.root, &entry.path, false)
                    .with_context(|| format!("open {}", entry.path))?;
                self.opens += 1;
                self.cache.push((id, file, self.stamp));
                self.cache.len() - 1
            };
            self.cache[at].2 = self.stamp;
            let file = &self.cache[at].1;
            ensure!(
                file.metadata()?.len() == entry.bytes,
                "manifest/physical asset size mismatch: {}",
                entry.path
            );
            let count = length.min(entry.bytes.saturating_sub(offset) as usize);
            let mut done = 0;
            while done < count {
                ensure!(
                    !stop.load(Ordering::Acquire),
                    "asset read cancelled after {done} bytes"
                );
                let n = (count - done).min(SCRATCH_BYTES);
                let got = loop {
                    match file.read_at(&mut self.scratch[..n], offset + done as u64) {
                        Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                        r => break r?,
                    }
                };
                ensure!(got != 0, "unexpected EOF after {done} bytes");
                sink(&self.scratch[..got])?;
                done += got;
                self.delivered += got as u64;
            }
            Ok(done)
        }
    }
}
#[cfg(unix)]
pub use unix::Reader;

#[cfg(test)]
mod tests {
    use super::*;
    fn index(rows: serde_json::Value) -> Result<Index> {
        Index::parse(
            serde_json::to_string(&serde_json::json!({"mount":"/game/","files":rows}))
                .unwrap()
                .as_bytes(),
        )
    }
    #[test]
    fn manifest_identity_and_case() {
        let i = index(serde_json::json!([
            ["Z/a.rpf", 4294967419u64, 7],
            ["z/A.rpf", 0, 8],
            ["x64/a.rpf", 12, 0]
        ]))
        .unwrap();
        assert_eq!(i.entries[0].bytes, 4294967419);
        assert_eq!(i.lookup("/game/Z/a.rpf", true).unwrap(), 0);
        assert!(i.lookup("/game/z/a.rpf", true).is_err());
        assert_eq!(i.lookup("/game/X64/A.RPF", true).unwrap(), 2);
        assert!(i.lookup("/game/X64/A.RPF", false).is_err());
        assert_ne!(save_namespace(1).unwrap(), save_namespace(2).unwrap());
        assert!(save_namespace(0).is_err());
    }
    #[test]
    fn invalid_paths_and_sizes() {
        for p in [
            "", "../a", "/a", "a//b", "a/./b", "a/../b", "a\\b", "a:b", "a\0b", "a\nb",
        ] {
            assert!(index(serde_json::json!([[p, 1, 0]])).is_err(), "{p:?}");
        }
        for n in [
            serde_json::json!(-1),
            serde_json::json!(true),
            serde_json::json!(1.5),
            serde_json::json!(9007199254740992u64),
        ] {
            assert!(index(serde_json::json!([["a", n, 0]])).is_err());
        }
        assert!(index(serde_json::json!([["a", 1, 0], ["a", 2, 0]])).is_err());
        assert!(index(serde_json::json!([])).is_err());
        assert_eq!(
            index(serde_json::json!([["a", 3.0, 0.0]])).unwrap().entries[0].bytes,
            3
        );
    }
    #[test]
    fn worker_keeps_original_entries_without_path_lookup_copies() {
        let rows: Vec<_> = (0..20_000).map(|i| {
            serde_json::json!([format!("x64/levels/gta5/cityw/archive-{i:05}.rpf"), i, 0])
        }).collect();
        let parsed = index(serde_json::json!(rows)).unwrap();
        let entry_bytes: usize = parsed.entries.iter().map(|e| e.path.capacity()).sum();
        let lookup_bytes: usize = parsed.exact.keys().map(String::capacity).sum::<usize>()
            + parsed.folded.keys().map(String::capacity).sum::<usize>();
        assert_eq!(lookup_bytes, 2 * entry_bytes);
        assert_eq!(parsed.exact.len(), 20_000);
        assert_eq!(parsed.folded.len(), 20_000);
        let entries_ptr = parsed.entries.as_ptr();
        let path_ptr = parsed.entries[123].path.as_ptr();
        let entries = parsed.into_worker_entries();
        assert_eq!(entries.as_ptr(), entries_ptr);
        assert_eq!(entries[123].path.as_ptr(), path_ptr);
        assert_eq!(entries[123].path, "x64/levels/gta5/cityw/archive-00123.rpf");
        assert_eq!(entries[123].bytes, 123);
        eprintln!("worker index fixture: retained path payload {entry_bytes} bytes; released lookup path payload {lookup_bytes} bytes plus 40,000 map entries (allocator/footprint not measured)");
    }
}
