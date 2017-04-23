use std::time::Duration;

#[derive(Copy, Clone)]
pub struct Config {
    pub min_connections: usize,
    pub max_connections: Option<usize>,
    pub min_idle_connections: Option<usize>,
    pub max_idle_connections: Option<usize>,
    pub connect_timeout: Option<Duration>,
    pub max_live_time: Option<Duration>,
    pub max_idle_time: Option<Duration>,
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
