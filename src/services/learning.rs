use std::sync::Arc;

use crate::amas::engine::AMASEngine;
use crate::store::Store;

/// Coordinates learning workflows that need both Store and AMAS access.
#[derive(Clone)]
pub struct LearningService {
    store: Arc<Store>,
    amas: Arc<AMASEngine>,
}

impl LearningService {
    pub fn new(store: Arc<Store>, amas: Arc<AMASEngine>) -> Self {
        Self { store, amas }
    }

    pub fn store(&self) -> &Arc<Store> {
        &self.store
    }

    pub fn amas(&self) -> &Arc<AMASEngine> {
        &self.amas
    }
}
