//! Poison-recovering synchronization guards and bounded concurrent map insertion.
//!
//! // ARCH: ARCHITECTURE.md §6 — Eliminates `.unwrap()` on `Mutex`/`RwLock` acquisitions and
//! // enforces bounded `DashMap` growth without holding shard iterators across `.remove()`.

use dashmap::DashMap;
use std::hash::Hash;
use std::sync::{Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};

/// Stateless synchronization and bounded concurrent collection guard utility.
#[derive(Debug, Clone, Copy, Default)]
pub struct OryxSyncGuard;

impl OryxSyncGuard {
    /// Acquires a [`Mutex`] lock, recovering the inner guard if a prior thread panicked while holding it.
    ///
    /// # Panics
    /// Never.
    pub fn lock_mutex_recovering_poison<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
        match mutex.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// Acquires a [`RwLock`] read guard, recovering the inner guard if poisoned.
    ///
    /// # Panics
    /// Never.
    pub fn read_rwlock_recovering_poison<T>(rwlock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
        match rwlock.read() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// Acquires a [`RwLock`] write guard, recovering the inner guard if poisoned.
    ///
    /// # Panics
    /// Never.
    pub fn write_rwlock_recovering_poison<T>(rwlock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
        match rwlock.write() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// Inserts `(key, value)` into `map` while enforcing `max_entries` capacity.
    ///
    /// If `max_entries == 0`, returns without inserting. When inserting a new key at capacity,
    /// snapshots one existing key and drops the `DashMap::Iter` shard read guard *before*
    /// calling `map.remove(&evict_key)` to prevent single-shard deadlocks.
    ///
    /// # Panics
    /// Never.
    pub fn insert_into_bounded_dashmap<K, V>(
        map: &DashMap<K, V>,
        max_entries: usize,
        key: K,
        value: V,
    ) where
        K: Eq + Hash + Clone,
    {
        if max_entries == 0 {
            return;
        }
        if !map.contains_key(&key) && map.len() >= max_entries {
            // Drop shard iterator guard before calling `remove` so the same shard cannot deadlock.
            let evict_key = map.iter().next().map(|entry| entry.key().clone());
            if let Some(victim) = evict_key {
                map.remove(&victim);
            }
        }
        map.insert(key, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_mutex_and_rwlock_recover_from_poison() {
        let m = Arc::new(Mutex::new(10_u32));
        let m_clone = Arc::clone(&m);
        let _ = std::thread::spawn(move || {
            if let Ok(mut g) = m_clone.lock() {
                *g = 42;
                std::panic::resume_unwind(Box::new("intentional poison test"));
            }
        })
        .join();
        let guard = OryxSyncGuard::lock_mutex_recovering_poison(&m);
        assert_eq!(*guard, 42);
        drop(guard);

        let rw = Arc::new(RwLock::new(7_u32));
        let rw_clone = Arc::clone(&rw);
        let _ = std::thread::spawn(move || {
            if let Ok(mut g) = rw_clone.write() {
                *g = 99;
                std::panic::resume_unwind(Box::new("intentional rwlock poison test"));
            }
        })
        .join();
        assert_eq!(*OryxSyncGuard::read_rwlock_recovering_poison(&rw), 99);
        {
            let mut wg = OryxSyncGuard::write_rwlock_recovering_poison(&rw);
            *wg = 100;
        }
        assert_eq!(*OryxSyncGuard::read_rwlock_recovering_poison(&rw), 100);
    }

    #[test]
    fn test_insert_into_bounded_dashmap_evicts_without_deadlock() {
        let map: DashMap<String, usize> = DashMap::new();
        OryxSyncGuard::insert_into_bounded_dashmap(&map, 0, "ignored".to_string(), 0);
        assert!(map.is_empty());

        for i in 0..10 {
            OryxSyncGuard::insert_into_bounded_dashmap(&map, 3, format!("k{i}"), i);
            assert!(map.len() <= 3);
        }
        assert_eq!(map.len(), 3);

        // Updating an existing key at capacity must not evict other keys.
        let existing = map.iter().next().map(|e| e.key().clone());
        if let Some(existing_key) = existing {
            OryxSyncGuard::insert_into_bounded_dashmap(&map, 3, existing_key.clone(), 999);
            assert_eq!(map.len(), 3);
            assert_eq!(map.get(&existing_key).map(|v| *v), Some(999));
        }
    }
}
