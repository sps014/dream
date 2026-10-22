use super::state::{Config, error};
use crate::socket_operations::Failure;
use hyper::http::request::Parts;
use ipnet::IpNet;
use std::net::{IpAddr, SocketAddr};

/// Peers whose `X-Forwarded-*` headers are believed, from `forwarded_allow_ips`.
pub(super) enum Trusted {
    Any,
    Networks(Vec<IpNet>),
}
impl Trusted {
    /// Parses a comma-separated list of addresses and CIDR networks; `*` trusts every peer.
    pub fn parse(text: &str) -> Result<Self, Failure> {
        let mut networks = Vec::new();
        for entry in text.split(',').map(str::trim).filter(|e| !e.is_empty()) {
            if entry == "*" {
                return Ok(Self::Any);
            }
            let network = entry
                .parse::<IpNet>()
                .or_else(|_| entry.parse::<IpAddr>().map(IpNet::from))
                .map_err(|_| error("EINVAL", format!("invalid forwarded_allow_ips entry {entry}")))?;
            networks.push(network);
        }
        Ok(Self::Networks(networks))
    }
    fn contains(&self, address: IpAddr) -> bool {
        match self {
            Self::Any => true,
            Self::Networks(networks) => networks.iter().any(|n| n.contains(&address)),
        }
    }
    fn contains_host(&self, host: &str) -> bool {
        host.parse::<IpAddr>()
            .is_ok_and(|address| self.contains(address.to_canonical()))
    }
}

/// The request's client as Uvicorn reports it: the TCP peer, or the proxy-supplied client and
/// scheme when the peer is a trusted proxy.
pub(super) struct Client {
    pub host: String,
    pub port: u16,
    pub scheme: &'static str,
}

pub(super) fn resolve(head: &Parts, peer: SocketAddr, secure: bool, config: &Config) -> Client {
    let address = peer.ip().to_canonical();
    let mut client = Client {
        host: address.to_string(),
        port: peer.port(),
        scheme: if secure { "https" } else { "http" },
    };
    if !config.proxy_headers || !config.forwarded_allow.contains(address) {
        return client;
    }
    if let Some(proto) = values(head, "x-forwarded-proto").last() {
        match proto.to_ascii_lowercase().as_str() {
            "http" | "ws" => client.scheme = "http",
            "https" | "wss" => client.scheme = "https",
            _ => {}
        }
    }
    let hops = values(head, "x-forwarded-for");
    // Proxies append, so the nearest untrusted hop is the client; a chain of only trusted
    // proxies names its first entry.
    let chosen = hops
        .iter()
        .rev()
        .find(|hop| !config.forwarded_allow.contains_host(hop))
        .or(hops.first());
    if let Some(hop) = chosen {
        client.host = (*hop).to_owned();
        client.port = 0;
    }
    client
}

fn values<'a>(head: &'a Parts, name: &str) -> Vec<&'a str> {
    head.headers
        .get_all(name)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect()
}
