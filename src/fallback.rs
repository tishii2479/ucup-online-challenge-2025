use std::{cmp::Reverse, collections::BinaryHeap};

use crate::{core::*, interactor::*};

#[allow(dead_code)]
pub struct FallbackSolver;

impl Solver for FallbackSolver {
    fn solve<I: Interactor>(
        &self,
        n: usize,
        interactor: &mut I,
        input: &Input,
        graph: &crate::core::Graph,
    ) {
        #[derive(Clone)]
        struct Task {
            packet_type: usize,
            ps: Vec<Packet>,
            path_index: usize,
        }
        let start_t = LAST_PACKET_T + 10;
        let packets = interactor.send_receive_packets(start_t).unwrap();
        let mut groups = vec![vec![]; N_PACKET_TYPE];
        for p in packets {
            groups[p.packet_type].push(p);
        }

        const MAX_BATCH_SIZES: [usize; N_PACKET_TYPE] = [49, 49, 49, 8, 8, 49, 49];
        let mut q = BinaryHeap::new();
        let mut tasks = vec![vec![]; input.n_cores];
        let mut special_costs = vec![None; n];
        let mut core_id = 0;
        for group in groups {
            let p_type = group[0].packet_type;
            for i in (0..group.len()).step_by(MAX_BATCH_SIZES[p_type]) {
                let batch = &group[i..(i + MAX_BATCH_SIZES[p_type]).min(group.len())];
                let mut ps = vec![];
                for p in batch {
                    ps.push(p.clone());
                }
                tasks[core_id % input.n_cores].push(Task {
                    packet_type: p_type,
                    ps,
                    path_index: 0,
                });
                core_id += 1;
            }
        }
        for i in 0..input.n_cores {
            q.push((Reverse(start_t + input.cost_r), i));
        }

        while let Some((t, core_id)) = q.pop() {
            let core_id = core_id;
            let task = match tasks[core_id].pop() {
                Some(task) => task,
                None => continue,
            };
            let node_id = graph.paths[task.packet_type].path[task.path_index];
            if node_id == SPECIAL_NODE_ID {
                for p in &task.ps {
                    if special_costs[p.i].is_none() {
                        // 特殊ノードのコストを問い合わせる
                        let works = interactor.send_query_works(t.0, p.i).unwrap();
                        let mut cost_sum = 0;
                        for i in 0..N_SPECIAL {
                            if works[i] {
                                cost_sum += graph.special_costs[i];
                            }
                        }
                        special_costs[p.i] = Some(cost_sum);
                    }
                }
            }

            let s = task.ps.len();
            let dt = if node_id == SPECIAL_NODE_ID {
                graph.nodes[node_id].costs[s]
                    + task
                        .ps
                        .iter()
                        .map(|p| special_costs[p.i].unwrap())
                        .sum::<i64>()
            } else {
                graph.nodes[node_id].costs[s]
            };
            let switch_cost = if task.path_index == 0 {
                input.cost_switch * s as i64
            } else {
                0
            };
            let next_t = t.0 + dt + switch_cost;
            q.push((Reverse(next_t), core_id));

            interactor.send_execute(
                t.0,
                core_id,
                node_id,
                task.ps.len(),
                task.ps.iter().map(|p| p.i).collect(),
            );

            if task.path_index + 1 < graph.paths[task.packet_type].l {
                tasks[core_id].push(Task {
                    packet_type: task.packet_type,
                    ps: task.ps,
                    path_index: task.path_index + 1,
                });
            }
        }

        interactor.send_finish();
    }
}
