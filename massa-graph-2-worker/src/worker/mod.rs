use massa_graph::BootstrapableGraph;
use massa_graph_2_exports::{GraphChannels, GraphConfig, GraphController, GraphManager};
use massa_models::block::BlockId;
use massa_models::clique::Clique;
use massa_models::prehash::PreHashSet;
use massa_models::slot::Slot;
use massa_storage::Storage;
use massa_time::MassaTime;
use parking_lot::RwLock;
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Instant;

use crate::commands::GraphCommand;
use crate::controller::GraphControllerImpl;
use crate::manager::GraphManagerImpl;
use crate::state::GraphState;

pub struct GraphWorker {
    command_receiver: mpsc::Receiver<GraphCommand>,
    config: GraphConfig,
    shared_state: Arc<RwLock<GraphState>>,
    /// Previous slot.
    previous_slot: Option<Slot>,
    /// Next slot
    next_slot: Slot,
    /// Next slot instant
    next_instant: Instant,
}

mod init;
mod main_loop;

pub fn start_graph_worker(
    config: GraphConfig,
    channels: GraphChannels,
    init_graph: Option<BootstrapableGraph>,
    storage: Storage,
) -> (Box<dyn GraphController>, Box<dyn GraphManager>) {
    let (tx, rx) = mpsc::sync_channel(10);
    // desync detection timespan
    let stats_desync_detection_timespan =
        config.t0.checked_mul(config.periods_per_cycle * 2).unwrap();
    let shared_state = Arc::new(RwLock::new(GraphState {
        storage: storage.clone(),
        config: config.clone(),
        channels: channels.clone(),
        max_cliques: vec![Clique {
            block_ids: PreHashSet::<BlockId>::default(),
            fitness: 0,
            is_blockclique: true,
        }],
        sequence_counter: 0,
        waiting_for_slot_index: Default::default(),
        waiting_for_dependencies_index: Default::default(),
        discarded_index: Default::default(),
        to_propagate: Default::default(),
        attack_attempts: Default::default(),
        new_final_blocks: Default::default(),
        new_stale_blocks: Default::default(),
        incoming_index: Default::default(),
        active_index: Default::default(),
        save_final_periods: Default::default(),
        latest_final_blocks_periods: Default::default(),
        best_parents: Default::default(),
        block_statuses: Default::default(),
        genesis_hashes: Default::default(),
        gi_head: Default::default(),
        final_block_stats: Default::default(),
        stale_block_stats: Default::default(),
        protocol_blocks: Default::default(),
        wishlist: Default::default(),
        launch_time: MassaTime::now(config.clock_compensation_millis).unwrap(),
        stats_desync_detection_timespan,
        stats_history_timespan: std::cmp::max(
            stats_desync_detection_timespan,
            config.stats_timespan,
        ),
        prev_blockclique: Default::default(),
    }));

    let shared_state_cloned = shared_state.clone();
    let thread_graph = thread::Builder::new()
        .name("graph worker".into())
        .spawn(move || {
            let mut graph_worker =
                GraphWorker::new(config, rx, shared_state_cloned, init_graph, storage).unwrap();
            graph_worker.run()
        })
        .expect("Can't spawn thread graph.");

    let manager = GraphManagerImpl {
        thread_graph: Some(thread_graph),
        graph_command_sender: tx.clone(),
    };

    let controller = GraphControllerImpl::new(tx, shared_state);

    (Box::new(controller), Box::new(manager))
}
