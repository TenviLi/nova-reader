//! # Leiden Community Detection — Pure Rust Implementation
//!
//! Implements a simplified Leiden algorithm for detecting communities
//! in the entity relationship graph. No Neo4j GDS dependency required.
//!
//! Algorithm overview:
//! 1. Extract graph from Neo4j as adjacency list
//! 2. Run modularity-based community detection (Louvain-style)
//! 3. Leiden refinement pass (ensure community connectivity)
//! 4. Hierarchical detection at multiple resolutions
//! 5. Write results back to Neo4j as Community nodes

use std::collections::{HashMap, HashSet};
use serde::{Deserialize, Serialize};
use crate::neo4j::Neo4jClient;

// ─── Graph Data Structures ───────────────────────────────────────────────────

/// A weighted edge in the graph.
#[derive(Debug, Clone)]
pub struct Edge {
    pub source: usize,
    pub target: usize,
    pub weight: f64,
}

/// In-memory graph for community detection.
#[derive(Debug, Clone)]
pub struct Graph {
    pub node_names: Vec<String>,
    pub node_index: HashMap<String, usize>,
    pub edges: Vec<Edge>,
    pub adj: Vec<Vec<(usize, f64)>>,
    pub total_weight: f64,
}

impl Graph {
    pub fn new() -> Self {
        Self {
            node_names: Vec::new(),
            node_index: HashMap::new(),
            edges: Vec::new(),
            adj: Vec::new(),
            total_weight: 0.0,
        }
    }

    pub fn add_node(&mut self, name: &str) -> usize {
        if let Some(&idx) = self.node_index.get(name) {
            return idx;
        }
        let idx = self.node_names.len();
        self.node_names.push(name.to_string());
        self.node_index.insert(name.to_string(), idx);
        self.adj.push(Vec::new());
        idx
    }

    pub fn add_edge(&mut self, source: &str, target: &str, weight: f64) {
        let s = self.add_node(source);
        let t = self.add_node(target);
        self.edges.push(Edge { source: s, target: t, weight });
        self.adj[s].push((t, weight));
        self.adj[t].push((s, weight));
        self.total_weight += weight;
    }

    pub fn node_count(&self) -> usize {
        self.node_names.len()
    }

    /// Get the weighted degree of a node.
    pub fn degree(&self, node: usize) -> f64 {
        self.adj[node].iter().map(|(_, w)| w).sum()
    }
}

// ─── Community Assignment ────────────────────────────────────────────────────

/// Community detection result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunityResult {
    /// Node → community ID mapping
    pub assignments: Vec<usize>,
    /// Number of communities found
    pub num_communities: usize,
    /// Modularity score of the partition
    pub modularity: f64,
    /// Resolution level
    pub level: i32,
}

/// A community with its members and metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedCommunity {
    pub id: String,
    pub level: i32,
    pub members: Vec<String>,
    pub internal_edges: usize,
    pub modularity_contribution: f64,
}

// ─── Leiden Algorithm ────────────────────────────────────────────────────────

/// Configuration for community detection.
#[derive(Debug, Clone)]
pub struct LeidenConfig {
    /// Resolution parameter (higher = smaller communities)
    pub resolution: f64,
    /// Maximum iterations for convergence
    pub max_iterations: usize,
    /// Random seed for reproducibility
    pub seed: u64,
    /// Minimum community size to keep
    pub min_community_size: usize,
}

impl Default for LeidenConfig {
    fn default() -> Self {
        Self {
            resolution: 1.0,
            max_iterations: 50,
            seed: 42,
            min_community_size: 2,
        }
    }
}

