use std::time::Duration;

#[derive(Copy, Clone, Deserialize)]
pub struct Config {
    #[serde(default = "min_connections")]
    pub min_connections: usize,
    #[serde(default = "max_connections")]
    pub max_connections: Option<usize>,
    #[serde(default = "min_idle_connections")]
    pub min_idle_connections: Option<usize>,
    #[serde(default = "max_idle_connections")]
    pub max_idle_connections: Option<usize>,
    #[serde(default = "connect_timeout")]
    pub connect_timeout: Option<Duration>,
    #[serde(default = "max_live_time")]
    pub max_live_time: Option<Duration>,
    #[serde(default = "max_idle_time")]
    pub max_idle_time: Option<Duration>,
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
