use std::collections::HashSet;
use std::hash::Hash;
use std::sync::{Arc, RwLock};

pub struct HashCache<T> {
    data: Arc<RwLock<HashSet<T>>>,
}

impl<T> HashCache<T>
where
    T: Eq + Hash + Clone,
{
    pub fn new() -> Self {
        Self {
            data: Arc::new(RwLock::new(HashSet::new())),
        }
    }
    
    pub fn insert(&self, value: T) -> bool {
        let mut data = self.data.write().unwrap();
        data.insert(value)
    }
    
    pub fn contains(&self, value: &T) -> bool {
        let data = self.data.read().unwrap();
        data.contains(value)
    }
    
    pub fn remove(&self, value: &T) -> bool {
        let mut data = self.data.write().unwrap();
        data.remove(value)
    }
    
    pub fn len(&self) -> usize {
        let data = self.data.read().unwrap();
        data.len()
    }
    
    pub fn is_empty(&self) -> bool {
        let data = self.data.read().unwrap();
        data.is_empty()
    }
    
    pub fn clear(&self) {
        let mut data = self.data.write().unwrap();
        data.clear();
    }
}