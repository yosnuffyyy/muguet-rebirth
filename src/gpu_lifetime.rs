//! Cache ownership follows explicit protocol-object lifetimes. Removing a cache
//! handle never destroys a native resource or waits for the GPU: encoded work
//! retains its own wgpu references until completion.
use rustc_hash::{FxHashMap, FxHashSet};
use std::rc::Rc;

struct Entry<T> {
    value: T,
    owners: Vec<u32>,
}

/// A reverse index makes retirement proportional to affected groups, rather
/// than scanning the entire cache for each streaming-object destruction.
/// Keys share one allocation between the cache and its reverse-index edges.
pub struct DependencyCache<T> {
    entries: FxHashMap<Rc<[u64]>, Entry<T>>,
    owners: FxHashMap<u32, FxHashSet<Rc<[u64]>>>,
}

impl<T> Default for DependencyCache<T> {
    fn default() -> Self { Self { entries: Default::default(), owners: Default::default() } }
}

impl<T> DependencyCache<T> {
    pub fn get(&self, key: &[u64]) -> Option<&T> { self.entries.get(key).map(|e| &e.value) }
    pub fn len(&self) -> usize { self.entries.len() }
    pub fn values(&self) -> impl Iterator<Item = &T> { self.entries.values().map(|e| &e.value) }
    pub fn owner_count(&self) -> usize { self.owners.len() }
    pub fn dependency_count(&self) -> usize { self.entries.values().map(|e| e.owners.len()).sum() }

    pub fn insert(&mut self, key: Vec<u64>, value: T, mut owners: Vec<u32>) {
        self.remove(&key);
        owners.retain(|id| *id != 0);
        owners.sort_unstable();
        owners.dedup();
        let key: Rc<[u64]> = key.into();
        for id in &owners { self.owners.entry(*id).or_default().insert(key.clone()); }
        self.entries.insert(key, Entry { value, owners });
    }

    fn remove(&mut self, key: &[u64]) -> bool {
        let Some(entry) = self.entries.remove(key) else { return false };
        for id in entry.owners {
            if let Some(keys) = self.owners.get_mut(&id) {
                keys.remove(key);
                if keys.is_empty() { self.owners.remove(&id); }
            }
        }
        true
    }

    pub fn invalidate(&mut self, owner: u32) -> usize {
        let Some(keys) = self.owners.remove(&owner) else { return 0 };
        let mut removed = 0;
        for key in keys { removed += usize::from(self.remove(&key)); }
        removed
    }

    pub fn retain(&mut self, mut keep: impl FnMut(&[u64], &T) -> bool) {
        let remove: Vec<_> = self.entries.iter()
            .filter_map(|(k, e)| (!keep(k, &e.value)).then(|| k.clone())).collect();
        for key in remove { self.remove(&key); }
    }

    pub fn clear(&mut self) { self.entries.clear(); self.owners.clear(); }
}

/// Tracks descriptor-owned view caches separately from bind groups. An engine
/// resource replacement must not reuse a TextureView of its old native object.
#[derive(Default)]
pub struct ViewOwners {
    by_resource: FxHashMap<u32, FxHashSet<u32>>,
    by_view: FxHashMap<u32, u32>,
}

impl ViewOwners {
    pub fn insert(&mut self, view: u32, resource: u32) {
        self.remove(view);
        self.by_view.insert(view, resource);
        self.by_resource.entry(resource).or_default().insert(view);
    }
    pub fn remove(&mut self, view: u32) {
        if let Some(resource) = self.by_view.remove(&view) {
            if let Some(views) = self.by_resource.get_mut(&resource) {
                views.remove(&view);
                if views.is_empty() { self.by_resource.remove(&resource); }
            }
        }
    }
    pub fn views(&self, resource: u32) -> Vec<u32> {
        self.by_resource.get(&resource).map(|v| v.iter().copied().collect()).unwrap_or_default()
    }
    pub fn len(&self) -> usize { self.by_view.len() }
}

#[cfg(test)]
#[path = "gpu_lifetime_tests.rs"]
mod tests;
