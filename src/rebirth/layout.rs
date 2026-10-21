//! Existing assets are selected in place; never invoke the ZIP importer.
use anyhow::{ensure, Context, Result};
use std::path::{Path,PathBuf};
pub const ORIGINAL_SHA256: &str = "e65bec476e8fd8125515150c24037c9bd0d0c46530e3544f9d389ab108b5592e";
pub const ORIGINAL_BUILD: &str = "8b0b5899ed";
#[derive(Clone,Debug)]
pub struct Layout { pub root:PathBuf, pub data:PathBuf, pub shaders:PathBuf, pub wasm:PathBuf }
fn child(root:&Path, relative:&str) -> Result<PathBuf> {
    ensure!(super::assets::valid_path(relative),"invalid relative path");
    let mut path=root.to_path_buf();
    for part in relative.split('/') {
        let exact=path.join(part);
        if exact.try_exists()? { path=exact; }
        else {
            let matches=std::fs::read_dir(&path)?.collect::<std::io::Result<Vec<_>>>()?.into_iter()
                .filter(|e|e.file_name().to_str().is_some_and(|s|s.eq_ignore_ascii_case(part))).collect::<Vec<_>>();
            ensure!(matches.len()==1,"missing or ambiguous path: {relative}");
            path=matches[0].path();
        }
        ensure!(!std::fs::symlink_metadata(&path)?.file_type().is_symlink(),"asset symlink rejected: {relative}");
    }
    ensure!(path.canonicalize()?.starts_with(root),"asset path escapes selected directory");
    Ok(path)
}
impl Layout {
    pub fn open(root:&Path) -> Result<Self> {
        ensure!(root.is_absolute(),"selected folder must be absolute");
        let root=root.canonicalize().context("selected folder access expired or missing")?;
        let data=child(&root,"data")?;
        let manifest=child(&root,"data/manifest.json")?;
        ensure!(manifest.metadata()?.len()<=super::assets::MANIFEST_LIMIT as u64,"manifest exceeds limit");
        super::assets::Index::parse(&std::fs::read(&manifest)?)?;
        // Pin to our original shader build; don't pick the first arbitrary b/ entry.
        let shaders=child(&root,&format!("b/{ORIGINAL_BUILD}/shaders"))?;
        let index=child(&root,&format!("b/{ORIGINAL_BUILD}/shaders/index.json"))?;
        ensure!(index.is_file(),"shader index missing");
        let wasm=root.join(format!("b/{ORIGINAL_BUILD}/game.wasm"));
        Ok(Self {root,data,shaders,wasm})
    }
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn original_layout_is_in_place_and_strict() {
        let root=std::env::temp_dir().join(format!("rebirth-layout-{}",std::process::id()));
        std::fs::create_dir_all(root.join("data")).unwrap();
        std::fs::create_dir_all(root.join(format!("b/{ORIGINAL_BUILD}/shaders"))).unwrap();
        let manifest=br#"{"mount":"/game/","files":[["original.rpf",7,0]]}"#;
        std::fs::write(root.join("data/manifest.json"),manifest).unwrap();
        let index=root.join(format!("b/{ORIGINAL_BUILD}/shaders/index.json"));
        std::fs::write(&index,b"{}").unwrap();
        let l=Layout::open(&root).unwrap();assert_eq!(l.root,root.canonicalize().unwrap());
        assert_eq!(std::fs::read(root.join("data/manifest.json")).unwrap(),manifest);
        assert!(child(&l.root,"../escape").is_err());
        std::fs::remove_file(index).unwrap();assert!(Layout::open(&root).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