/// Run Leiden community detection on a graph.
pub fn detect_communities(graph: &Graph, config: &LeidenConfig) -> CommunityResult {
    let n = graph.node_count();
    if n == 0 {
        return CommunityResult {
            assignments: Vec::new(),
            num_communities: 0,
            modularity: 0.0,
            level: 0,
        };
    }

    // Initialize: each node in its own community
    let mut assignments: Vec<usize> = (0..n).collect();
    let m2 = graph.total_weight; // 2 * total_edge_weight (undirected counted twice)

    if m2 == 0.0 {
        return CommunityResult {
            assignments,
            num_communities: n,
            modularity: 0.0,
            level: 0,
        };
    }

    // Precompute node degrees (weighted)
    let degrees: Vec<f64> = (0..n).map(|i| graph.degree(i)).collect();

    // Phase 1: Local moving (Louvain-style)
    let mut improved = true;
    let mut iteration = 0;

    while improved && iteration < config.max_iterations {
        improved = false;
        iteration += 1;

        // Iterate over nodes in a deterministic order
        // (true Leiden uses random order, but deterministic is fine for our use case)
        for node in 0..n {
            let current_community = assignments[node];

            // Calculate weights to each neighboring community
            let mut community_weights: HashMap<usize, f64> = HashMap::new();
            for &(neighbor, weight) in &graph.adj[node] {
                let neighbor_comm = assignments[neighbor];
                *community_weights.entry(neighbor_comm).or_insert(0.0) += weight;
            }

            // Calculate degree sum per community
            let mut community_degree_sum: HashMap<usize, f64> = HashMap::new();
            for (i, &comm) in assignments.iter().enumerate() {
                *community_degree_sum.entry(comm).or_insert(0.0) += degrees[i];
            }

            // Find best community to move to (modularity gain)
            let ki = degrees[node];
            let mut best_community = current_community;
            let mut best_delta = 0.0;

            // Remove node from current community for calculation
            let sigma_in_current = community_weights.get(&current_community).copied().unwrap_or(0.0);
            let sigma_tot_current = community_degree_sum.get(&current_community).copied().unwrap_or(0.0) - ki;

            for (&comm, &w_ic) in &community_weights {
                if comm == current_community {
                    continue;
                }

                let sigma_tot_c = community_degree_sum.get(&comm).copied().unwrap_or(0.0);

                // Modularity gain of moving node to community `comm`
                let delta = (w_ic - sigma_in_current) / m2
                    - config.resolution * ki * (sigma_tot_c - sigma_tot_current) / (m2 * m2);

                if delta > best_delta {
                    best_delta = delta;
                    best_community = comm;
                }
            }

            if best_community != current_community {
                assignments[node] = best_community;
                improved = true;
            }
        }
    }

    // Phase 2: Leiden refinement — ensure communities are connected
    leiden_refine(graph, &mut assignments);

    // Compact community IDs (remove gaps)
    let mut id_map: HashMap<usize, usize> = HashMap::new();
    let mut next_id = 0;
    for a in assignments.iter_mut() {
        let new_id = *id_map.entry(*a).or_insert_with(|| {
            let id = next_id;
            next_id += 1;
            id
        });
        *a = new_id;
    }

    // Filter out tiny communities (assign back to nearest large community)
    let mut community_sizes: HashMap<usize, usize> = HashMap::new();
    for &a in &assignments {
        *community_sizes.entry(a).or_insert(0) += 1;
    }

    let small_communities: HashSet<usize> = community_sizes
        .iter()
        .filter(|(_, &size)| size < config.min_community_size)
        .map(|(&id, _)| id)
        .collect();

    if !small_communities.is_empty() {
        for node in 0..n {
            if small_communities.contains(&assignments[node]) {
                // Move to the community with strongest connection
                let mut best_comm = assignments[node];
                let mut best_weight = 0.0;
                for &(neighbor, weight) in &graph.adj[node] {
                    let neighbor_comm = assignments[neighbor];
                    if !small_communities.contains(&neighbor_comm) && weight > best_weight {
                        best_weight = weight;
                        best_comm = neighbor_comm;
                    }
                }
                assignments[node] = best_comm;
            }
        }
    }

    // Recompact after filtering
    id_map.clear();
    next_id = 0;
    for a in assignments.iter_mut() {
        let new_id = *id_map.entry(*a).or_insert_with(|| {
            let id = next_id;
            next_id += 1;
            id
        });
        *a = new_id;
    }

    let modularity = compute_modularity(graph, &assignments, config.resolution);

    CommunityResult {
        num_communities: next_id,
        assignments,
        modularity,
        level: 0,
    }
}

