use std::io;
use std::sync::Arc;

use crossbeam::sync::SegQueue;
use futures::{Future, Stream};
use futures::sync::oneshot;
use core::reactor::{Handle, Timeout, Interval};
use service::NewService;

use config::Config;
use queue::{Queue, Live};

pub struct InnerPool<C: NewService> {
    conns: Queue<C::Instance>,
    waiting: SegQueue<oneshot::Sender<Live<C::Instance>>>,
    handle: Handle,
    client: C,
    config: Config,
}

impl<C: NewService + 'static> InnerPool<C> where C::Future: 'static {
    pub fn new(conns: Queue<C::Instance>, handle: Handle, client: C, config: Config) -> InnerPool<C> {
        InnerPool {
            conns: conns,
            waiting: SegQueue::new(),
            handle: handle,
            client: client,
            config: config,
        }
    }

    /// Prepare the reap job to run on the event loop.
    pub fn prepare_reaper(this: &Arc<Self>) -> Result<(), io::Error> {
        let pool = this.clone();
        let reaper = Interval::new(this.config.reap_frequency, &pool.handle)?.for_each(move |_| {
            InnerPool::reap_and_replenish(&pool);
            Ok(())
        }).map_err(|_| ());
        this.handle.spawn(reaper);
        Ok(())
    }

    /// Create a new connection and store it in the pool.
    pub fn replenish_connection(&self, pool: Arc<Self>) {
        let spawn = self.new_connection().map_err(|_| ()).map(move |conn| {
            pool.increment();
            InnerPool::store(&pool, Live::new(conn))
        });
        self.handle.spawn(spawn);
    }

    /// Get a connection from the pool.
    pub fn get_connection(&self) -> Option<Live<C::Instance>> {
        self.conns.get()
    }

    /// Create and return a new connection.
    pub fn new_connection(&self) -> C::Future {
        self.client.new_service()
    }

    /// Prepare to notify this sender of an available connection.
    pub fn notify_of_connection(&self, tx: oneshot::Sender<Live<C::Instance>>) {
        self.waiting.push(tx);
    }

    /// The timeout for waiting on a new connection.
    pub fn connection_timeout(&self) -> Option<io::Result<Timeout>> {
        self.config.connect_timeout.map(|duration| Timeout::new(duration, &self.handle))
    }

    /// Receive a connection back to be stored in the pool. This could have one
    /// of three outcomes:
    /// * The connection will be released, if it should be released.
    /// * The connection will be passed to a waiting future, if any exist.
    /// * The connection will be put back into the connection pool.
    // XXX IMPORANT:
    // XXX  This function MUST be thread safe.
    // XXX  This means it cannot access either:
    // XXX      * self.handle
    // XXX      * self.client
    // XXX  These two fields are not guaranteed to be safe to access from threads
    // XXX  other than that of the event loop they're tied to.
    // XXX
    // XXX This invariant MUST be maintained to support the unsafe impl of Send
    // XXX for Conn.
    pub fn store(this: &Arc<Self>, mut conn: Live<C::Instance>) {
        // First attempt to send the conn to any waiting requests
        while let Some(waiting) = this.waiting.try_pop() {
            conn = match waiting.send(conn) {
                Ok(_)       => return,
                Err(conn)   => conn,
            };
        }

        // If there are no waiting connections, kill this connection or store
        // it.

        // Check if we are over the max idle connections limit
        if this.config.max_idle_connections.map_or(false, |max| max < this.conns.idle()) {
            return this.conns.decrement();
        }

        // Check if this connection has been alive too long
        if this.config.max_live_time.map_or(false, |max| max < conn.live_since.elapsed()) {
            return this.conns.decrement();
        }

        // Otherwise, store it.
        this.conns.store(conn);
    }

    /// Increment the connection count.
    pub fn increment(&self) {
        self.conns.increment();
    }

    pub fn reap_and_replenish(this: &Arc<Self>) {
        debug_assert!(this.total() >= this.idle(),
            "total ({}) < idle ({})", this.total(), this.idle());
        debug_assert!(this.max_conns().map_or(true, |max| this.total() <= max),
            "total ({}) > max_conns ({})", this.total(), this.max_conns().unwrap());
        debug_assert!(this.total() >= this.min_conns(),
            "total ({}) < min_conns ({})", this.total(), this.min_conns());
        this.reap();
        InnerPool::replenish(this);
    }

    /// Reap connections.
    fn reap(&self) {
        self.conns.reap(&self.config);
    }

    /// Replenish connections after finishing reaping.
    fn replenish(this: &Arc<Self>) {
        // Create connections (up to max) for each request waiting for notifications
        if let Some(max) = this.max_conns() {
            for _ in 0..(max - this.total()) {
                if let Some(waiting) = this.waiting.try_pop() {
                    let pool = this.clone();
                    let spawn = this.new_connection().map_err(|_| ()).map(move |conn| {
                        let conn = Live::new(conn);
                        if let Err(conn) = waiting.send(conn) {
                            InnerPool::store(&pool, conn)
                        }
                    });
                    this.handle.spawn(spawn);
                } else { break }
            }
        } else {
            while let Some(waiting) = this.waiting.try_pop() {
                let pool = this.clone();
                let spawn = this.new_connection().map_err(|_| ()).map(move |conn| {
                    let conn = Live::new(conn);
                    if let Err(conn) = waiting.send(conn) {
                        InnerPool::store(&pool, conn)
                    }
                });
                this.handle.spawn(spawn);
            }
        }

        // Create connections until we have the minimum number of connections
        for _ in this.total()..this.config.min_connections {
            this.replenish_connection(this.clone());
        }

        // Create connections until we have the minimum number of idle connections
        if let Some(min_idle_connections) = this.config.min_idle_connections {
            for _ in this.conns.idle()..min_idle_connections {
                this.replenish_connection(this.clone());
            }
        }
    }

    /// The total number of connections in the pool.
    pub fn total(&self) -> usize {
        self.conns.total()
    }

    /// The maximum connections allowed in the pool.
    pub fn max_conns(&self) -> Option<usize> {
        self.config.max_connections
    }

    /// The number of idle connections in the pool.
    fn idle(&self) -> usize {
        self.conns.idle()
    }

    /// The minimum connections allowed in the pool.
    fn min_conns(&self) -> usize {
        self.config.min_connections
    }
}
