use std::collections::VecDeque;
use std::cell::RefCell;
use std::io;
use std::rc::Rc;

use futures::{Future, Stream};
use futures::unsync::oneshot;
use core::reactor::{Handle, Timeout, Interval};
use service::Connect;

use config::Config;
use queue::{Queue, Live};

pub struct InnerPool<C: Connect<Handle>> {
    conns: RefCell<Queue<C::Instance>>,
    waiting: RefCell<VecDeque<oneshot::Sender<Live<C::Instance>>>>,
    handle: Handle,
    client: C,
    config: Config,
}

impl<C: Connect<Handle> + 'static> InnerPool<C> where C::Future: 'static {
    pub fn new(conns: Queue<C::Instance>, handle: Handle, client: C, config: Config) -> InnerPool<C> {
        InnerPool {
            conns: RefCell::new(conns),
            waiting: RefCell::new(VecDeque::new()),
            handle: handle,
            client: client,
            config: config,
        }
    }

    /// Prepare the reap job to run on the event loop.
    pub fn prepare_reaper(this: &Rc<Self>) -> Result<(), io::Error> {
        if let Some(freq) = this.config.reap_frequency {
            let pool = this.clone();
            let reaper = Interval::new(freq, &pool.handle)?.for_each(move |_| {
                InnerPool::reap_and_replenish(&pool);
                Ok(())
            }).map_err(|_| ());
            this.handle.spawn(reaper);
        }
        Ok(())
    }

    /// Create a new connection and store it in the pool.
    pub fn replenish_connection(&self, pool: Rc<Self>) {
        let spawn = self.new_connection().map_err(|_| ()).map(move |conn| {
            pool.increment();
            InnerPool::store(&pool, Live::new(conn))
        });
        self.handle.spawn(spawn);
    }

    /// Get a connection from the pool.
    pub fn get_connection(&self) -> Option<Live<C::Instance>> {
        self.conns.borrow_mut().get()
    }

    /// Create and return a new connection.
    pub fn new_connection(&self) -> C::Future {
        self.client.connect(&self.handle)
    }

    /// Prepare to notify this sender of an available connection.
    pub fn notify_of_connection(&self, tx: oneshot::Sender<Live<C::Instance>>) {
        self.waiting.borrow_mut().push_back(tx);
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
    pub fn store(this: &Rc<Self>, conn: Live<C::Instance>) {
        // If this connection has been alive too long, release it
        if this.config.max_live_time.map_or(false, |max| conn.live_since.elapsed() <= max) {
            this.conns.borrow_mut().decrement();
            // Create a new connection if we've fallen below the minimum count
            if this.conns.borrow().total() < this.config.min_connections {
                this.replenish_connection(this.clone());
            }
        } else {
            // Otherwise, first attempt to send it to any waiting requests
            let mut conn = conn;
            while let Some(waiting) = this.waiting.borrow_mut().pop_front() {
                conn = match waiting.send(conn) {
                    Ok(_)       => return,
                    Err(conn)   => conn,
                };
            }
            // If there are no waiting requests & we aren't over the max idle
            // connections limit, attempt to store it back in the pool
            if this.config.max_idle_connections.map_or(false, |max| max <= this.conns.borrow().idle()) {
                this.conns.borrow_mut().store(conn);
            }
        }
    }

    /// Increment the connection count.
    pub fn increment(&self) {
        self.conns.borrow_mut().increment();
    }

    pub fn reap_and_replenish(this: &Rc<Self>) {
        debug_assert!(this.total() >= this.idle());
        debug_assert!(this.max_conns().map_or(true, |max| this.total() <= max));
        debug_assert!(this.total() >= this.min_conns());
        this.reap();
        InnerPool::replenish(this);
    }

    /// Reap connections.
    fn reap(&self) {
        self.conns.borrow_mut().reap(&self.config);
    }

    /// Replenish connections after finishing reaping.
    fn replenish(this: &Rc<Self>) {
        // Create connections (up to max) for each request waiting for notifications
        if let Some(max) = this.max_conns() {
            let mut waiting = this.waiting.borrow_mut();
            for waiting in waiting.drain(..).take(max - this.total()) {
                let pool = this.clone();
                let spawn = this.new_connection().map_err(|_| ()).map(move |conn| {
                    let conn = Live::new(conn);
                    if let Err(conn) = waiting.send(conn) {
                        InnerPool::store(&pool, conn)
                    }
                });
                this.handle.spawn(spawn);
            }
        } else {
            let mut waiting = this.waiting.borrow_mut();
            for waiting in waiting.drain(..) {
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
            for _ in this.conns.borrow().idle()..min_idle_connections {
                this.replenish_connection(this.clone());
            }
        }
    }

    /// The total number of connections in the pool.
    pub fn total(&self) -> usize {
        self.conns.borrow().total()
    }

    /// The maximum connections allowed in the pool.
    pub fn max_conns(&self) -> Option<usize> {
        self.config.max_connections
    }

    /// The number of idle connections in the pool.
    fn idle(&self) -> usize {
        self.conns.borrow().idle()
    }

    /// The minimum connections allowed in the pool.
    fn min_conns(&self) -> usize {
        self.config.min_connections
    }
}
