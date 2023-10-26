use std::{
    any::{Any, TypeId},
    collections::HashMap,
    sync::Arc,
};

use tokio::sync::RwLock;

/// The global bucket for a given test suite. This must match the one in the proc macro crate.
const GLOBAL_DEFAULT: usize = usize::MAX;

type StateInner =
    Arc<RwLock<HashMap<usize, HashMap<TypeId, Box<dyn Any + Send + Sync + 'static>>>>>;

/// Holds state during the execution of a test suite.
#[derive(Debug)]
pub struct State<const VERBOSE: bool> {
    state: StateInner,
}

impl<const VERBOSE: bool> Default for State<VERBOSE> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const VERBOSE: bool> State<VERBOSE> {
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(HashMap::from([(
                GLOBAL_DEFAULT,
                HashMap::new(),
            )]))),
        }
    }
    /// Removes item `T` from the bucket specified by `ID`.
    pub async fn remove<const ID: usize, T: 'static>(&self) -> Option<T> {
        self.state
            .write()
            .await
            .get_mut(&ID)
            .expect("local state not initialised")
            .remove(&TypeId::of::<T>())
            .map(|t| *t.downcast::<T>().expect("the impossible"))
    }

    /// Inserts item `T` to the bucket specified by `ID`.
    pub async fn insert<const ID: usize, T: Send + Sync + 'static>(&self, val: T) -> Option<T> {
        self.state
            .write()
            .await
            .get_mut(&ID)
            .expect("local state not initialised")
            .insert(TypeId::of::<T>(), Box::new(val))
            .map(|prev| *prev.downcast::<T>().expect("the impossible"))
    }

    /// Attempts to get `T` from the local test state for the given `ID`. If the `T` does not exist
    /// in the local state, falls back to the global state. Panics if `T` cannot be found in
    /// either.
    pub async fn get<const ID: usize, T: Clone + 'static>(&self) -> Option<T> {
        let map = self.state.read().await;

        let local_map = map.get(&ID).expect("local state not initialised");
        if let Some(item) = local_map.get(&TypeId::of::<T>()) {
            return Some(item.downcast_ref::<T>().cloned().expect("the impossible"));
        }

        if VERBOSE {
            println!("item not in local state, defaulting to global");
        }

        map.get(&GLOBAL_DEFAULT)
            .expect("state not initialised")
            .get(&TypeId::of::<T>())
            .map(|item| item.downcast_ref::<T>().cloned().expect("the impossible"))
    }

    /// Creates local test states for the given ids.
    pub async fn create_local_state<const N: usize>(&self, ids: [usize; N]) {
        let mut map = self.state.write().await;
        for id in ids {
            map.insert(id, HashMap::new());
        }
    }
}