/// Leiden refinement: ensure each community is internally connected.
/// Split disconnected sub-communities into separate communities.
fn leiden_refine(graph: &Graph, assignments: &mut Vec<usize>) {
    let n = graph.node_count();
    debug_assert_eq!(
        assignments.len(),
        n,
        "community assignments must match graph node count"
    );
    let mut max_community = *assignments.iter().max().unwrap_or(&0);

    // For each community, check connectivity via BFS
    let mut community_nodes: HashMap<usize, Vec<usize>> = HashMap::new();
    for (node, &comm) in assignments.iter().enumerate() {
        community_nodes.entry(comm).or_default().push(node);
    }

    for (comm_id, nodes) in &community_nodes {
        if nodes.len() <= 1 {
            continue;
        }

        // BFS from first node, only following edges within same community
        let mut visited = HashSet::new();
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(nodes[0]);
        visited.insert(nodes[0]);

        while let Some(current) = queue.pop_front() {
            for &(neighbor, _) in &graph.adj[current] {
                if assignments[neighbor] == *comm_id && !visited.contains(&neighbor) {
                    visited.insert(neighbor);
                    queue.push_back(neighbor);
                }
            }
        }

        // If not all nodes were visited, the community is disconnected
        if visited.len() < nodes.len() {
            // Split: assign unvisited nodes to new communities
            for &node in nodes {
                if !visited.contains(&node) {
                    max_community += 1;
                    // BFS from this unvisited node to find its component
                    let mut component = HashSet::new();
                    let mut q2 = std::collections::VecDeque::new();
                    q2.push_back(node);
                    component.insert(node);

                    while let Some(current) = q2.pop_front() {
                        for &(neighbor, _) in &graph.adj[current] {
                            if assignments[neighbor] == *comm_id
                                && !visited.contains(&neighbor)
                                && !component.contains(&neighbor)
                            {
                                component.insert(neighbor);
                                q2.push_back(neighbor);
                            }
                        }
                    }

                    for &n in &component {
                        assignments[n] = max_community;
                        visited.insert(n); // Mark as processed
                    }
                }
            }
        }
    }
}

/// Compute modularity of a partition.
fn compute_modularity(graph: &Graph, assignments: &[usize], resolution: f64) -> f64 {
    let m2 = graph.total_weight;
    if m2 == 0.0 {
        return 0.0;
    }

    let mut q = 0.0;
    for edge in &graph.edges {
        if assignments[edge.source] == assignments[edge.target] {
            let ki = graph.degree(edge.source);
            let kj = graph.degree(edge.target);
            q += edge.weight - resolution * ki * kj / m2;
        }
    }

    q / m2
}

// ─── Hierarchical Detection ──────────────────────────────────────────────────

/// Run multi-level community detection at different resolutions.
pub fn detect_hierarchical(
    graph: &Graph,
    resolutions: &[f64],
    min_community_size: usize,
) -> Vec<CommunityResult> {
    resolutions
        .iter()
        .enumerate()
        .map(|(level, &resolution)| {
            let config = LeidenConfig {
                resolution,
                min_community_size,
                ..Default::default()
            };
            let mut result = detect_communities(graph, &config);
            result.level = level as i32;
            result
        })
        .collect()
}

// ─── Neo4j Integration ───────────────────────────────────────────────────────

/// Extract graph structure from Neo4j for a specific book.
pub async fn extract_book_graph(
    neo4j: &Neo4jClient,
    book_id: &str,
) -> nova_core::Result<Graph> {
    let cypher = "MATCH (a {book_id: $book_id})-[r]->(b {book_id: $book_id}) \
                  RETURN a.name AS source, b.name AS target, \
                  COALESCE(r.weight, 1.0) AS weight";

    let params = serde_json::json!({ "book_id": book_id });
    let result = neo4j.execute(cypher, Some(params)).await?;

    let mut graph = Graph::new();

    if let Some(results) = result["results"].as_array() {
        if let Some(first_result) = results.first() {
            if let Some(data) = first_result["data"].as_array() {
                for row in data {
                    if let Some(row_data) = row["row"].as_array() {
                        let source = row_data.first().and_then(|v| v.as_str()).unwrap_or("");
                        let target = row_data.get(1).and_then(|v| v.as_str()).unwrap_or("");
                        let weight = row_data.get(2).and_then(|v| v.as_f64()).unwrap_or(1.0);

                        if !source.is_empty() && !target.is_empty() {
                            graph.add_edge(source, target, weight);
                        }
                    }
                }
            }
        }
    }

    Ok(graph)
}

/// Store community detection results back to Neo4j.
pub async fn store_communities(
    neo4j: &Neo4jClient,
    book_id: &str,
    graph: &Graph,
    result: &CommunityResult,
) -> nova_core::Result<usize> {
    // First, remove existing communities for this book at this level
    let cleanup_cypher = "MATCH (c:Community {book_id: $book_id, level: $level}) DETACH DELETE c";
    neo4j.execute(cleanup_cypher, Some(serde_json::json!({
        "book_id": book_id,
        "level": result.level,
    }))).await?;

    // Group nodes by community
    let mut communities: HashMap<usize, Vec<String>> = HashMap::new();
    for (node_idx, &comm_id) in result.assignments.iter().enumerate() {
        communities
            .entry(comm_id)
            .or_default()
            .push(graph.node_names[node_idx].clone());
    }

    let mut stored = 0;

    for (comm_id, members) in &communities {
        if members.is_empty() {
            continue;
        }

        // Create Community node
        let comm_node_id = format!("{}_L{}_C{}", book_id, result.level, comm_id);
        let create_cypher = "CREATE (c:Community { \
            id: $id, book_id: $book_id, level: $level, \
            entity_count: $count, members: $members \
        })";

        neo4j.execute(create_cypher, Some(serde_json::json!({
            "id": comm_node_id,
            "book_id": book_id,
            "level": result.level,
            "count": members.len(),
            "members": members,
        }))).await?;

        // Link entities to community
        for member in members {
            let link_cypher = "MATCH (e {name: $name, book_id: $book_id}), \
                             (c:Community {id: $comm_id}) \
                             MERGE (e)-[:BELONGS_TO_COMMUNITY]->(c)";
            neo4j.execute(link_cypher, Some(serde_json::json!({
                "name": member,
                "book_id": book_id,
                "comm_id": comm_node_id,
            }))).await?;
        }

        stored += 1;
    }

    Ok(stored)
}

