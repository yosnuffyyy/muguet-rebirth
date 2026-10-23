// muguet - fs.rs
// Constantin Clerc (c22dev) - 0.1

use crate::{read_bytes, read_cstr, read_u64, write_bytes, write_u64};
use std::collections::HashMap;
use std::ffi::CString;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::fs::{FileExt, MetadataExt};
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use wasmtime::{SharedMemory, Val};

const EBADF: i64 = 8;
const EEXIST: i64 = 20;
const EINVAL: i64 = 28;
const EISDIR: i64 = 31;
const ENOENT: i64 = 44;
const ENOSYS: i64 = 52;
const ENOTDIR: i64 = 54;
const ENOTEMPTY: i64 = 55;
const ENOTTY: i64 = 59;
const EIO: i64 = 29;
const EFAULT: i64 = 21;

const O_ACCMODE: i64 = 3;
const O_CREAT: i64 = 64;
const O_EXCL: i64 = 128;
const O_TRUNC: i64 = 512;
const O_APPEND: i64 = 1024;
const O_DIRECTORY: i64 = 65536;
const AT_FDCWD: i64 = -100;

const S_IFCHR: u32 = 0o020000;
const S_IFDIR: u32 = 0o040000;
const S_IFREG: u32 = 0o100000;

enum Entry {
    File { file: File },
    Dir { guest: String, names: Option<Vec<(String, u8)>>, pos: u64 },
}

struct Fs {
    root: PathBuf,
    fds: HashMap<i64, Entry>,
    next_fd: i64,
    // One bounded transfer buffer shared by synchronous file operations under FS.
    scratch: Box<[u8; FILE_SCRATCH_BYTES]>,
}

const FILE_SCRATCH_BYTES: usize = 64 * 1024;

static FS: LazyLock<Mutex<Fs>> = LazyLock::new(|| {
    let root = std::env::var("MUGUET_FSROOT").unwrap_or_else(|_| "fsroot".into());
    let root = PathBuf::from(root);
    for d in ["tmp", "home/web_user", "dev", "proc/self"] {
        let _ = fs::create_dir_all(root.join(d));
    }
    Mutex::new(Fs { root, fds: HashMap::new(), next_fd: 3, scratch: Box::new([0; FILE_SCRATCH_BYTES]) })
});

fn guest_abs(fs: &Fs, dirfd: i64, path: &str) -> Result<String, i64> {
    let base = if path.starts_with('/') || dirfd == AT_FDCWD {
        "/".to_string()
    } else {
        match fs.fds.get(&dirfd) {
            Some(Entry::Dir { guest, .. }) => guest.clone(),
            Some(_) => return Err(ENOTDIR),
            None => return Err(EBADF),
        }
    };
    let mut parts: Vec<String> = Vec::new();
    for c in Path::new(&base).join(path).components() {
        match c {
            Component::Normal(s) => parts.push(s.to_string_lossy().into_owned()),
            Component::ParentDir => { parts.pop(); }
            _ => {}
        }
    }
    Ok(format!("/{}", parts.join("/")))
}

fn host_path(fs: &Fs, guest: &str) -> PathBuf {
    fs.root.join(guest.trim_start_matches('/'))
}

fn write_stat(mem: &SharedMemory, buf: u64, md: &fs::Metadata, chr: bool) -> i64 {
    let mode = if chr { S_IFCHR | 0o666 } else if md.is_dir() { S_IFDIR | 0o777 } else { S_IFREG | 0o666 };
    let mut record = crate::rebirth::abi::Stat64::from_metadata(md);
    record.mode = mode;
    let mut b = [0u8; crate::rebirth::abi::STAT64_BYTES];
    record.write(&mut b).expect("fixed stat buffer");
    if write_bytes(mem, buf, &b) {0} else {-EFAULT}
}

fn ok_i(results: &mut [Val], v: i64) {
    if let Some(r) = results.get_mut(0) {
        *r = match r { Val::I64(_) => Val::I64(v), _ => Val::I32(v as i32) };
    }
}

fn guest_range(mem: &SharedMemory, ptr: u64, length: u64) -> bool {
    ptr.checked_add(length).is_some_and(|end| end <= mem.data_size() as u64)
}

