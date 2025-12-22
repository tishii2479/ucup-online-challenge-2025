use crate::{MAX_BATCH_SIZE, State, Task, core::*};

pub struct DurationCalculator {
    /// path_duration[packet_type][batch_size][from_path_index]
    path_durations: Vec<Vec<Vec<Duration>>>,
}

impl DurationCalculator {
    pub fn new(input: &Input, graph: &Graph) -> Self {
        let max_batch_size = MAX_BATCH_SIZE.iter().max().unwrap() + 5;
        let max_path_length = graph.paths.iter().map(|p| p.path.len()).max().unwrap();
        let mut path_durations =
            vec![
                vec![vec![Duration::new(0, 0); max_path_length + 1]; max_batch_size + 1];
                N_PACKET_TYPE
            ];
        for packet_type in 0..N_PACKET_TYPE {
            for batch_size in 1..=max_batch_size {
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
        self.path_durations[packet_type][batch_size][from_path_index].clone()
    }

    /// packet_typeをbatch_sizeで処理する場合の所要時間を見積もる
    /// NOTE: SPECIAL_NODEは判明していないものとしている
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
pub fn calculate_core_duration(
    state: &State,
    core_id: usize,
    input: &Input,
    graph: &Graph,
) -> Duration {
    let mut ret = Duration::new(0, 0);
    if let Some(cur_task) = &state.next_tasks[core_id] {
        ret.add(&calculate_task_duration(
            cur_task,
            input,
            graph,
            &state.packet_special_cost,
        ));
    }
    ret
}

/// タスクを終了するのにかかる時間を見積もる
/// NOTE: estimate_path_durationはnode=8が判明したことを考慮していないので、こっちを使う必要がある
pub fn calculate_task_duration(
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

    let path = &graph.paths[task.packet_type].path;
    for (i, node_id) in path[task.path_index..path.len()].iter().enumerate() {
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

#[derive(Clone, Debug)]
pub struct DurationEstimator {
    special_costs: Vec<Vec<i64>>,
    cur_alpha: Vec<f64>,
    init_weight: f64,
}

impl DurationEstimator {
    pub fn new(n: usize, init_alpha: f64, init_weight: f64) -> Self {
        DurationEstimator {
            special_costs: vec![Vec::with_capacity(n); N_PACKET_TYPE],
            cur_alpha: vec![init_alpha; N_PACKET_TYPE],
            init_weight,
        }
    }

    pub fn update_alpha(&mut self, packet_type: usize, cost: i64) {
        let alpha = cost as f64 / SPECIAL_COST_SUM as f64;
        let w = self.init_weight + self.special_costs[packet_type].len() as f64;
        self.cur_alpha[packet_type] = (self.cur_alpha[packet_type] as f64 * w + alpha) / (w + 1.0);
        self.special_costs[packet_type].push(cost);
    }

    pub fn estimate(&self, duration: &Duration, packet_type: usize) -> i64 {
        (duration.lower_bound() as f64
            + self.cur_alpha[packet_type]
                * (duration.upper_bound() - duration.lower_bound()) as f64)
            .round() as i64
    }
}
