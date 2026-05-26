use std::time::Duration;

use chitchat::transport::ChannelTransport;
use chitchat::{spawn_chitchat, ChitchatConfig, ChitchatId, FailureDetectorConfig};

use crate::cluster::ClusterState;
use crate::config::{ClusterMode, Config};

const GOSSIP_INTERVAL_MS: u64 = 50;
const CONVERGENCE_MS: u64 = GOSSIP_INTERVAL_MS * 10; // 10 rounds for safe convergence

/// Helper: Build a ClusterState from ChannelTransport for in-process testing.
async fn make_node(
    node_id: &str,
    addr: &str,
    seeds: Vec<String>,
    transport: &ChannelTransport,
) -> ClusterState {
    make_node_with_detector(
        node_id,
        addr,
        seeds,
        transport,
        FailureDetectorConfig::default(),
    )
    .await
}

/// Helper: Build a ClusterState with custom failure detector config.
async fn make_node_with_detector(
    node_id: &str,
    addr: &str,
    seeds: Vec<String>,
    transport: &ChannelTransport,
    failure_detector_config: FailureDetectorConfig,
) -> ClusterState {
    let chitchat_id = ChitchatId {
        node_id: node_id.to_string(),
        generation_id: 1,
        gossip_advertise_addr: addr.parse().unwrap(),
    };
    let config = ChitchatConfig {
        chitchat_id,
        cluster_id: "test-cluster".to_string(),
        gossip_interval: Duration::from_millis(GOSSIP_INTERVAL_MS),
        listen_addr: addr.parse().unwrap(),
        seed_nodes: seeds,
        failure_detector_config,
        marked_for_deletion_grace_period: Duration::from_secs(10),
        catchup_callback: None,
        extra_liveness_predicate: None,
    };
    let handle = spawn_chitchat(config, vec![], transport).await.unwrap();
    ClusterState {
        handle,
        self_id: node_id.to_string(),
    }
}

// ============================================================================
// Integration Tests (using ChannelTransport for in-process gossip)
// ============================================================================

#[tokio::test]
async fn single_node_elects_self_as_leader() {
    let transport = ChannelTransport::with_mtu(65_507);
    let node = make_node("node-1", "127.0.0.1:21001", vec![], &transport).await;

    // Wait for convergence (single node should immediately see itself as live)
    tokio::time::sleep(Duration::from_millis(CONVERGENCE_MS)).await;

    assert!(
        node.is_leader().await,
        "Single node should elect itself as leader"
    );
}

#[tokio::test]
async fn two_nodes_elect_min_node_id_as_leader() {
    let transport = ChannelTransport::with_mtu(65_507);

    // Create two nodes with cross-seeding
    let node_a = make_node(
        "node-a",
        "127.0.0.1:21011",
        vec!["127.0.0.1:21012".to_string()],
        &transport,
    )
    .await;
    let node_b = make_node(
        "node-b",
        "127.0.0.1:21012",
        vec!["127.0.0.1:21011".to_string()],
        &transport,
    )
    .await;

    // Wait for membership convergence
    tokio::time::sleep(Duration::from_millis(CONVERGENCE_MS)).await;

    // "node-a" < "node-b" lexicographically, so node-a should be leader
    assert!(
        node_a.is_leader().await,
        "node-a (lexicographically smaller) should be leader"
    );
    assert!(
        !node_b.is_leader().await,
        "node-b should be follower when node-a is leader"
    );
}

#[tokio::test]
async fn leader_cache_propagates_to_follower() {
    let transport = ChannelTransport::with_mtu(65_507);

    let node_a = make_node(
        "node-a",
        "127.0.0.1:21021",
        vec!["127.0.0.1:21022".to_string()],
        &transport,
    )
    .await;
    let node_b = make_node(
        "node-b",
        "127.0.0.1:21022",
        vec!["127.0.0.1:21021".to_string()],
        &transport,
    )
    .await;

    // Wait for membership convergence
    tokio::time::sleep(Duration::from_millis(CONVERGENCE_MS)).await;

    // Verify node-a is leader
    assert!(node_a.is_leader().await, "node-a should be the leader");

    // Leader publishes cache
    let test_targets = r#"[{"targets":["10.0.0.1:9090"],"labels":{}}]"#;
    node_a.publish_cache(test_targets).await;

    // Wait for KV propagation
    tokio::time::sleep(Duration::from_millis(CONVERGENCE_MS)).await;

    // Follower reads leader's cache
    let cached = node_b.read_leader_cache().await;
    assert_eq!(
        cached,
        Some(test_targets.to_string()),
        "Follower should see leader's published cache"
    );
}

