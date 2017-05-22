use std::time::Duration;

/// The configuration for a connection pool.
#[derive(Copy, Clone, Deserialize)]
pub struct Config {
    /// The minimum number of connections this pool should maintain.
    #[serde(default = "min_connections")]
    pub min_connections: usize,

    /// The maximum number of connections this pool should maintain.
    #[serde(default = "max_connections")]
    pub max_connections: Option<usize>,

    /// The minimum number of idle connections this pool should maintain.
    #[serde(default = "min_idle_connections")]
    pub min_idle_connections: Option<usize>,

    /// The maximum number of idle connections this pool should maintain.
    #[serde(default = "max_idle_connections")]
    pub max_idle_connections: Option<usize>,

    /// How long to wait before timing out when trying to lease a new
    /// connection.
    #[serde(default = "connect_timeout")]
    pub connect_timeout: Option<Duration>,

    /// Maximum time to keep a connection alive.
    #[serde(default = "max_live_time")]
    pub max_live_time: Option<Duration>,

    /// Maximum time to keep a connection idle.
    #[serde(default = "max_idle_time")]
    pub max_idle_time: Option<Duration>,

    /// How frequently to run the reap job, which kills connections that have
    /// been alive or idle for too long, and then re-establishes the minimum
    /// number of connections.
    #[serde(default = "reap_frequency")]
    pub reap_frequency: Duration,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            min_connections: 10,
            max_connections: None,
            min_idle_connections: None,
            max_idle_connections: None,
            connect_timeout: Some(Duration::from_secs(30)),
            max_live_time: Some(Duration::from_secs(30 * 60)),
            max_idle_time: Some(Duration::from_secs(10 * 60)),
            reap_frequency: Duration::from_secs(30),
        }
    }
}

fn min_connections() -> usize { 10 }
fn max_connections() -> Option<usize> { None }
fn min_idle_connections() -> Option<usize> { None }
fn max_idle_connections() -> Option<usize> { None }
fn connect_timeout() -> Option<Duration> { Some(Duration::from_secs(30)) }
fn max_live_time() -> Option<Duration> { Some(Duration::from_secs(30 * 60)) }
fn max_idle_time() -> Option<Duration> { Some(Duration::from_secs(10 * 60)) }
fn reap_frequency() -> Duration { Duration::from_secs(30) }
