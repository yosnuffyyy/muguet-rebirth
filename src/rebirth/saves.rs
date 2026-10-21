//! Copy legacy /userdata into an isolated Muguet filesystem, never rewrite it.
//! Both hosts execute the same original engine: payloads stay opaque and exact.
use anyhow::{ensure, Context, Result};
use std::{fs, io::{Read, Write}, path::{Path, PathBuf}};
const LIMIT: u64 = 256 * 1024 * 1024;
const FILES: usize = 10_000;
const MARKER: &str = ".rebirth-migration-v1";

fn inventory(root: &Path, at: &Path, out: &mut Vec<(PathBuf,u64)>, total: &mut u64, nodes:&mut usize, depth:usize) -> Result<()> {
    ensure!(out.len() < FILES && depth <= 32, "save migration exceeds file bound");
    for item in fs::read_dir(at)? {
        let item = item?;*nodes+=1;ensure!(*nodes<=FILES,"save tree exceeds entry bound");
        let path = item.path();
        let md = fs::symlink_metadata(&path)?;
        ensure!(!md.file_type().is_symlink(), "save symlinks are not migrated");
        if md.is_dir() { inventory(root,&path,out,total,nodes,depth+1)?; }
        else {
            ensure!(md.is_file(), "save source is not a regular file");
            *total = total.checked_add(md.len()).context("save size overflow")?;
            ensure!(*total <= LIMIT && out.len() < FILES, "save migration exceeds bounded budget");
            out.push((path.strip_prefix(root)?.to_path_buf(),md.len()));
        }
    }
    Ok(())
}

fn copy_verified(from: &Path, to: &Path, length:u64) -> Result<()> {
    if let Some(parent)=to.parent() { fs::create_dir_all(parent)?; }
    let open_source=||->std::io::Result<fs::File> {
        let mut options=fs::OpenOptions::new();options.read(true);
        #[cfg(unix)] {use std::os::unix::fs::OpenOptionsExt;options.custom_flags(libc::O_NOFOLLOW|libc::O_CLOEXEC);}
        options.open(from)
    };
    let mut source=open_source()?;
    ensure!(source.metadata()?.len()==length,"save changed during migration");
    let mut dest=fs::OpenOptions::new().write(true).create_new(true).open(to)?;
    let mut chunk=[0u8;64*1024];
    let mut copied=0u64;
    loop { let n=source.read(&mut chunk)?; if n==0 {break} copied+=n as u64;
        ensure!(copied<=length,"save grew during migration");dest.write_all(&chunk[..n])?;
    }
    ensure!(copied==length,"save shrank during migration");dest.sync_all()?;drop(dest);
    let mut source=open_source()?;let mut dest=fs::File::open(to)?;
    let mut verify=[0u8;64*1024];let mut left=length;
    while left>0 {let n=left.min(chunk.len() as u64) as usize;source.read_exact(&mut chunk[..n])?;dest.read_exact(&mut verify[..n])?;
        ensure!(chunk[..n]==verify[..n],"save copy verification failed");left-=n as u64;}
    ensure!(source.read(&mut chunk[..1])?==0,"save changed during verification");
    Ok(())
}

pub fn prepare(support:&Path,mode:u32)->Result<PathBuf> {
    let namespace=super::assets::save_namespace(mode)?;
    let base=support.join("RebirthSaves");fs::create_dir_all(&base)?;
    let target=base.join(namespace);
    if target.exists() {
        ensure!(target.join(MARKER).is_file() && target.join("userdata").is_dir(),"existing Rebirth save root is incomplete; originals preserved");
        return Ok(target);
    }
    let source=support.join(namespace);
    let staging=base.join(format!("{namespace}.pending"));
    // Never merge over a partial or unrelated directory. A failed copy is kept
    // for inspection and cannot become an active filesystem on the next launch.
    fs::create_dir(&staging).context("save migration staging already exists or cannot be created")?;
    fs::create_dir(staging.join("userdata"))?;
    let mut files=Vec::new();let mut bytes=0;
    if source.exists() {
        ensure!(fs::symlink_metadata(&source)?.is_dir(),"legacy save root is not a directory");
        inventory(&source,&source,&mut files,&mut bytes,&mut 0,0)?;
        for (relative,len) in &files {copy_verified(&source.join(relative),&staging.join("userdata").join(relative),*len)?;}
    }
    let mut marker=fs::OpenOptions::new().write(true).create_new(true).open(staging.join(MARKER))?;
    writeln!(marker,"schema=1\noriginal-engine={}\nmode={mode}\nfiles={}\nbytes={bytes}\nlegacy-untouched=true",super::layout::ORIGINAL_SHA256,files.len())?;
    marker.sync_all()?;drop(marker);
    fs::rename(&staging,&target)?;
    eprintln!("[rebirth] isolated save migration: mode {mode}, {} files / {bytes} bytes verified; legacy directory unchanged",files.len());
    Ok(target)
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn exact_isolated_idempotent_migration() {
        let root=std::env::temp_dir().join(format!("rebirth-save-test-{}",std::process::id()));
        fs::create_dir_all(root.join("NativeRuntimeProbe/Documents/profile")).unwrap();
        fs::create_dir_all(root.join("NativeSandboxProbe")).unwrap();
        fs::write(root.join("NativeRuntimeProbe/Documents/profile/SGTA50000"),b"story bytes\0\xff").unwrap();
        fs::write(root.join("NativeSandboxProbe/Sandbox"),b"sandbox bytes").unwrap();
        let story=prepare(&root,1).unwrap();let sandbox=prepare(&root,2).unwrap();
        let save=story.join("userdata/Documents/profile/SGTA50000");
        assert_eq!(fs::read(&save).unwrap(),b"story bytes\0\xff");
        fs::write(&save,b"new session").unwrap();assert_eq!(prepare(&root,1).unwrap(),story);
        assert_eq!(fs::read(save).unwrap(),b"new session");
        assert_eq!(fs::read(root.join("NativeRuntimeProbe/Documents/profile/SGTA50000")).unwrap(),b"story bytes\0\xff");
        assert_eq!(fs::read(sandbox.join("userdata/Sandbox")).unwrap(),b"sandbox bytes");
        assert_ne!(story,sandbox);assert!(prepare(&root,0).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
