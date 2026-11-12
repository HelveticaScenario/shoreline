//! Graph analysis via [`petgraph::algo::tarjan_scc`].
//!
//! A single Tarjan pass yields two things the patch update needs:
//!
//! - A [`ProcessingMode`] per module ID. Modules in a strongly-connected
//!   component with >1 member, or a single node with a self-loop, are assigned
//!   `Sample` mode (so the wrapper computes one sample at a time and the
//!   1-sample feedback delay invariant holds). All others get `Block` mode.
//! - A cache-efficient processing order — every producer is listed before the
//!   consumers that read it, so when a consumer runs its block the upstream
//!   output buffers it reads are freshly written and hot in cache.
//!
//! [`f64_readers`] runs over the same cables, with their ports, to find the
//! modules whose `precision = inherit` outputs carry f64 values.
//!
//! Cable extraction lives elsewhere — callers walk deserialized params via
//! the [`modular_core::types::CollectCables`] trait and pass the resulting
//! adjacency map here. Keeping graph_analysis ignorant of params shape means
//! the cable schema lives in exactly one place: each type's own `CollectCables`
//! impl.

use modular_core::types::{CableSource, OutputPrecision, ProcessingMode};
use petgraph::algo::tarjan_scc;
use petgraph::graphmap::DiGraphMap;
use std::collections::{HashMap, HashSet};

/// Result of analysing the cable adjacency graph: the per-module processing
/// mode and a cache-efficient (producer-before-consumer) processing order.
pub struct GraphAnalysis {
    /// `Block` or `Sample` mode per module ID (cycle participants get `Sample`).
    pub modes: HashMap<String, ProcessingMode>,
    /// Module IDs in producer-before-consumer order. Cycle members are grouped
    /// contiguously; their relative order is arbitrary for correctness (the
    /// wrapper's reentrancy guard makes any order within a cycle valid). Within
    /// each component IDs are sorted for deterministic output. Dangling
    /// producer IDs (referenced by a cable but not themselves modules) appear
    /// here too; callers skip any ID absent from their module map.
    pub order: Vec<String>,
}

/// Analyse the cable adjacency graph in a single Tarjan SCC pass.
///
/// `adjacency[consumer]` is the list of producer module IDs `consumer` reads
/// from (one entry per cable; duplicates are tolerated). Producer IDs that
/// are not also keys in `adjacency` are inserted as orphan `Block` nodes and
/// still appear in `order`.
pub fn analyze(adjacency: &HashMap<String, Vec<String>>) -> GraphAnalysis {
    let mut g: DiGraphMap<&str, ()> = DiGraphMap::new();

    for (consumer, producers) in adjacency {
        g.add_node(consumer.as_str());
        for producer in producers {
            // `add_edge` inserts both endpoints if missing; safe even when the
            // producer isn't a key in `adjacency` (e.g. a dangling cable).
            g.add_edge(consumer.as_str(), producer.as_str(), ());
        }
    }

    let mut modes = HashMap::new();
    let mut order = Vec::new();

    // Edges point consumer→producer, and `tarjan_scc` returns components in
    // reverse-topological order — i.e. a component appears before the
    // components that have edges into it. With this edge orientation that is
    // producer-before-consumer, exactly the cache-efficient processing order.
    for mut scc in tarjan_scc(&g) {
        // A 1-node SCC is cyclic iff the node has an edge to itself.
        let cyclic = scc.len() > 1 || (scc.len() == 1 && g.contains_edge(scc[0], scc[0]));
        let mode = if cyclic {
            ProcessingMode::Sample
        } else {
            ProcessingMode::Block
        };
        // Deterministic order within a component (membership is arbitrary).
        scc.sort_unstable();
        for id in scc {
            modes.insert(id.to_string(), mode);
            order.push(id.to_string());
        }
    }

    GraphAnalysis { modes, order }
}

/// The modules with a cable from an f64 output: an `F64` output, or an
/// `Inherit` output of a module that is itself in the set. Their `Inherit`
/// outputs are f64, so a chain of arithmetic on an f64 value stays f64.
///
/// `cables[consumer]` lists the outputs `consumer` reads, and
/// `precision(module, port)` gives each output's declared precision. Membership
/// only ever grows, so repeating the pass until nothing joins settles
/// feedback cycles too.
pub fn f64_readers(
    cables: &HashMap<String, Vec<CableSource>>,
    precision: impl Fn(&str, &str) -> OutputPrecision,
) -> HashSet<String> {
    let mut readers: HashSet<String> = HashSet::new();
    loop {
        let mut grew = false;
        for (consumer, sources) in cables {
            if readers.contains(consumer) {
                continue;
            }
            let reads_f64 =
                sources
                    .iter()
                    .any(|source| match precision(&source.module, &source.port) {
                        OutputPrecision::F32 => false,
                        OutputPrecision::F64 => true,
                        OutputPrecision::Inherit => readers.contains(&source.module),
                    });
            if reads_f64 {
                readers.insert(consumer.clone());
                grew = true;
            }
        }
        if !grew {
            return readers;
        }
    }
}

