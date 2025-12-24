use crate::{State, Task, core::*};

pub struct DurationCalculator {
    /// path_duration[packet_type][batch_size][from_path_index]
    path_durations: Vec<Vec<Vec<Duration>>>,
}

impl DurationCalculator {
    pub fn new(input: &Input, graph: &Graph) -> Self {
        const MAX_BATCH_SIZE: usize = 128;
        let max_path_length = graph.paths.iter().map(|p| p.path.len()).max().unwrap();
        let mut path_durations =
            vec![
                vec![vec![Duration::new(0, 0); max_path_length + 1]; MAX_BATCH_SIZE + 1];
                N_PACKET_TYPE
            ];
        for packet_type in 0..N_PACKET_TYPE {
            for batch_size in 1..=MAX_BATCH_SIZE {
                for from_path_index in 0..=graph.paths[packet_type].path.len() {
                    path_durations[packet_type][batch_size][from_path_index] =
                        DurationCalculator::calculate_path_duration(
                            packet_type,
                            batch_size,
                            from_path_index,
                            input,
                            graph,
                        );
                }
            }
        }

        Self { path_durations }
    }

    pub fn get_path_duration(
        &self,
        packet_type: usize,
        batch_size: usize,
        from_path_index: usize,
    ) -> Duration {
        self.path_durations[packet_type][batch_size][from_path_index]
    }

    /// packet_typeをbatch_sizeで処理する場合の所要時間を見積もる
    pub fn calculate_path_duration(
        packet_type: usize,
        batch_size: usize,
        from_path_index: usize,
        input: &Input,
        graph: &Graph,
    ) -> Duration {
        let mut ret = Duration::new(0, 0);

        // core=0から移るswitch cost
        if from_path_index == 0 {
            ret.fixed += batch_size as i64 * input.cost_switch;
        }

        for node_id in &graph.paths[packet_type].path[from_path_index..] {
            let max_batch_size = graph.nodes[*node_id].costs.len() - 1;
            let full_chunk_count = batch_size / max_batch_size;
            ret.fixed += graph.nodes[*node_id].costs[max_batch_size] * full_chunk_count as i64;

            let remainder = batch_size % max_batch_size;
            ret.fixed += graph.nodes[*node_id].costs[remainder];

            if *node_id == SPECIAL_NODE_ID {
                ret.special_node_count += batch_size as i64;
            }
        }

        ret
    }
}

/// コアが現状保持しているの処理が終了するまでの時間を見積もる
pub fn estimate_core_duration(
    state: &State,
    core_id: usize,
    input: &Input,
    graph: &Graph,
) -> Duration {
    let mut ret = Duration::new(0, 0);
    if let Some(cur_task) = &state.next_tasks[core_id] {
        ret.add(&estimate_task_duration(
            cur_task,
            input,
            graph,
            &state.packet_special_cost,
        ));
    }
    if let Some(idle_task) = &state.idle_tasks[core_id] {
        ret.add(&estimate_task_duration(
            idle_task,
            input,
            graph,
            &state.packet_special_cost,
        ));
    }
    ret
}

/// タスクを終了するのにかかる時間を見積もる
/// NOTE: estimate_path_durationはnode=8が判明したことを考慮していないので、こっちを使う必要がある
pub fn estimate_task_duration(
    task: &Task,
    input: &Input,
    graph: &Graph,
    packet_special_cost: &Vec<Option<i64>>,
) -> Duration {
    let first_packets = if task.is_chunked {
        task.packets.iter().filter(|&&b| !b.is_advanced).count()
    } else {
        task.packets.len()
    };

    let mut ret = Duration::new(0, 0);

    let has_next_node = task.path_index < graph.paths[task.packet_type].path.len() - 1;
    for p in &task.packets {
        let need_switch = p.is_switching_core && (!p.is_advanced || has_next_node);
        if need_switch {
            ret.fixed += input.cost_switch;
        }
    }

    for (i, node_id) in graph.paths[task.packet_type].path[task.path_index..]
        .iter()
        .enumerate()
    {
        let packets_count = if i == 0 {
            first_packets
        } else {
            task.packets.len()
        };

        let max_batch_size = graph.nodes[*node_id].costs.len() - 1;
        let full_chunk_count = packets_count / max_batch_size;
        ret.fixed += graph.nodes[*node_id].costs[max_batch_size] * full_chunk_count as i64;

        let remainder = packets_count % max_batch_size;
        ret.fixed += graph.nodes[*node_id].costs[remainder];

        if *node_id == SPECIAL_NODE_ID {
            for p in task.packets.iter().filter(|&p| {
                if i == 0 && task.is_chunked {
                    !p.is_advanced
                } else {
                    true
                }
            }) {
                if let Some(special_cost) = packet_special_cost[p.id] {
                    ret.fixed += special_cost;
                } else {
                    ret.special_node_count += 1;
                }
            }
        }
    }

    ret
}
