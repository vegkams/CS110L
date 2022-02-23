use std::net::IpAddr;

pub mod fixed_window;

#[derive(clap::ArgEnum, Debug, Clone)]
pub enum ArgRateLimiter {
    FixedWindow
}

pub trait RateLimiterAlgorithm: Send + Sync {
    fn register_request(&mut self, addr: IpAddr) -> bool;

    fn refresh(&mut self);
}