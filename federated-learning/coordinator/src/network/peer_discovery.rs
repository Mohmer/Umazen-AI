//! Umazen Peer Discovery - Decentralized Network Node Management

#![forbid(unsafe_code)]
#![warn(
    missing_docs,
    trivial_casts,
    trivial_numeric_casts,
    unused_import_braces,
    unused_qualifications
)]

use {
    anchor_lang::{prelude::*, solana_program::pubkey::Pubkey},
    rand::{seq::SliceRandom, Rng},
    solana_program::clock::Clock,
    std::{
        collections::{HashMap, HashSet},
        net::{IpAddr, Ipv4Addr, SocketAddr},
        time::{Duration, Instant},
    },
};

/// Node information stored on-chain
#[account]
#[derive(Debug)]
pub struct NodeRegistry {
    pub version: u8,
    pub node_count: u32,
    pub last_cleanup: i64,
    pub nodes: Vec<NodeInfo>,
}

/// Individual node information
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct NodeInfo {
    pub pubkey: Pubkey,
    pub socket: SocketAddr,
    pub last_heartbeat: i64,
    pub capabilities: u64,
    pub stake_amount: u64,
    pub active: bool,
}

impl NodeRegistry {
    /// Initialize new node registry
    pub fn new() -> Self {
        Self {
            version: 1,
            node_count: 0,
            last_cleanup: 0,
            nodes: Vec::with_capacity(1000),
        }
    }

    /// Register new node
    pub fn register_node(&mut self, node: NodeInfo) -> Result<()> {
        if self.nodes.iter().any(|n| n.pubkey == node.pubkey) {
            return Err(ErrorCode::DuplicateNode.into());
        }

        if !valid_socket(node.socket) {
            return Err(ErrorCode::InvalidSocket.into());
        }

        self.nodes.push(node);
        self.node_count = self.nodes.len() as u32;
        Ok(())
    }

    /// Update node heartbeat
    pub fn update_heartbeat(&mut self, pubkey: &Pubkey) -> Result<()> {
        let clock = Clock::get()?;
        let node = self.nodes.iter_mut()
            .find(|n| n.pubkey == *pubkey)
            .ok_or(ErrorCode::NodeNotFound)?;

        node.last_heartbeat = clock.unix_timestamp;
        Ok(())
    }

    /// Get active nodes with filtering
    pub fn get_active_nodes(&self, filter: DiscoveryFilter) -> Vec<NodeInfo> {
        self.nodes.iter()
            .filter(|n| n.active && filter.matches(n))
            .cloned()
            .collect()
    }

    /// Cleanup inactive nodes
    pub fn cleanup_inactive(&mut self, max_age: i64) -> Result<()> {
        let clock = Clock::get()?;
        let now = clock.unix_timestamp;
        
        if now - self.last_cleanup < 300 {
            return Ok(());
        }

        self.nodes.retain(|n| now - n.last_heartbeat < max_age);
        self.node_count = self.nodes.len() as u32;
        self.last_cleanup = now;
        Ok(())
    }
}

/// Discovery filter parameters
#[derive(Debug, Clone)]
pub struct DiscoveryFilter {
    pub min_stake: u64,
    pub required_capabilities: u64,
    pub max_nodes: usize,
    pub exclude: HashSet<Pubkey>,
}

impl DiscoveryFilter {
    /// Create new discovery filter
    pub fn new(min_stake: u64, capabilities: u64) -> Self {
        Self {
            min_stake,
            required_capabilities: capabilities,
            max_nodes: 50,
            exclude: HashSet::new(),
        }
    }

    /// Check if node matches filter
    pub fn matches(&self, node: &NodeInfo) -> bool {
        node.stake_amount >= self.min_stake &&
        (node.capabilities & self.required_capabilities) == self.required_capabilities &&
        !self.exclude.contains(&node.pubkey)
    }
}

/// Peer discovery implementation
pub struct PeerDiscovery {
    registry: Box<Account<NodeRegistry>>,
    cache: HashMap<Pubkey, Instant>,
    rng: rand::rngs::ThreadRng,
}

impl PeerDiscovery {
    /// Discover peers with load balancing
    pub fn discover_peers(&mut self, filter: DiscoveryFilter) -> Vec<NodeInfo> {
        let mut candidates = self.registry.get_active_nodes(filter.clone());
        candidates.shuffle(&mut self.rng);
        candidates.truncate(filter.max_nodes);
        candidates
    }

    /// Select optimal peers using stake-weighted selection
    pub fn select_peers(&self, candidates: Vec<NodeInfo>, count: usize) -> Vec<NodeInfo> {
        let total_stake: u64 = candidates.iter().map(|n| n.stake_amount).sum();
        let mut selected = Vec::with_capacity(count);
        let mut rng = rand::thread_rng();

        for _ in 0..count {
            let mut pick = rng.gen_range(0..total_stake);
            for node in &candidates {
                if pick < node.stake_amount {
                    selected.push(node.clone());
                    break;
                }
                pick -= node.stake_amount;
            }
        }

        selected
    }

    /// Refresh local cache
    pub fn refresh_cache(&mut self) -> Result<()> {
        let now = Instant::now();
        self.cache.retain(|_, t| now - *t < Duration::from_secs(30));
        Ok(())
    }
}

/// Validation functions
fn valid_socket(socket: SocketAddr) -> bool {
    !socket.ip().is_unspecified() &&
    !socket.ip().is_multicast() &&
    socket.port() > 1024
}

#[error_code]
pub enum ErrorCode {
    #[msg("Duplicate node registration")]
    DuplicateNode,
    #[msg("Invalid socket address")]
    InvalidSocket,
    #[msg("Node not found")]
    NodeNotFound,
    #[msg("Stake below minimum requirement")]
    InsufficientStake,
    #[msg("Missing required capabilities")]
    MissingCapabilities,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::RngCore;

    fn mock_node(pubkey: Pubkey) -> NodeInfo {
        NodeInfo {
            pubkey,
            socket: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 8080),
            last_heartbeat: 0,
            capabilities: 0b111,
            stake_amount: 100,
            active: true,
        }
    }

    #[test]
    fn test_registration() {
        let mut registry = NodeRegistry::new();
        let node = mock_node(Pubkey::new_unique());
        registry.register_node(node.clone()).unwrap();
        assert_eq!(registry.node_count, 1);
    }

    #[test]
    fn test_duplicate_registration() {
        let mut registry = NodeRegistry::new();
        let node = mock_node(Pubkey::new_unique());
        registry.register_node(node.clone()).unwrap();
        let result = registry.register_node(node);
        assert_eq!(result, Err(ErrorCode::DuplicateNode.into()));
    }

    #[test]
    fn test_discovery_filter() {
        let filter = DiscoveryFilter::new(50, 0b101);
        let mut node = mock_node(Pubkey::new_unique());
        node.stake_amount = 100;
        node.capabilities = 0b101;
        
        assert!(filter.matches(&node));
    }
}
