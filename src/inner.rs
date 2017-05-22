use std::sync::Arc;

use crossbeam::sync::SegQueue;

use futures::{Future, Stream, IntoFuture};
use futures::sync::oneshot;
use core::reactor::{Remote, Timeout, Interval};
use service::NewService;

use config::Config;
use queue::{Queue, Live};

pub struct InnerPool<C: NewService> {
    conns: Queue<C::Instance>,
    waiting: SegQueue<oneshot::Sender<Live<C::Instance>>>,
    remote: Remote,
    client: C,
    config: Config,
}

impl<C: Send + Sync + NewService + 'static> InnerPool<C>
where
    C::Instance: Send,
    C::Future: Send + 'static,
{
    pub fn new(conns: Queue<C::Instance>, remote: Remote, client: C, config: Config) -> InnerPool<C> {
        InnerPool {
            conns: conns,
            waiting: SegQueue::new(),
            remote: remote,
            client: client,
            config: config,
        }
    }

    /// Prepare the reap job to run on the event loop.
    pub fn prepare_reaper(this: &Arc<Self>) {
        let pool = this.clone();
        this.remote.spawn(|handle| {
            Interval::new(pool.config.reap_frequency, handle).into_future().and_then(|interval| {
                interval.for_each(move |_| {
                    InnerPool::reap_and_replenish(&pool);
                    Ok(())
                })
            }).map_err(|_| ())
        });
    }

    /// Create a new connection and store it in the pool.
    pub fn replenish_connection(&self, pool: Arc<Self>) {
        let spawn = self.new_connection().map_err(|_| ()).map(move |conn| {
            pool.increment();
            InnerPool::store(&pool, Live::new(conn))
        });
        self.remote.spawn(|_| spawn);
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
    pub fn connection_timeout(&self) -> Option<oneshot::Receiver<()>> {
        self.config.connect_timeout.map(|duration| {
            let (tx, rx) = oneshot::channel();
            self.remote.spawn(move |handle| {
                Timeout::new(duration, handle).into_future().map_err(|_| ()).and_then(|timeout| {
                    timeout.map_err(|_| ()).and_then(move |_| tx.send(()))
                })
            });
            rx
        })
    }

    /// Receive a connection back to be stored in the pool. This could have one
    /// of three outcomes:
    /// * The connection will be released, if it should be released.
    /// * The connection will be passed to a waiting future, if any exist.
    /// * The connection will be put back into the connection pool.
    pub fn store(this: &Arc<Self>, conn: Live<C::Instance>) {
        // If this connection has been alive too long, release it
        if this.config.max_live_time.map_or(false, |max| conn.live_since.elapsed() >= max) {
            // Create a new connection if we've fallen below the minimum count
            if this.conns.total() - 1 < this.config.min_connections {
                this.replenish_connection(this.clone());
            } else {
                this.conns.decrement();
            }
        } else {
            // Otherwise, first attempt to send it to any waiting requests
            let mut conn = conn;
            while let Some(waiting) = this.waiting.try_pop() {
                conn = match waiting.send(conn) {
                    Ok(_)       => return,
                    Err(conn)   => conn,
                };
            }
            // If there are no waiting requests & we aren't over the max idle
            // connections limit, attempt to store it back in the pool
            if this.config.max_idle_connections.map_or(true, |max| max >= this.conns.idle()) {
                this.conns.store(conn);
            }
        }
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
            let mut ctr = max - this.total();
            while let Some(waiting) = this.waiting.try_pop() {
                let pool = this.clone();
                let spawn = this.new_connection().map_err(|_| ()).map(move |conn| {
                    let conn = Live::new(conn);
                    if let Err(conn) = waiting.send(conn) {
                        InnerPool::store(&pool, conn)
                    }
                });
                this.remote.spawn(|_| spawn);
                ctr -= 1;
                if ctr == 0 { break }
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
                this.remote.spawn(|_| spawn);
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
