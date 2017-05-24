use std::time::Instant;
use std::sync::atomic::{AtomicUsize, Ordering};

use crossbeam::sync::SegQueue;

use config::Config;

/// A connection, carrying with it a record of how long it has been live.
pub struct Live<T> {
    pub conn: T,
    pub live_since: Instant,
}

impl<T> Live<T> {
    pub fn new(conn: T) -> Live<T> {
        Live {
            conn: conn,
            live_since: Instant::now(),
        }
    }
}

/// An idle connection, carrying with it a record of how long it has been idle.
struct Idle<T> {
    conn: Live<T>,
    idle_since: Instant,
}

impl<T> Idle<T> {
    fn new(conn: Live<T>) -> Idle<T> {
        Idle {
            conn: conn,
            idle_since: Instant::now(),
        }
    }
}

/// A queue of idle connections which counts how many connections exist total
/// (including those which are not in the queue.)
pub struct Queue<C> {
    idle: SegQueue<Idle<C>>,
    idle_count: AtomicUsize,
    total_count: AtomicUsize,
}

impl<C> Queue<C> {
    /// Construct an empty queue with a certain capacity
    pub fn new() -> Queue<C> {
        Queue {
            idle: SegQueue::new(),
            idle_count: AtomicUsize::new(0),
            total_count: AtomicUsize::new(0),
        }
    }

    /// Count of idle connection in queue
    #[inline(always)]
    pub fn idle(&self) -> usize {
        self.idle_count.load(Ordering::SeqCst)
    }

    /// Count of total connections active
    #[inline(always)]
    pub fn total(&self) -> usize {
        self.total_count.load(Ordering::SeqCst)
    }

    /// Push a new connection into the queue (this will increment
    /// the total connection count).
    pub fn new_conn(&self, conn: Live<C>) {
        self.store(conn);
        self.increment();
    }

    /// Store a connection which has already been counted in the queue
    /// (this will NOT increment the total connection count).
    pub fn store(&self, conn: Live<C>) {
        self.idle_count.fetch_add(1, Ordering::SeqCst);
        self.idle.push(Idle::new(conn));
    }

    /// Get the longest-idle connection from the queue.
    pub fn get(&self) -> Option<Live<C>> {
        self.idle.try_pop().map(|Idle { conn, ..}| {
            self.idle_count.fetch_sub(1, Ordering::SeqCst);
            conn
        })
    }

    /// Increment the connection count without pushing a connection into the
    /// queue.
    #[inline(always)]
    pub fn increment(&self) {
        self.total_count.fetch_add(1, Ordering::SeqCst);
    }

    /// Decrement the connection count
    #[inline(always)]
    pub fn decrement(&self) {
        self.total_count.fetch_sub(1, Ordering::SeqCst);
    }

    /// Reap connections from the queue. This will reap connections which have
    /// been alive or idle longer than the configuration's max_live_time and
    /// max_idle_time.
    pub fn reap(&self, config: &Config) {
        if config.max_idle_time.is_some() || config.max_live_time.is_some() {
            let mut ctr = self.idle();
            while let Some(conn) = self.idle.try_pop() {
                let mut keep = true;

                if let Some(max) = config.max_idle_time {
                    if conn.idle_since.elapsed() > max {
                        keep = false;
                    }
                }

                if let Some(max) = config.max_live_time {
                    if conn.conn.live_since.elapsed() > max {
                        keep = false;
                    }
                }

                if keep {
                    self.idle.push(conn);
                } else {
                    self.decrement();
                    self.idle_count.fetch_sub(1, Ordering::SeqCst);
                }

                ctr -= 1;
                if ctr == 0 {
                    break
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::thread;
    use std::time::Duration;

    use config::Config;
    use super::*;

    #[test]
    fn new_conn() {
        let mut conns = Queue::empty(1);
        assert_eq!(conns.idle(), 0);
        assert_eq!(conns.total(), 0);
        conns.new_conn(Live::new(()));
        assert_eq!(conns.idle(), 1);
        assert_eq!(conns.total(), 1);
    }

    #[test]
    fn store() {
        let mut conns = Queue::empty(1);
        assert_eq!(conns.idle(), 0);
        assert_eq!(conns.total(), 0);
        conns.store(Live::new(()));
        assert_eq!(conns.idle(), 1);
        assert_eq!(conns.total(), 0);
    }

    #[test]
    fn get() {
        let mut conns = Queue::empty(1);
        assert!(conns.get().is_none());
        conns.new_conn(Live::new(()));
        assert!(conns.get().is_some());
        assert_eq!(conns.idle(), 0);
        assert_eq!(conns.total(), 1);
    }

    #[test]
    fn increment_and_decrement() {
        let mut conns: Queue<()>= Queue::empty(0);
        assert_eq!(conns.total(), 0);
        assert_eq!(conns.idle(), 0);
        conns.increment();
        assert_eq!(conns.total(), 1);
        assert_eq!(conns.idle(), 0);
        conns.decrement();
        assert_eq!(conns.total(), 0);
        assert_eq!(conns.idle(), 0);
    }

    #[test]
    fn reap_idle() {
        let max_idle = Duration::new(0, 100_000);
        let config = Config::new(0).max_idle_time(max_idle);
        
        let mut conns = Queue::empty(2);
        conns.new_conn(Live::new(()));
        thread::sleep(max_idle);
        conns.new_conn(Live::new((())));
        assert_eq!(conns.total(), 2);
        assert_eq!(conns.idle(), 2);
        conns.reap(&config);
        assert_eq!(conns.total(), 1);
        assert_eq!(conns.idle(), 1);
    }

    #[test]
    fn reap_live() {
        let max_live = Duration::new(0, 100_000);
        let config = Config::new(0).max_live_time(max_live);
        
        let mut conns = Queue::empty(2);
        conns.new_conn(Live::new(()));
        thread::sleep(max_live);
        conns.new_conn(Live::new((())));
        assert_eq!(conns.total(), 2);
        assert_eq!(conns.idle(), 2);
        conns.reap(&config);
        assert_eq!(conns.total(), 1);
        assert_eq!(conns.idle(), 1);
    }
}