fn iovecs(mem: &SharedMemory, iovs: u64, n: u64) -> Result<Vec<(u64, u64)>, i64> {
    let bytes = n.checked_mul(16).ok_or(EFAULT)?;
    if !guest_range(mem, iovs, bytes) { return Err(EFAULT); }
    let count = usize::try_from(n).map_err(|_| EFAULT)?;
    let mut result = Vec::new();
    result.try_reserve_exact(count).map_err(|_| 48 /* WASI ENOMEM */)?;
    for k in 0..n {
        let ptr = read_u64(mem, iovs + k * 16).ok_or(EFAULT)?;
        let len = read_u64(mem, iovs + k * 16 + 8).ok_or(EFAULT)?;
        if !guest_range(mem, ptr, len) { return Err(EFAULT); }
        result.push((ptr, len));
    }
    Ok(result)
}

/// File payloads are copied in bounded chunks, not allocated at request size.
/// The synchronous guest call owns these ranges until it returns. Snapshot the
/// descriptors first: a valid read destination may overlap the iovec table.
fn transfer_file(file: &mut File, mem: &SharedMemory, iov: &[(u64, u64)],
                 offset: Option<u64>, scratch: &mut [u8], writing: bool) -> Result<u64, i64> {
    if scratch.is_empty() { return Err(EINVAL); }
    let mut total = 0u64;
    for &(ptr, len) in iov {
        if !guest_range(mem, ptr, len) { return Err(EFAULT); }
        let mut done = 0u64;
        while done < len {
            let count = (len - done).min(scratch.len() as u64) as usize;
            let position = match offset {
                Some(start) => Some(start.checked_add(total).ok_or(EINVAL)?),
                None => None,
            };
            if writing {
                let (base, _) = crate::mem_base_len(mem);
                // Stable shared-memory base; never retain this pointer over I/O.
                unsafe { std::ptr::copy_nonoverlapping(base.add((ptr + done) as usize), scratch.as_mut_ptr(), count); }
            }
            let result = loop {
                let r = match (writing, position) {
                    (true, Some(at)) => file.write_at(&scratch[..count], at),
                    (true, None) => file.write(&scratch[..count]),
                    (false, Some(at)) => file.read_at(&mut scratch[..count], at),
                    (false, None) => file.read(&mut scratch[..count]),
                };
                if matches!(&r, Err(e) if e.kind() == std::io::ErrorKind::Interrupted) { continue; }
                break r;
            };
            let n = match result {
                Ok(n) => n,
                Err(_) if total != 0 => return Ok(total),
                Err(_) => return Err(EIO),
            };
            if n == 0 {
                return if writing && total == 0 { Err(EIO) } else { Ok(total) };
            }
            if !writing && !write_bytes(mem, ptr + done, &scratch[..n]) { return Err(EFAULT); }
            total += n as u64;
            done += n as u64;
            if !writing && n < count { return Ok(total); }
        }
    }
    Ok(total)
}

fn read_dir_names(host: &Path) -> Vec<(String, u8)> {
    let mut v = vec![(".".to_string(), 4u8), ("..".to_string(), 4u8)];
    if let Ok(rd) = fs::read_dir(host) {
        for e in rd.flatten() {
            let t = if e.file_type().map(|t| t.is_dir()).unwrap_or(false) { 4 } else { 8 };
            v.push((e.file_name().to_string_lossy().into_owned(), t));
        }
    }
    v
}

fn statfs_bytes(host: &Path) -> Result<[u8; 120], i64> {
    let path = CString::new(host.as_os_str().as_bytes()).map_err(|_| EINVAL)?;
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    if unsafe { libc::statvfs(path.as_ptr(), stats.as_mut_ptr()) } != 0 {
        let error = std::io::Error::last_os_error();
        return Err(match error.kind() {
            std::io::ErrorKind::NotFound => ENOENT,
            std::io::ErrorKind::PermissionDenied => 2,
            _ => EIO,
        });
    }
    let stats = unsafe { stats.assume_init() };
    // The memory64 guest uses 8-byte statfs fields, not the host's struct layout.
    let mut bytes = [0u8; 120];
    for (offset, value) in [
        (8, stats.f_bsize as u64), (16, stats.f_blocks as u64),
        (24, stats.f_bfree as u64), (32, stats.f_bavail as u64),
        (40, stats.f_files as u64), (48, stats.f_ffree as u64),
        (56, stats.f_fsid as u64), (64, stats.f_namemax as u64),
        (72, stats.f_frsize as u64), (80, stats.f_flag as u64),
    ] {
        bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }
    Ok(bytes)
}

