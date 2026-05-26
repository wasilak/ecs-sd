use std::sync::Arc;

use chitchat::{ChitchatHandle, ChitchatId};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

/// Serializable DTO for routing table entries gossiped via `ecs_sd.routing.v1`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GossipProxyTarget {
    /// UUID as string (avoid uuid crate trait dependency issue).
    pub route_id: String,
    pub address: String,
}

/// Owns the `ChitchatHandle` and provides cluster-level helpers.
pub struct ClusterState {
    pub handle: ChitchatHandle,
    pub self_id: String,
}

impl ClusterState {
    /// Returns true if this node is the current leader.
    ///
    /// The leader is the live node with the lexicographically smallest `node_id`.
    /// If `live_nodes()` is empty, returns `false` (defensive — should not occur in practice).
    pub async fn is_leader(&self) -> bool {
        let chitchat = self.handle.chitchat();
        let cc = chitchat.lock().await;
        let min_live_id = cc.live_nodes().map(|id| id.node_id.as_str()).min();
        min_live_id == Some(self.self_id.as_str())
    }

    /// Publish serialized cache state to our own gossip node state.
    pub async fn publish_cache(&self, targets_json: &str) {
        let chitchat = self.handle.chitchat();
        let mut cc = chitchat.lock().await;
        cc.self_node_state().set("ecs_sd.cache.v1", targets_json);
    }

    /// Publish serialized routing table to our own gossip node state (proxy mode only).
    pub async fn publish_routing(&self, routing_json: &str) {
        let chitchat = self.handle.chitchat();
        let mut cc = chitchat.lock().await;
        cc.self_node_state().set("ecs_sd.routing.v1", routing_json);
    }

    /// Read the leader's cached targets from gossip.
    ///
    /// Finds the live node with the minimum `node_id`, then reads
    /// `ecs_sd.cache.v1` from that node's gossip state.
    pub async fn read_leader_cache(&self) -> Option<String> {
        let chitchat = self.handle.chitchat();
        let cc = chitchat.lock().await;
        let leader_id = cc.live_nodes().min_by_key(|id| &id.node_id)?;
        cc.node_state(leader_id)?
            .get("ecs_sd.cache.v1")
            .map(|s| s.to_string())
    }
}

/// Pure election logic: the leader is the lexicographically smallest node_id.
///
/// This is a private free function so it can be unit-tested without mocking chitchat.
fn elect_leader<'a>(live_node_ids: &[&'a str]) -> Option<&'a str> {
    // STUB: intentionally wrong for TDD RED phase
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leader_is_min_node_id() {
        assert_eq!(
            elect_leader(&["node-b", "node-a", "node-c"]),
            Some("node-a")
        );
    }

    #[test]
    fn single_node_is_leader() {
        assert_eq!(elect_leader(&["node-x"]), Some("node-x"));
    }

    #[test]
    fn empty_cluster_has_no_leader() {
        assert_eq!(elect_leader(&[]), None);
    }

    #[test]
    fn gossip_proxy_target_round_trips_json() {
        let original = GossipProxyTarget {
            route_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            address: "10.0.0.1:8080".to_string(),
        };

        let json = serde_json::to_string(&original).expect("serialize");
        let round_tripped: GossipProxyTarget =
            serde_json::from_str(&json).expect("deserialize");

        assert_eq!(round_tripped.route_id, original.route_id);
        assert_eq!(round_tripped.address, original.address);
    }
}
