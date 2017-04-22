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
    pub reap_frequency: Option<Duration>,
}

impl Config {
    pub fn new(min_connections: usize) -> Config {
        Config {
            min_connections: min_connections,
            max_connections: None,
            min_idle_connections: None,
            max_idle_connections: None,
            connect_timeout: None,
            max_live_time: None,
            max_idle_time: None,
            reap_frequency: None,
        }
    }

}

impl Default for Config {
    fn default() -> Config {
        Config {
            min_connections: 10,
            max_connections: Some(10),
            min_idle_connections: None,
            max_idle_connections: None,
            connect_timeout: Some(Duration::from_secs(30)),
            max_live_time: Some(Duration::from_secs(30 * 60)),
            max_idle_time: Some(Duration::from_secs(10 * 60)),
            reap_frequency: Some(Duration::from_secs(30)),
        }
    }
}

macro_rules! config {
    ($($flag:ident: $t:ty),*) => (
        impl Config {
            $(pub fn $flag(self, $flag: $t) -> Config {
                Config { $flag: Some($flag), ..self }
            })*
        }

        #[cfg(test)]
        mod tests {
            // Test that this flag exists
            $(#[test]
            #[allow(path_statements)]
            fn $flag() {
                super::Config::$flag;
            })*
        }
        
    );
}

config!(
    max_connections: usize,
    min_idle_connections: usize,
    max_idle_connections: usize,
    connect_timeout: Duration,
    max_live_time: Duration,
    max_idle_time: Duration,
    reap_frequency: Duration
);
