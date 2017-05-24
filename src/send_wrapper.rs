use std::sync::Arc;

use service::NewService;

use inner::InnerPool;
use queue::Live;

pub struct PoolSendWrapper<C: NewService + 'static>(Arc<InnerPool<C>>);

impl<C: NewService + 'static> PoolSendWrapper<C> {
    pub fn new(inner: Arc<InnerPool<C>>) -> Self {
        PoolSendWrapper(inner)
    }

    pub fn store(&self, conn: Live<C::Instance>) {
        InnerPool::store(&self.0, conn)
    }
}

unsafe impl<C: NewService + 'static> Send for PoolSendWrapper<C>
where C::Instance: Send { }