pub fn handle(name: &str, mem: &SharedMemory, p: &[Val], results: &mut [Val]) -> bool {
    let a = |i: usize| -> i64 { match p.get(i) { Some(Val::I32(x)) => *x as i64, Some(Val::I64(x)) => *x, _ => 0 } };
    let mut fs = FS.lock().unwrap();
    let r: i64 = match name {
        "__syscall_openat" => (|| {
            let flags = a(2);
            let mode_creat = flags & O_CREAT != 0;
            let guest = guest_abs(&fs, a(0), &read_cstr(mem, a(1) as u64)).map_err(|e| -e)?;
            let host = host_path(&fs, &guest);
            let exists = host.exists();
            if exists && mode_creat && flags & O_EXCL != 0 { return Err(-EEXIST); }
            if host.is_dir() {
                if flags & O_ACCMODE != 0 { return Err(-EISDIR); }
                let fd = fs.next_fd; fs.next_fd += 1;
                fs.fds.insert(fd, Entry::Dir { guest, names: None, pos: 0 });
                return Ok(fd);
            }
            if flags & O_DIRECTORY != 0 { return Err(if exists { -ENOTDIR } else { -ENOENT }); }
            if !exists && !mode_creat { return Err(-ENOENT); }
            let acc = flags & O_ACCMODE;
            let file = OpenOptions::new()
                .read(acc == 0 || acc == 2)
                .write(acc == 1 || acc == 2)
                .append(flags & O_APPEND != 0)
                .create(mode_creat)
                .truncate(flags & O_TRUNC != 0)
                .open(&host)
                .map_err(|_| -ENOENT)?;
            let fd = fs.next_fd; fs.next_fd += 1;
            fs.fds.insert(fd, Entry::File { file });
            Ok(fd)
        })().unwrap_or_else(|e| e),
        "__syscall_stat64" | "__syscall_lstat64" | "__syscall_newfstatat" => {
            let (dirfd, path, buf) = if name == "__syscall_newfstatat" { (a(0), a(1), a(2)) } else { (AT_FDCWD, a(0), a(1)) };
            match guest_abs(&fs, dirfd, &read_cstr(mem, path as u64)) {
                Ok(g) => match fs::metadata(host_path(&fs, &g)) {
                    Ok(md) => { write_stat(mem, buf as u64, &md, g.starts_with("/dev/")) }
                    Err(_) => -ENOENT,
                },
                Err(e) => -e,
            }
        }
        "__syscall_fstat64" => {
            let fd = a(0);
            if fd <= 2 {
                let md = fs::metadata("/dev/null").unwrap();
                write_stat(mem, a(1) as u64, &md, true)
            } else {
                match fs.fds.get(&fd) {
                    Some(Entry::File { file, .. }) => match file.metadata() { Ok(md) => { write_stat(mem, a(1) as u64, &md, false) } Err(_) => -EBADF },
                    Some(Entry::Dir { guest, .. }) => { let md = fs::metadata(host_path(&fs, guest)).unwrap(); write_stat(mem, a(1) as u64, &md, false) }
                    None => -EBADF,
                }
            }
        }
        "__syscall_mkdirat" => match guest_abs(&fs, a(0), &read_cstr(mem, a(1) as u64)) {
            Ok(g) => { let h = host_path(&fs, &g); if h.exists() { -EEXIST } else if fs::create_dir(&h).is_ok() { 0 } else { -ENOENT } }
            Err(e) => -e,
        },
        "__syscall_unlinkat" => match guest_abs(&fs, a(0), &read_cstr(mem, a(1) as u64)) {
            Ok(g) => {
                let h = host_path(&fs, &g);
                let res = if a(2) & 0x200 != 0 { fs::remove_dir(&h) } else { fs::remove_file(&h) };
                if res.is_ok() { 0 } else if !h.exists() { -ENOENT } else { -ENOTEMPTY }
            }
            Err(e) => -e,
        },
        "__syscall_rmdir" => match guest_abs(&fs, AT_FDCWD, &read_cstr(mem, a(0) as u64)) {
            Ok(g) => { let h = host_path(&fs, &g); if fs::remove_dir(&h).is_ok() { 0 } else if !h.exists() { -ENOENT } else { -ENOTEMPTY } }
            Err(e) => -e,
        },
        "__syscall_renameat" => {
            let from = guest_abs(&fs, a(0), &read_cstr(mem, a(1) as u64));
            let to = guest_abs(&fs, a(2), &read_cstr(mem, a(3) as u64));
            match (from, to) {
                (Ok(f), Ok(t)) => if fs::rename(host_path(&fs, &f), host_path(&fs, &t)).is_ok() { 0 } else { -ENOENT },
                (Err(e), _) | (_, Err(e)) => -e,
            }
        }
        "__syscall_ftruncate64" => match fs.fds.get(&a(0)) {
            Some(Entry::File { file, .. }) => if file.set_len(a(1) as u64).is_ok() { 0 } else { -EINVAL },
            _ => -EBADF,
        },
        "__syscall_getcwd" => {
            let (buf, size) = (a(0) as u64, a(1) as u64);
            if size < 2 { -EINVAL } else { write_bytes(mem, buf, b"/\0"); 2 }
        }
        "__syscall_getdents64" => {
            let (fd, dirp, count) = (a(0), a(1) as u64, a(2) as u64);
            let root = fs.root.clone();
            match fs.fds.get_mut(&fd) {
                Some(Entry::Dir { guest, names, pos }) => {
                    let names = names.get_or_insert_with(|| read_dir_names(&root.join(guest.trim_start_matches('/'))));
                    let start = (*pos / 280) as usize;
                    let end = names.len().min(start + (count / 280) as usize);
                    let mut off = 0u64;
                    for (idx, (n, t)) in names.iter().enumerate().take(end).skip(start) {
                        let mut rec = [0u8; 280];
                        rec[0..8].copy_from_slice(&((idx as u64) + 1).to_le_bytes());
                        rec[8..16].copy_from_slice(&(((idx + 1) * 280) as u64).to_le_bytes());
                        rec[16..18].copy_from_slice(&280u16.to_le_bytes());
                        rec[18] = *t;
                        let nb = n.as_bytes();
                        let l = nb.len().min(255);
                        rec[19..19 + l].copy_from_slice(&nb[..l]);
                        write_bytes(mem, dirp + off, &rec);
                        off += 280;
                    }
                    *pos = (end * 280) as u64;
                    off as i64
                }
                Some(_) => -ENOTDIR,
                None => -EBADF,
            }
        }
        "__syscall_fcntl64" => if a(0) <= 2 || fs.fds.contains_key(&a(0)) { 0 } else { -EBADF },
        "__syscall_ioctl" => -ENOTTY,
        "__syscall_statfs64" => (|| {
            if a(1) != 120 { return Err(-EINVAL); }
            let guest = guest_abs(&fs, AT_FDCWD, &read_cstr(mem, a(0) as u64)).map_err(|e| -e)?;
            let host = host_path(&fs, &guest);
            let bytes = statfs_bytes(host.ancestors().find(|p| p.exists()).unwrap_or(&host)).map_err(|e| -e)?;
            if !write_bytes(mem, a(2) as u64, &bytes) { return Err(-EFAULT); }
            Ok(0)
        })().unwrap_or_else(|e| e),

        "__syscall_socket" | "__syscall_connect" | "__syscall_accept4" | "__syscall_sendto"
        | "__syscall_recvfrom" | "__syscall_setsockopt" | "__syscall_getsockopt"
        | "__syscall_getsockname" | "__syscall_shutdown" | "__syscall_poll" => -ENOSYS,

        "fd_read" | "fd_pread" | "fd_write" | "fd_pwrite"
            if matches!(name, "fd_read" | "fd_pread") || a(0) > 2 => {
            let fd = a(0);
            let writing = matches!(name, "fd_write" | "fd_pwrite");
            let positioned = matches!(name, "fd_pread" | "fd_pwrite");
            let offset = positioned.then(|| a(3) as u64);
            let count_ptr = a(if positioned { 4 } else { 3 }) as u64;
            (|| {
                if !guest_range(mem, count_ptr, 8) { return Err(EFAULT); }
                let iov = iovecs(mem, a(1) as u64, a(2) as u64)?;
                let Fs { fds, scratch, .. } = &mut *fs;
                let total = match fds.get_mut(&fd) {
                    Some(Entry::File { file }) => transfer_file(file, mem, &iov, offset, &mut scratch[..], writing)?,
                    Some(Entry::Dir { .. }) => return Err(if writing { EBADF } else { EISDIR }),
                    None if fd == 0 && !writing => 0,
                    None => return Err(EBADF),
                };
                write_u64(mem, count_ptr, total);
                Ok(0)
            })().unwrap_or_else(|error| error)
        }
        "fd_seek" => {
            let (fd, offset, whence, out) = (a(0), a(1), a(2), a(3) as u64);
            match fs.fds.get_mut(&fd) {
                Some(Entry::File { file, .. }) => {
                    let sf = match whence { 0 => SeekFrom::Start(offset as u64), 1 => SeekFrom::Current(offset), 2 => SeekFrom::End(offset), _ => return { ok_i(results, EINVAL); true } };
                    match file.seek(sf) { Ok(p) => { write_u64(mem, out, p); 0 } Err(_) => EINVAL }
                }
                Some(Entry::Dir { pos, .. }) => { if whence == 0 { *pos = offset as u64; } write_u64(mem, out, *pos); 0 }
                None => EBADF,
            }
        }
        "fd_close" => if fs.fds.remove(&a(0)).is_some() || a(0) <= 2 { 0 } else { EBADF },
        _ => return false,
    };
    drop(fs);
    ok_i(results, r);
    true
}

