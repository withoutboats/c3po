extern crate futures;
extern crate tokio_core as core;
extern crate tokio_service as service;

#[macro_use] extern crate serde_derive;

mod config;
mod queue;
mod inner;

use std::io;
use std::iter;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;

use futures::{future, stream, Future, Stream};
use futures::unsync::oneshot;
use core::reactor::Handle;
use service::NewService;

pub use config::Config;

use queue::{Queue, Live};
use inner::InnerPool;

/// Future yielded by `Pool::connection`. Optimized not to allocate when
/// pulling an idle future out of the pool.
pub type ConnFuture<T, E> = future::Either<future::FutureResult<T, E>, Box<Future<Item = T, Error = E>>>;

/// A smart wrapper around a connection which stores it back in the pool
/// when it is dropped.
pub struct Conn<C: NewService + 'static> {
    conn: Option<Live<C::Instance>>,
    pool: Rc<InnerPool<C>>,
}
impl<C: NewService + 'static> Deref for Conn<C> {
    type Target = C::Instance;
    fn deref(&self) -> &Self::Target {
        &self.conn.as_ref().unwrap().conn
    }
}

impl<C: NewService + 'static> DerefMut for Conn<C> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.conn.as_mut().unwrap().conn
    }
}

impl<C: NewService + 'static> Drop for Conn<C> {
    fn drop(&mut self) {
        let conn = self.conn.take().unwrap();
        InnerPool::store(&self.pool, conn)
    }
}

/// An asynchronous, single-threaded connection pool.
///
/// This pool stores connections for re-use according to the policies set by the
/// `Config` type. It uses an asynchronous tokio event loop, and performs the
/// connections asynchronously. It can take any type of connection which implements
/// a tokio  protocol.
///
/// The first type parameter is the protocol for the clients produced by this pool.
/// The second parameter is the Kind type, usually found in tokio_proto, which is
/// used to distinguish pipelined and mutiplexed connections.
pub struct Pool<C: NewService> {
    inner: Rc<InnerPool<C>>,
}

impl<C: NewService> Clone for Pool<C> {
    fn clone(&self) -> Self {
        Pool { inner: self.inner.clone() }
    }
}

impl<C: NewService + 'static> Pool<C> {
    /// Construct a new pool. This returns a future, because it will attempt to
    /// establish the minimum number of connections immediately.
    ///
    /// This takes an address and a protocol for establishing connections, a
    /// handle to an event loop to run those connections on, and a configuration
    /// object to control its policy.
    pub fn new(client: C, handle: Handle, config: Config)
        -> Box<Future<Item = Pool<C>, Error = io::Error>>
    {

        // Establish the minimum number of connections (in an unordered stream)
        let conns = stream::futures_unordered(iter::repeat(&client)
                                                    .take(config.min_connections)
                                                    .map(|c| c.new_service()));

        // Fold the connections we are creating into a Queue object
        let count = config.max_connections.unwrap_or(config.min_connections);
        let conns = conns.fold::<_, _, io::Result<_>>(Queue::empty(count), |mut conns, conn| {
            conns.new_conn(Live::new(conn));
            Ok(conns)
        });
        
        // Set up the pool once the connections are established
        Box::new(conns.and_then(move |conns| {
            let pool = Rc::new(InnerPool::new(conns, handle, client, config));

            // Prepare a repear task to run (if configured to reap)
            InnerPool::prepare_reaper(&pool)?;

            Ok(Pool { inner: pool })
        }))
    }

    /// Yield a connection from the pool.
    ///
    /// In the happy path, this future will evaluate immediately and perform no
    /// allocations - it just pulls an idle connection out of the pool.
    ///
    /// In the less happy path, depending on the state of the pool and your
    /// configurations, it may do one of several things:
    ///
    /// * It may attempt to establish a new connection
    /// * It may wait for a connection to become available and use that one.
    /// * It may or may not timeout while waiting, depending on your configuration.
    ///
    /// Once the connection this future yields is dropped, it will be returned the pool.
    /// During storage, the connection may be released according to your configuration.
    /// Otherwise, it will prioritize giving the connection to a waiting request and only
    /// if there are none return it to the queue inside the pool.
    pub fn connection<E: From<io::Error>>(&self) -> ConnFuture<Conn<C>, E> {
        // If an idle connection is available in the case, return immediately (happy path)
        if let Some(conn) = self.inner.get_connection() {
            future::Either::A(future::ok(Conn {
                conn: Some(conn),
                pool: self.inner.clone(),
            }))
        } else {
            // Otherwise, we need to wait for a connection to free up.

            // Error to indicate connection failure.
            #[derive(Debug)]
            struct ConnectError;

            use std::{fmt, error};

            impl error::Error for ConnectError {
                fn description(&self) -> &'static str { "Connection attempt timed out" }
            }

            impl fmt::Display for ConnectError {
                fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
                    write!(f, "Connection attempt timed out")
                }
            }

            //Have the pool notify us of the connection
            let (tx, rx) = oneshot::channel();
            self.inner.notify_of_connection(tx);

            // Prepare the future which will wait for a free connection (may or may not
            // have a timeout)
            let pool = self.inner.clone();
            if let Some(Ok(timeout)) = self.inner.connection_timeout() {

                let timeout = timeout.then(|_| future::err(ConnectError));
                let rx = rx.map_err(|_| ConnectError).select(timeout);
                future::Either::B(Box::new(rx.map(|(conn, _)| {
                    Conn {
                        conn: Some(conn),
                        pool: pool,
                    }
                }).map_err(|(err, _)| E::from(io::Error::new(io::ErrorKind::TimedOut, err)))))
            } else {
                future::Either::B(Box::new(rx.map(|conn| {
                    Conn {
                        conn: Some(conn),
                        pool: pool,
                    }
                }).map_err(|_| E::from(io::Error::new(io::ErrorKind::TimedOut, ConnectError)))))
            }
        } 
    }
}
