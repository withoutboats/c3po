use std::collections::VecDeque;
use std::mem;
use std::time::Instant;

use config::Config;

/// A connection, carrying with it a record of how long it has been live.
pub struct Conn<T> {
    pub conn: T,
    pub live_since: Instant,
}

impl<T> Conn<T> {
    pub fn new(conn: T) -> Conn<T> {
        Conn {
            conn: conn,
            live_since: Instant::now(),
        }
    }
}

/// An idle connection, carrying with it a record of how long it has been idle.
struct Idle<T> {
    conn: Conn<T>,
    idle_since: Instant,
}

impl<T> Idle<T> {
    fn new(conn: Conn<T>) -> Idle<T> {
        Idle {
            conn: conn,
            idle_since: Instant::now(),
        }
    }
}

/// A queue of idle connections which counts how many connections exist total
/// (including those which are not in the queue.)
pub struct ConnQueue<C> {
    idle: VecDeque<Idle<C>>,
    total_count: usize,
}

impl<C> ConnQueue<C> {
    /// Construct an empty queue with a certain capacity
    pub fn empty(capacity: usize) -> ConnQueue<C> {
        ConnQueue {
            idle: VecDeque::with_capacity(capacity),
            total_count: 0,
        }
    }

    /// Count of idle connection in queue
    #[inline(always)]
    pub fn idle(&self) -> usize {
        self.idle.len()
    }

    /// Count of total connections active
    #[inline(always)]
    pub fn total(&self) -> usize {
        self.total_count
    }

    /// Push a new connection into the queue (this will increment
    /// the total connection count).
    pub fn new_conn(&mut self, conn: Conn<C>) {
        self.store(conn);
        self.increment();
    }

    /// Store a connection which has already been counted in the queue
    /// (this will NOT increment the total connection count).
    pub fn store(&mut self, conn: Conn<C>) {
        self.idle.push_back(Idle::new(conn));
    }

    /// Get the longest-idle connection from the queue.
    pub fn get(&mut self) -> Option<Conn<C>> {
        self.idle.pop_front().map(|Idle { conn, ..}| conn)
    }

    /// Increment the connection count without pushing a connection into the
    /// queue.
    #[inline(always)]
    pub fn increment(&mut self) {
        self.total_count += 1;
    }

    /// Decrement the connection count
    #[inline(always)]
    pub fn decrement(&mut self) {
        self.total_count -= 1;
    }

    /// Reap connections from the queue. This will reap connections which have
    /// been alive or idle longer than the configuration's max_live_time and
    /// max_idle_time.
    pub fn reap(&mut self, config: &Config) {
        if config.max_idle_time.is_some() || config.max_live_time.is_some() {
            let new = VecDeque::with_capacity(self.idle());
            let conns = mem::replace(&mut self.idle, new);
            for conn in conns {
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
                    self.idle.push_back(conn);
                } else {
                    self.decrement();
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
        let mut conns = ConnQueue::empty(1);
        assert_eq!(conns.idle(), 0);
        assert_eq!(conns.total(), 0);
        conns.new_conn(Conn::new(()));
        assert_eq!(conns.idle(), 1);
        assert_eq!(conns.total(), 1);
    }

    #[test]
    fn store() {
        let mut conns = ConnQueue::empty(1);
        assert_eq!(conns.idle(), 0);
        assert_eq!(conns.total(), 0);
        conns.store(Conn::new(()));
        assert_eq!(conns.idle(), 1);
        assert_eq!(conns.total(), 0);
    }

    #[test]
    fn get() {
        let mut conns = ConnQueue::empty(1);
        assert!(conns.get().is_none());
        conns.new_conn(Conn::new(()));
        assert!(conns.get().is_some());
        assert_eq!(conns.idle(), 0);
        assert_eq!(conns.total(), 1);
    }

    #[test]
    fn increment_and_decrement() {
        let mut conns: ConnQueue<()>= ConnQueue::empty(0);
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
        
        let mut conns = ConnQueue::empty(2);
        conns.new_conn(Conn::new(()));
        thread::sleep(max_idle);
        conns.new_conn(Conn::new((())));
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
        
        let mut conns = ConnQueue::empty(2);
        conns.new_conn(Conn::new(()));
        thread::sleep(max_live);
        conns.new_conn(Conn::new((())));
        assert_eq!(conns.total(), 2);
        assert_eq!(conns.idle(), 2);
        conns.reap(&config);
        assert_eq!(conns.total(), 1);
        assert_eq!(conns.idle(), 1);
    }
}