pub fn httpfs_manifest(mem: &SharedMemory, p: &[Val], results: &mut [Val]) {
    // The launch already owns immutable original bytes. Keep a separate owner
    // only for the upstream CLI path without a frozen Rebirth launch.
    static CLI_MANIFEST: LazyLock<Option<Vec<u8>>> = LazyLock::new(|| {
        let dir = std::env::var("MUGUET_DATA").unwrap_or_default();
        fs::read(Path::new(&dir).join("manifest.json")).ok()
    });
    let manifest = crate::rebirth::current().map(|launch| launch.layout.manifest.as_ref())
        .or_else(|| CLI_MANIFEST.as_deref());
    let a = |i: usize| -> i64 { match p.get(i) { Some(Val::I32(x)) => *x as i64, Some(Val::I64(x)) => *x, _ => 0 } };
    let r = match manifest {
        None => -1,
        Some(bytes) => {
            if a(1) as usize > bytes.len() {
                write_bytes(mem, a(0) as u64, bytes);
                write_bytes(mem, a(0) as u64 + bytes.len() as u64, &[0]);
            }
            bytes.len() as i64
        }
    };
    ok_i(results, r);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn transfer_fixture(label: &str) -> (std::path::PathBuf, File, SharedMemory) {
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("rebirth-transfer-{label}-{}-{nonce}", std::process::id()));
        let file = OpenOptions::new().create_new(true).read(true).write(true).open(&path).unwrap();
        let mut config = wasmtime::Config::new(); config.shared_memory(true);
        let engine = wasmtime::Engine::new(&config).unwrap();
        let mem = SharedMemory::new(&engine, wasmtime::MemoryType::shared(1024, 1024)).unwrap();
        (path, file, mem)
    }

    #[test]
    fn bounded_transfer_large_request_byte_equivalence() {
        // This is a host file-I/O fixture, not a measured iPhone streaming call.
        const REQUEST: usize = 37_314_560;
        let (path, mut file, mem) = transfer_fixture("large");
        let block: Vec<u8> = (0..FILE_SCRATCH_BYTES).map(|i| (i.wrapping_mul(31) >> 3) as u8).collect();
        let mut remaining = REQUEST;
        while remaining != 0 {
            let n = remaining.min(block.len()); file.write_all(&block[..n]).unwrap(); remaining -= n;
        }
        let ptr = FILE_SCRATCH_BYTES as u64;
        file.seek(SeekFrom::Start(0)).unwrap();
        let start = std::time::Instant::now();
        let mut old = vec![0; REQUEST];
        assert_eq!(file.read(&mut old).unwrap(), REQUEST);
        assert!(write_bytes(&mem, ptr, &old));
        let before = start.elapsed();
        drop(old);
        let checksum = |mem: &SharedMemory| -> u64 {
            (0..REQUEST).step_by(FILE_SCRATCH_BYTES).map(|at| {
                read_bytes(mem, ptr + at as u64, (REQUEST - at).min(FILE_SCRATCH_BYTES) as u64)
                    .into_iter().map(u64::from).sum::<u64>()
            }).sum()
        };
        let expected = checksum(&mem);
        assert!(expected > 0);
        let zeros = [0; FILE_SCRATCH_BYTES];
        for at in (0..REQUEST).step_by(FILE_SCRATCH_BYTES) {
            write_bytes(&mem, ptr + at as u64, &zeros[..(REQUEST - at).min(FILE_SCRATCH_BYTES)]);
        }
        let mut scratch = Box::new([0; FILE_SCRATCH_BYTES]);
        let allocation = scratch.as_ptr();
        file.seek(SeekFrom::Start(17)).unwrap();
        let start = std::time::Instant::now();
        assert_eq!(transfer_file(&mut file, &mem, &[(ptr, REQUEST as u64)], Some(0), &mut scratch[..], false), Ok(REQUEST as u64));
        let after = start.elapsed();
        assert_eq!(file.stream_position().unwrap(), 17, "pread must preserve file position");
        assert_eq!(checksum(&mem), expected);
        assert_eq!(scratch.as_ptr(), allocation);
        // Round-trip the same large payload through positioned writes.
        assert_eq!(transfer_file(&mut file, &mem, &[(ptr, REQUEST as u64)], Some(0), &mut scratch[..], true), Ok(REQUEST as u64));
        assert_eq!(file.stream_position().unwrap(), 17);
        assert_eq!(file.metadata().unwrap().len(), REQUEST as u64);
        eprintln!("[ram-fixture] file payload {} bytes; old transient payload allocation {}, bounded scratch {}; host read/copy before {:?}, after {:?}; checksum {} (not device footprint/FPS)", REQUEST, REQUEST, scratch.len(), before, after, expected);
        drop(file); fs::remove_file(path).unwrap();
    }

    #[test]
    fn bounded_transfer_short_reads_zero_lengths_and_iovec_aliasing() {
        let (path, mut file, mem) = transfer_fixture("iov");
        file.write_all(b"abcdefghij").unwrap(); file.seek(SeekFrom::Start(0)).unwrap();
        let mut scratch = [0; 4];
        assert!(write_bytes(&mem, 128, &[0xa5; 64]));
        assert_eq!(transfer_file(&mut file, &mem, &[(128, 0), (128, 8), (144, 8)], None, &mut scratch, false), Ok(10));
        assert_eq!(read_bytes(&mem,128,8), b"abcdefgh");
        assert_eq!(read_bytes(&mem,144,8), [b'i',b'j',0xa5,0xa5,0xa5,0xa5,0xa5,0xa5]);
        assert_eq!(file.stream_position().unwrap(),10);
        // The first output overwrites the descriptor of the second output.
        write_u64(&mem,256,272); write_u64(&mem,264,8);
        write_u64(&mem,272,400); write_u64(&mem,280,2);
        let snapshot = iovecs(&mem,256,2).unwrap();
        assert_eq!(transfer_file(&mut file,&mem,&snapshot,Some(0),&mut scratch,false),Ok(10));
        assert_eq!(read_bytes(&mem,272,8),b"abcdefgh");
        assert_eq!(read_bytes(&mem,400,2),b"ij");
        assert!(iovecs(&mem,u64::MAX,1).is_err());
        assert!(iovecs(&mem,256,u64::MAX).is_err());
        write_u64(&mem,256,mem.data_size() as u64 - 1); write_u64(&mem,264,2);
        assert!(iovecs(&mem,256,1).is_err());
        assert_eq!(transfer_file(&mut file,&mem,&[(u64::MAX,2)],Some(0),&mut scratch,false),Err(EFAULT));
        drop(file); fs::remove_file(path).unwrap();
    }

    #[test]
    fn original_stat64_preserves_canaries_and_rejects_short_memory() {
        let mut config=wasmtime::Config::new();config.shared_memory(true);
        let engine=wasmtime::Engine::new(&config).unwrap();
        let mem=SharedMemory::new(&engine,wasmtime::MemoryType::shared(1,1)).unwrap();
        let metadata=fs::metadata(std::env::temp_dir()).unwrap();
        assert!(write_bytes(&mem,248,&[0xa5;128]));
        assert_eq!(write_stat(&mem,256,&metadata,false),0);
        assert_eq!(read_bytes(&mem,248,8),vec![0xa5;8]);
        assert_eq!(read_bytes(&mem,360,16),vec![0xa5;16]);
        let end=mem.data_size() as u64;
        assert!(write_bytes(&mem,end-103,&[0xa5;103]));
        assert_eq!(write_stat(&mem,end-103,&metadata,false),-EFAULT);
        assert_eq!(read_bytes(&mem,end-103,103),vec![0xa5;103]);
    }

    #[test]
    fn statfs_reports_real_available_space_in_guest_layout() {
        let host = std::env::temp_dir();
        let bytes = statfs_bytes(&host).unwrap();
        let field = |offset| u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap());
        let path = CString::new(host.as_os_str().as_bytes()).unwrap();
        let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
        assert_eq!(unsafe { libc::statvfs(path.as_ptr(), stats.as_mut_ptr()) }, 0);
        let stats = unsafe { stats.assume_init() };
        assert_eq!(field(8), stats.f_bsize as u64);
        assert_eq!(field(16), stats.f_blocks as u64);
        assert!(field(8) > 0);
        assert!(field(32) <= field(24));
        assert!(field(24) <= field(16));
        assert_eq!(field(72), stats.f_frsize as u64);
        // A zero-filled response previously made this writable disk look full.
        assert!(field(32) * field(72) > 1024 * 1024);
        assert_eq!(statfs_bytes(&host.join(format!("muguet-missing-{}", std::process::id()))), Err(ENOENT));
    }

    #[test]
    fn guest_save_survives_close_and_reopen() {
        let root = std::env::temp_dir().join(format!("muguet-save-test-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        std::env::set_var("MUGUET_FSROOT", &root);
        let mut config = wasmtime::Config::new();
        config.shared_memory(true);
        let engine = wasmtime::Engine::new(&config).unwrap();
        let mem = SharedMemory::new(&engine, wasmtime::MemoryType::shared(1, 1)).unwrap();
        let call = |name: &str, args: &[i64]| {
            let params: Vec<_> = args.iter().map(|v| Val::I64(*v)).collect();
            let mut result = [Val::I32(-999)];
            assert!(handle(name, &mem, &params, &mut result));
            result[0].unwrap_i32() as i64
        };
        // Console output remains the host logger's responsibility.
        for name in ["fd_write", "fd_pwrite"] {
            let mut untouched = [Val::I32(-999)];
            assert!(!handle(name, &mem, &[Val::I64(1)], &mut untouched));
            assert_eq!(untouched[0].unwrap_i32(), -999);
        }
        write_bytes(&mem, 32, b"/tmp/SGTA50000\0");
        write_bytes(&mem, 64, b"/tmp/SGTA50001\0");
        let fd = call("__syscall_openat", &[AT_FDCWD, 32, 2 | O_CREAT | O_TRUNC, 0]);
        assert!(fd >= 3);
        let payload = b"save round trip";
        write_bytes(&mem, 512, payload);
        write_u64(&mem, 256, 512);
        write_u64(&mem, 264, payload.len() as u64);
        assert_eq!(call("fd_write", &[fd, 256, 1, 320]), 0);
        assert_eq!(read_u64(&mem, 320), Some(payload.len() as u64));
        assert_eq!(call("fd_close", &[fd]), 0);
        assert_eq!(call("__syscall_renameat", &[AT_FDCWD, 32, AT_FDCWD, 64]), 0);
        // Read from disk independently, then reopen through the guest interface.
        assert_eq!(fs::read(root.join("tmp/SGTA50001")).unwrap(), payload);
        let fd = call("__syscall_openat", &[AT_FDCWD, 64, 0, 0]);
        assert!(fd >= 3);
        write_bytes(&mem, 512, &[0; 15]);
        assert_eq!(call("fd_read", &[fd, 256, 1, 320]), 0);
        assert_eq!(crate::read_bytes(&mem, 512, payload.len() as u64), payload);
        assert_eq!(call("fd_close", &[fd]), 0);
        write_bytes(&mem, 96, b"/tmp\0");
        write_bytes(&mem, 1024, &[0xaa; 128]);
        assert_eq!(call("__syscall_statfs64", &[96, 120, 1024]), 0);
        assert!(read_u64(&mem, 1056).unwrap() > 0);
        assert_eq!(crate::read_bytes(&mem, 1144, 8), [0xaa; 8]);
        assert_eq!(call("__syscall_statfs64", &[96, 88, 1024]), -EINVAL);
        fs::remove_dir_all(root).unwrap();
    }
}