/// [`f64_readers`] for a patch whose modules have the types in
/// `module_types`, with each output's precision from the module schemas.
pub fn f64_modules(
    cables: &HashMap<String, Vec<CableSource>>,
    module_types: &HashMap<&str, &str>,
) -> HashSet<String> {
    let precisions = modular_core::dsp::output_precisions();
    f64_readers(cables, |module, port| {
        module_types
            .get(module)
            .and_then(|module_type| precisions.get(*module_type))
            .and_then(|ports| ports.get(port))
            .copied()
            .unwrap_or_default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use modular_core::types::ProcessingMode;

    fn adj(edges: &[(&str, &[&str])]) -> HashMap<String, Vec<String>> {
        edges
            .iter()
            .map(|(consumer, producers)| {
                (
                    consumer.to_string(),
                    producers.iter().map(|p| p.to_string()).collect(),
                )
            })
            .collect()
    }

    fn modes(edges: &[(&str, &[&str])]) -> HashMap<String, ProcessingMode> {
        analyze(&adj(edges)).modes
    }

    /// Index of `id` within a processing order, for before/after assertions.
    fn pos(order: &[String], id: &str) -> usize {
        order
            .iter()
            .position(|s| s == id)
            .unwrap_or_else(|| panic!("{id} missing from order {order:?}"))
    }

    #[test]
    fn no_cycle_is_block_mode() {
        // A -> B -> C (consumer C reads B; consumer B reads A)
        let m = modes(&[("A", &[]), ("B", &["A"]), ("C", &["B"])]);
        assert_eq!(m["A"], ProcessingMode::Block);
        assert_eq!(m["B"], ProcessingMode::Block);
        assert_eq!(m["C"], ProcessingMode::Block);
    }

    #[test]
    fn two_node_cycle_is_sample_mode() {
        // A <-> B
        let m = modes(&[("A", &["B"]), ("B", &["A"])]);
        assert_eq!(m["A"], ProcessingMode::Sample);
        assert_eq!(m["B"], ProcessingMode::Sample);
    }

    #[test]
    fn self_loop_is_sample_mode() {
        let m = modes(&[("A", &["A"])]);
        assert_eq!(m["A"], ProcessingMode::Sample);
    }

    #[test]
    fn three_node_cycle_is_sample_mode() {
        // A → B → C → A (no shorter back-edges)
        let m = modes(&[("A", &["C"]), ("B", &["A"]), ("C", &["B"])]);
        assert_eq!(m["A"], ProcessingMode::Sample);
        assert_eq!(m["B"], ProcessingMode::Sample);
        assert_eq!(m["C"], ProcessingMode::Sample);
    }

    #[test]
    fn cycle_plus_independent_node() {
        let m = modes(&[("A", &["B"]), ("B", &["A"]), ("C", &[])]);
        assert_eq!(m["A"], ProcessingMode::Sample);
        assert_eq!(m["B"], ProcessingMode::Sample);
        assert_eq!(m["C"], ProcessingMode::Block);
    }

    #[test]
    fn dangling_producer_is_inserted_as_block() {
        // Consumer references a module ID that isn't a key — still treated as
        // a node so it gets a mode entry.
        let m = modes(&[("A", &["MISSING"])]);
        assert_eq!(m["A"], ProcessingMode::Block);
        assert_eq!(m["MISSING"], ProcessingMode::Block);
    }

    #[test]
    fn duplicate_producer_edges_collapse() {
        // Two cables from B reading A — adjacency contains "A" twice. Should
        // not affect classification.
        let m = modes(&[("B", &["A", "A"])]);
        assert_eq!(m["A"], ProcessingMode::Block);
        assert_eq!(m["B"], ProcessingMode::Block);
    }

    #[test]
    fn order_lists_producers_before_consumers() {
        // Chain A → B → C (B reads A, C reads B). Processing order must run
        // the producer before each consumer: A, then B, then C.
        let order = analyze(&adj(&[("A", &[]), ("B", &["A"]), ("C", &["B"])])).order;
        assert!(pos(&order, "A") < pos(&order, "B"));
        assert!(pos(&order, "B") < pos(&order, "C"));
    }

    #[test]
    fn order_diamond_producer_first() {
        // D reads B and C; B and C both read A. A must come first, D last.
        let order = analyze(&adj(&[
            ("A", &[]),
            ("B", &["A"]),
            ("C", &["A"]),
            ("D", &["B", "C"]),
        ]))
        .order;
        assert!(pos(&order, "A") < pos(&order, "B"));
        assert!(pos(&order, "A") < pos(&order, "C"));
        assert!(pos(&order, "B") < pos(&order, "D"));
        assert!(pos(&order, "C") < pos(&order, "D"));
    }

    #[test]
    fn order_includes_every_module() {
        // Fully disconnected modules (no cables at all) still appear, so they
        // get force-processed by the audio thread.
        let order = analyze(&adj(&[("A", &[]), ("B", &[]), ("C", &[])])).order;
        assert_eq!(order.len(), 3);
        for id in ["A", "B", "C"] {
            assert!(order.contains(&id.to_string()), "{id} missing");
        }
    }

    #[test]
    fn order_groups_cycle_before_downstream_consumer() {
        // Cycle A <-> B feeds C. The two cycle members come before C and are
        // adjacent to each other.
        let order = analyze(&adj(&[("A", &["B"]), ("B", &["A"]), ("C", &["A"])])).order;
        assert!(pos(&order, "A") < pos(&order, "C"));
        assert!(pos(&order, "B") < pos(&order, "C"));
        let span = pos(&order, "A").abs_diff(pos(&order, "B"));
        assert_eq!(span, 1, "cycle members should be contiguous: {order:?}");
    }

    fn cables(edges: &[(&str, &[(&str, &str)])]) -> HashMap<String, Vec<CableSource>> {
        edges
            .iter()
            .map(|(consumer, sources)| {
                let sources = sources
                    .iter()
                    .map(|(module, port)| CableSource {
                        module: module.to_string(),
                        port: port.to_string(),
                    })
                    .collect();
                (consumer.to_string(), sources)
            })
            .collect()
    }

    /// `clock.playhead` is f64; every `add*`/`loop` module's `output` inherits.
    fn precision(module: &str, port: &str) -> OutputPrecision {
        match (module, port) {
            ("clock", "playhead") => OutputPrecision::F64,
            (m, "output") if m.starts_with("add") || m.starts_with("loop") => {
                OutputPrecision::Inherit
            }
            _ => OutputPrecision::F32,
        }
    }

    fn readers(edges: &[(&str, &[(&str, &str)])]) -> Vec<String> {
        let mut readers: Vec<String> = f64_readers(&cables(edges), precision).into_iter().collect();
        readers.sort();
        readers
    }

    #[test]
    fn f64_flows_through_inheriting_modules_only() {
        let found = readers(&[
            ("clock", &[]),
            ("add1", &[("clock", "playhead")]),
            ("add2", &[("add1", "output"), ("lfo", "output")]),
            ("seq", &[("add2", "output")]),
            ("env", &[("clock", "barTrigger")]),
            ("add3", &[("env", "output")]),
            ("filter", &[("seq", "output")]),
        ]);
        assert_eq!(found, ["add1", "add2", "seq"]);
    }

    #[test]
    fn f64_settles_around_a_feedback_cycle() {
        // `add2` reads `loop1` before `loop1` is known to be f64.
        let found = readers(&[
            ("add2", &[("loop1", "output")]),
            ("loop1", &[("add1", "output"), ("add2", "output")]),
            ("add1", &[("clock", "playhead")]),
        ]);
        assert_eq!(found, ["add1", "add2", "loop1"]);
    }

    #[test]
    fn f64_modules_follow_the_module_schemas() {
        let module_types: HashMap<&str, &str> = [
            ("clock", "_clock"),
            ("sub", "_arithmetic"),
            ("frac", "$wrap"),
            ("bar", "_round"),
            ("half", "$scaleAndShift"),
            ("lfo", "$sine"),
            ("mix", "_arithmetic"),
        ]
        .into_iter()
        .collect();
        let cables = cables(&[
            ("clock", &[]),
            ("sub", &[("clock", "playhead")]),
            ("frac", &[("sub", "output")]),
            ("bar", &[("frac", "output")]),
            ("half", &[("bar", "output")]),
            ("lfo", &[("half", "output")]),
            ("mix", &[("lfo", "output"), ("clock", "beatTrigger")]),
        ]);
        let mut found: Vec<String> = f64_modules(&cables, &module_types).into_iter().collect();
        found.sort();
        assert_eq!(found, ["bar", "frac", "half", "lfo", "sub"]);
    }
}
