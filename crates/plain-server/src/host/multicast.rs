use std::collections::{HashSet, VecDeque};

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    acquire: bool,
    lease: String,
}

#[derive(Default)]
pub struct MulticastLeases {
    active: HashSet<String>,
    closed: VecDeque<String>,
}

impl MulticastLeases {
    pub fn without_platform_lock(&mut self, params: serde_json::Value) -> Result<bool, String> {
        let request: Request = serde_json::from_value(params).map_err(|error| error.to_string())?;
        if request.lease.is_empty() || request.lease.len() > 128 {
            return Err("Invalid multicast lease".into());
        }
        if request.acquire {
            if self.closed.contains(&request.lease) {
                return Ok(false);
            }
            self.active.insert(request.lease);
        } else {
            self.active.remove(&request.lease);
            if !self.closed.contains(&request.lease) {
                self.closed.push_back(request.lease);
                if self.closed.len() > 128 {
                    self.closed.pop_front();
                }
            }
        }
        Ok(true)
    }

    pub fn disconnect(&mut self) {
        self.active.clear();
    }
}

pub struct MulticastSession<'a>(&'a std::sync::Mutex<MulticastLeases>);

impl<'a> MulticastSession<'a> {
    pub fn new(leases: &'a std::sync::Mutex<MulticastLeases>) -> Self {
        Self(leases)
    }
}

impl Drop for MulticastSession<'_> {
    fn drop(&mut self) {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .disconnect();
    }
}

#[cfg(test)]
#[path = "../../tests/unit/host/multicast.rs"]
mod tests;
