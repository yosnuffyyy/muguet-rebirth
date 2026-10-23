use super::*;

#[test]
fn exact_retirement_preserves_other_groups_and_inflight_owners() {
    let allocation = Rc::new(vec![0x5a; 4 * 1024 * 1024]);
    let weak = Rc::downgrade(&allocation);
    let mut cache = DependencyCache::default();
    cache.insert(vec![1, 10], allocation.clone(), vec![100, 101, 101, 0]);
    cache.insert(vec![2, 20], allocation.clone(), vec![100, 102]);
    let inflight = allocation.clone();
    drop(allocation);
    cache.insert(vec![3, 30], Rc::new(vec![7]), vec![200]);
    assert_eq!((cache.len(), cache.owner_count(), cache.dependency_count()), (3, 4, 5));
    assert_eq!(cache.invalidate(100), 2);
    assert_eq!((cache.len(), cache.owner_count(), cache.dependency_count()), (1, 1, 1));
    assert_eq!(inflight[0], 0x5a);
    assert_eq!(Rc::strong_count(&inflight), 1, "cache no longer retains obsolete storage");
    drop(inflight);
    assert!(weak.upgrade().is_none());
    assert_eq!(cache.get(&[3, 30]).unwrap()[0], 7);
}

#[test]
fn replacement_clear_and_uniform_retirement_remove_reverse_edges() {
    let mut cache = DependencyCache::default();
    for generation in 0..100 {
        cache.insert(vec![10], generation, vec![1, 2]);
        assert_eq!(cache.get(&[10]), Some(&generation));
        assert_eq!((cache.len(), cache.dependency_count()), (1, 2));
    }
    cache.insert(vec![11], 101, vec![2, 3]);
    cache.retain(|_, value| *value == 101);
    assert_eq!(cache.invalidate(1), 0);
    assert_eq!(cache.invalidate(2), 1);
    assert_eq!((cache.len(), cache.owner_count()), (0, 0));
    cache.insert(vec![10], 102, vec![1]);
    cache.clear();
    assert_eq!(cache.invalidate(1), 0);
    assert_eq!(cache.owner_count(), 0);
}

#[test]
fn descriptor_owner_changes_do_not_leave_stale_views() {
    let mut views = ViewOwners::default();
    views.insert(11, 1);
    views.insert(12, 1);
    views.insert(11, 2);
    assert_eq!(views.views(1), vec![12]);
    assert_eq!(views.views(2), vec![11]);
    views.remove(11);
    views.remove(12);
    assert_eq!(views.len(), 0);
    assert!(views.views(1).is_empty() && views.views(2).is_empty());
}

#[test]
fn synthetic_streaming_churn_retains_only_the_live_working_set() {
    const RESOURCES: usize = 4096;
    const WINDOW: usize = 128;
    const PAYLOAD: usize = 16 * 1024;
    const GROUPS: usize = 4;
    // The former cache retained all four groups until its unrelated global
    // capacity reset. This workload stays below that reset threshold. Payloads
    // model a resource owned by several groups, not four copies of its bytes.
    let run = |indexed: bool| {
        let mut old: FxHashMap<Vec<u64>, Rc<Vec<u8>>> = FxHashMap::default();
        let mut cache = DependencyCache::default();
        let mut live: Vec<Option<Rc<Vec<u8>>>> = vec![None; WINDOW];
        let mut weak: Vec<std::rc::Weak<Vec<u8>>> = Vec::with_capacity(RESOURCES);
        let start = std::time::Instant::now();
        for i in 0..RESOURCES {
            if i >= WINDOW {
                let retired = i - WINDOW;
                let id = 1000 + retired as u32 * 3;
                if indexed { assert_eq!(cache.invalidate(id), GROUPS); }
                live[i % WINDOW] = None;
                assert_eq!(weak[retired].upgrade().is_none(), indexed);
            }
            let id = 1000 + i as u32 * 3;
            let data = Rc::new(vec![i as u8; PAYLOAD]);
            weak.push(Rc::downgrade(&data));
            for group in 0..GROUPS {
                let key = vec![group as u64, id as u64, 123];
                if indexed {
                    cache.insert(key.clone(), data.clone(), vec![id, id + 1 + (group % 2) as u32, 10 + (group / 2) as u32]);
                    assert_eq!(cache.get(&key).unwrap()[0], i as u8);
                } else {
                    old.insert(key.clone(), data.clone());
                    assert_eq!(old.get(key.as_slice()).unwrap()[0], i as u8);
                }
            }
            live[i % WINDOW] = Some(data);
            if indexed {
                assert!(cache.len() <= WINDOW * GROUPS);
                assert!(cache.owner_count() <= WINDOW * 3 + 2);
            }
        }
        let elapsed = start.elapsed();
        let retained_resources = weak.iter().filter(|w| w.upgrade().is_some()).count();
        let logical_payload = retained_resources * PAYLOAD;
        let entries = if indexed { cache.len() } else { old.len() };
        if indexed {
            assert_eq!((entries, cache.owner_count(), cache.dependency_count()), (512, 386, 1536));
            // Capacity counts describe metadata, not RSS. Hash tables retain a
            // bounded high-water allocation after removals; payload handles and
            // per-entry owner vectors are removed immediately.
            let owner_vector_bytes: usize = cache.entries.values().map(|e| e.owners.capacity() * std::mem::size_of::<u32>()).sum();
            let shared_key_bytes: usize = cache.entries.keys().map(|k| k.len() * std::mem::size_of::<u64>()).sum();
            let reverse_key_capacity: usize = cache.owners.values().map(|keys| keys.capacity()).sum();
            eprintln!("HOST cache metadata for {entries} live groups: owner-vector capacity {owner_vector_bytes} bytes, shared-key payload {shared_key_bytes} bytes, reverse-index key capacity {reverse_key_capacity} Rc slots; entries/owners hash capacity {}/{}. Excludes hash control bytes, allocator rounding and Rc headers; not physical memory.", cache.entries.capacity(), cache.owners.capacity());
        }
        old.clear(); cache.clear(); drop(live);
        assert!(weak.iter().all(|w| w.upgrade().is_none()), "shutdown must release every modeled resource");
        assert_eq!((cache.len(), cache.owner_count(), cache.dependency_count()), (0, 0, 0));
        (elapsed, entries, retained_resources, logical_payload)
    };
    // Run sequentially so the before/after resource payloads never coexist.
    let before = run(false);
    let after = run(true);
    assert_eq!((before.1, before.2, before.3), (16384, 4096, 64 << 20));
    assert_eq!((after.1, after.2, after.3), (512, 128, 2 << 20));
    eprintln!("HOST synthetic streaming ownership: {RESOURCES} resources, {WINDOW} live, {GROUPS} groups/resource; baseline {} entries / {} owned logical payload bytes / {:.3} ms; indexed {} entries / {} owned logical payload bytes / {:.3} ms. Exact Rc ownership, no physical iPhone RAM/FPS claim; timings include fixture checks and allocation work, with no timing assertions.", before.1, before.3, before.0.as_secs_f64()*1000.0, after.1, after.3, after.0.as_secs_f64()*1000.0);
}