/// Extract communities as structured data for summarization.
pub fn extract_community_data(
    graph: &Graph,
    result: &CommunityResult,
) -> Vec<DetectedCommunity> {
    let mut communities: HashMap<usize, Vec<usize>> = HashMap::new();
    for (node_idx, &comm_id) in result.assignments.iter().enumerate() {
        communities.entry(comm_id).or_default().push(node_idx);
    }

    communities
        .into_iter()
        .map(|(comm_id, node_indices)| {
            let members: Vec<String> = node_indices
                .iter()
                .map(|&idx| graph.node_names[idx].clone())
                .collect();

            // Count internal edges
            let internal_edges = graph.edges.iter().filter(|e| {
                result.assignments[e.source] == comm_id
                    && result.assignments[e.target] == comm_id
            }).count();

            DetectedCommunity {
                id: format!("C{}", comm_id),
                level: result.level,
                members,
                internal_edges,
                modularity_contribution: 0.0,
            }
        })
        .collect()
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn build_test_graph() -> Graph {
        // Two clusters connected by a bridge
        // Cluster 1: A-B-C (triangle)
        // Cluster 2: D-E-F (triangle)
        // Bridge: C-D
        let mut g = Graph::new();
        g.add_edge("A", "B", 1.0);
        g.add_edge("B", "C", 1.0);
        g.add_edge("A", "C", 1.0);
        g.add_edge("D", "E", 1.0);
        g.add_edge("E", "F", 1.0);
        g.add_edge("D", "F", 1.0);
        g.add_edge("C", "D", 0.5); // Weak bridge
        g
    }

    #[test]
    fn test_leiden_basic() {
        let graph = build_test_graph();
        let config = LeidenConfig::default();
        let result = detect_communities(&graph, &config);

        assert!(result.num_communities >= 2, "Should find at least 2 communities");
        assert!(result.modularity > 0.0, "Modularity should be positive");

        // A, B, C should be in same community
        let a = graph.node_index["A"];
        let b = graph.node_index["B"];
        let c = graph.node_index["C"];
        assert_eq!(result.assignments[a], result.assignments[b]);
        assert_eq!(result.assignments[b], result.assignments[c]);

        // D, E, F should be in same community
        let d = graph.node_index["D"];
        let e = graph.node_index["E"];
        let f = graph.node_index["F"];
        assert_eq!(result.assignments[d], result.assignments[e]);
        assert_eq!(result.assignments[e], result.assignments[f]);

        // The two clusters should be in different communities
        assert_ne!(result.assignments[a], result.assignments[d]);
    }

    #[test]
    fn test_hierarchical() {
        let graph = build_test_graph();
        let results = detect_hierarchical(&graph, &[0.5, 1.0, 2.0], 2);

        assert_eq!(results.len(), 3);
        // Higher resolution should produce more communities (or equal)
        assert!(results[2].num_communities >= results[0].num_communities);
    }

    #[test]
    fn test_empty_graph() {
        let graph = Graph::new();
        let config = LeidenConfig::default();
        let result = detect_communities(&graph, &config);
        assert_eq!(result.num_communities, 0);
    }

    #[test]
    fn test_single_node() {
        let mut graph = Graph::new();
        graph.add_node("Lonely");
        let config = LeidenConfig::default();
        let result = detect_communities(&graph, &config);
        assert_eq!(result.assignments.len(), 1);
    }

    #[test]
    fn test_community_extraction() {
        let graph = build_test_graph();
        let config = LeidenConfig::default();
        let result = detect_communities(&graph, &config);
        let communities = extract_community_data(&graph, &result);

        assert!(communities.len() >= 2);
        for comm in &communities {
            assert!(!comm.members.is_empty());
        }
    }
}
