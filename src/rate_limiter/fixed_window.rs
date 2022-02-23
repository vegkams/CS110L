use std::collections::HashMap;
use std::net::IpAddr;
use super::RateLimiterAlgorithm;

pub struct FixedWindow {
    limit: usize,
    requests: HashMap<IpAddr, usize>,
}

impl FixedWindow {
    pub fn new(limit: usize) -> Self {
        FixedWindow {
            limit: limit,
            requests: HashMap::new(),
        }
    }
}

impl RateLimiterAlgorithm for FixedWindow {
    fn register_request(&mut self, addr: IpAddr) -> bool {
        let count = self.requests.entry(addr).or_insert(0);
        *count += 1;
        *count <= self.limit
    }

    fn refresh(&mut self) {
        self.requests.clear();
    }
}