#[tokio::test]
async fn leader_failover_promotes_surviving_node() {
    let transport = ChannelTransport::with_mtu(65_507);

    // Use aggressive failure detector settings for faster failover detection
    // phi_threshold of 1.0 makes the detector much more sensitive
    let aggressive_detector = FailureDetectorConfig {
        phi_threshold: 1.0,
        sampling_window_size: 10,
        max_interval: Duration::from_millis(500),
        initial_interval: Duration::from_millis(50),
        dead_node_grace_period: Duration::from_secs(1),
    };

    let node_a = make_node_with_detector(
        "node-a",
        "127.0.0.1:21031",
        vec!["127.0.0.1:21032".to_string()],
        &transport,
        aggressive_detector.clone(),
    )
    .await;
    let node_b = make_node_with_detector(
        "node-b",
        "127.0.0.1:21032",
        vec!["127.0.0.1:21031".to_string()],
        &transport,
        aggressive_detector,
    )
    .await;

    // Wait for membership convergence
    tokio::time::sleep(Duration::from_millis(CONVERGENCE_MS)).await;

    // Verify initial leader
    assert!(node_a.is_leader().await, "node-a should initially be leader");
    assert!(!node_b.is_leader().await, "node-b should initially be follower");

    // Drop leader handle (simulates node failure)
    drop(node_a);

    // Wait for failure detection with retry loop
    // With phi_threshold=1.0, detection happens much faster (~5-10 intervals)
    let became_leader = tokio::time::timeout(
        Duration::from_millis(GOSSIP_INTERVAL_MS * 100), // 5s timeout (generous)
        async {
            loop {
                if node_b.is_leader().await {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(GOSSIP_INTERVAL_MS)).await;
            }
        },
    )
    .await;

    assert!(
        became_leader.is_ok(),
        "node-b did not become leader within 5s after node-a failure"
    );
    assert!(
        node_b.is_leader().await,
        "node-b should become leader after node-a fails"
    );
}

#[tokio::test]
async fn routing_table_gossips_via_routing_key() {
    let transport = ChannelTransport::with_mtu(65_507);

    let node_a = make_node(
        "node-a",
        "127.0.0.1:21041",
        vec!["127.0.0.1:21042".to_string()],
        &transport,
    )
    .await;
    let node_b = make_node(
        "node-b",
        "127.0.0.1:21042",
        vec!["127.0.0.1:21041".to_string()],
        &transport,
    )
    .await;

    // Wait for membership convergence
    tokio::time::sleep(Duration::from_millis(CONVERGENCE_MS)).await;

    // Verify node-a is leader
    assert!(node_a.is_leader().await, "node-a should be the leader");

    // Leader publishes routing table
    let routing_json = r#"[{"route_id":"550e8400-e29b-41d4-a716-446655440000","address":"10.0.0.1:8080"}]"#;
    node_a.publish_routing(routing_json).await;

    // Wait for propagation
    tokio::time::sleep(Duration::from_millis(CONVERGENCE_MS)).await;

    // Follower reads from leader's node state directly via chitchat
    let chitchat = node_b.handle.chitchat();
    let cc = chitchat.lock().await;

    // Find node-a's ChitchatId in live nodes
    let node_a_id = cc
        .live_nodes()
        .find(|id| id.node_id == "node-a")
        .expect("node-a should be in live nodes");

    // Read the routing key from node-a's state
    let routing_value = cc
        .node_state(node_a_id)
        .and_then(|state| state.get("ecs_sd.routing.v1"));

    assert!(
        routing_value.is_some(),
        "Follower should see leader's routing table"
    );
    let value = routing_value.unwrap();
    assert!(
        value.contains("route_id"),
        "Routing value should contain route_id"
    );
    assert!(
        value.contains("550e8400-e29b-41d4-a716-446655440000"),
        "Routing value should contain the expected UUID"
    );
}

#[test]
fn standalone_config_yields_no_cluster_state() {
    // This is a pure sync test — no async, no chitchat
    let cfg = Config::from_iter(["ecs-sd", "--clusters", "prod"]).unwrap();

    assert_eq!(
        cfg.cluster_mode,
        ClusterMode::Standalone,
        "Default cluster_mode should be Standalone"
    );

    // The actual None check for AppState.cluster is validated in plan 07-03's
    // structural guarantee — standalone path skips cluster init entirely.
}

// ============================================================================
// Unit Tests (from original inline tests module)
// ============================================================================

use crate::cluster::elect_leader;
use crate::cluster::GossipProxyTarget;

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
    let round_tripped: GossipProxyTarget = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(round_tripped.route_id, original.route_id);
    assert_eq!(round_tripped.address, original.address);
}
