mod core;
mod libb;
mod values;

use std::{cmp::Reverse, collections::BinaryHeap};

use crate::{core::*, libb::*};

fn main() {
    let p = ProblemParams {
        n: 1000,
        n_cores: 8,
        arrive_term: 1000,
    };
    let interactor = MockInteractor::new(123456789, p);
    let interactor = IOInteractor::new(StdIO::new(true));
    let solver = GreedySolver;
    let runner = Runner;
    let _ = runner.run(solver, interactor);
}

pub struct Runner;

impl Runner {
    pub fn run(&self, solver: impl Solver, mut interactor: impl Interactor) -> i64 {
        let graph = interactor.read_graph();
        let input = interactor.read_input();
        eprintln!("input: {:?}", input);

        let mut scores = vec![];
        for _ in 0..N_SUBTASK {
            let n = interactor.read_n();
            let mut tester = Tester::new(&mut interactor, n, input.clone(), graph.clone());
            let score = solver.solve(n, &mut tester, &input, &graph);

            eprintln!(
                "score: {:10.2} (throughput: {:8.2}, timeout_rate: {:6.2})",
                score.to_score(),
                score.throughput,
                score.timeout_rate
            );
            dump_task_logs(&tester.task_logs);

            scores.push(score.to_score());
        }
        scores.iter().map(|&s| s as i64).max().unwrap()
    }
}

fn dump_task_logs(task_logs: &[TaskLog]) {
    use std::io::Write;
    let mut file = std::fs::File::create("log.json").unwrap();
    let mut json = String::new();
    json.push('[');
    for (i, log) in task_logs.iter().enumerate() {
        if i > 0 {
            json.push(',');
        }
        json.push_str(&format!(
            r#"{{"core_id":{},"start_t":{},"end_t":{},"batch_size":{},"packet_type":{},"path_index":{}}}"#,
            log.core_id, log.start_t, log.end_t, log.batch_size, log.packet_type, log.path_index
        ));
    }
    json.push(']');
    file.write_all(json.as_bytes()).unwrap();
}

#[derive(Clone, Debug)]
struct Task {
    packet_type: usize,
    path_index: usize,
    ids: Vec<usize>,
    is_chunked: bool,
    is_advanced: Vec<bool>,
}

#[derive(Clone, Debug)]
pub struct State {
    packets: Vec<Option<Packet>>,
    packet_special_cost: Vec<Option<i64>>,
    /// cur_tasks[core_id]
    cur_tasks: Vec<Option<Task>>,
    /// idle_tasks[core_id]
    idle_tasks: Vec<Vec<Task>>,
    await_packets: IndexSet,
}

impl State {
    fn new(n: usize, input: &Input) -> Self {
        Self {
            packets: vec![None; n],
            packet_special_cost: vec![None; n],
            cur_tasks: vec![None; input.n_cores],
            idle_tasks: vec![Vec::with_capacity(10); input.n_cores],
            await_packets: IndexSet::empty(n),
        }
    }

    #[allow(dead_code)]
    fn dump(&self) {
        eprintln!("=== State Dump ===");
        eprintln!("Awaiting Packets: {:?}", self.await_packets.que);
        for (core_id, task_opt) in self.cur_tasks.iter().enumerate() {
            if let Some(task) = task_opt {
                eprintln!("Core {}: Executing {:?}", core_id, task);
            } else {
                eprintln!("Core {}: Idle", core_id);
            }
        }
        for (core_id, idle_tasks) in self.idle_tasks.iter().enumerate() {
            eprintln!("Core {}: Idle {:?}", core_id, idle_tasks);
        }
        eprintln!("==================");
    }
}

#[derive(Clone, Debug, Copy, PartialEq, PartialOrd, Eq, Ord)]
enum Event {
    ReceivePacket,
    /// core_id
    ResumeCore(usize),
}

pub struct GreedySolver;

impl Solver for GreedySolver {
    fn solve<I: Interactor>(
        &self,
        n: usize,
        tester: &mut Tester<I>,
        input: &Input,
        graph: &Graph,
    ) -> Score {
        let mut q: BinaryHeap<(Reverse<i64>, Event)> = BinaryHeap::new();
        q.push((Reverse(1), Event::ReceivePacket));

        let mut state = State::new(n, input);
        while let Some((t, event)) = q.pop() {
            eprintln!("t: {}, event: {:?}", t.0, event);
            match event {
                Event::ReceivePacket => {
                    receive_packet(&mut state, t.0, tester, input, graph, &mut q);
                }
                Event::ResumeCore(core_id) => {
                    // タスクが完了したか確認する
                    if let Some(cur_task) = &state.cur_tasks[core_id] {
                        if cur_task.path_index >= graph.paths[cur_task.packet_type].path.len() {
                            complete_task(&mut state, core_id, t.0, graph, &mut q);
                            continue;
                        }
                    }

                    process_task(&mut state, core_id, t.0, tester, graph, &mut q);
                }
            }
        }

        tester.send_finish()
    }
}

/// タスクを進める
fn process_task(
    state: &mut State,
    core_id: usize,
    t: i64,
    tester: &mut Tester<impl Interactor>,
    graph: &Graph,
    q: &mut BinaryHeap<(Reverse<i64>, Event)>,
) {
    let cur_task = state.cur_tasks[core_id]
        .as_mut()
        .expect("cur_task should be Some");

    let node_id = graph.paths[cur_task.packet_type].path[cur_task.path_index];
    eprintln!(
        "Core {} processing task {:?} at node {}",
        core_id, cur_task, node_id
    );
    if node_id == SPECIAL_NODE_ID {
        // 特殊ノードに到達した場合は`works`を問い合わせる
        for &packet_i in &cur_task.ids {
            if state.packet_special_cost[packet_i].is_some() {
                continue;
            }
            let work = tester.send_query_works(t, packet_i).unwrap();
            let mut cost_sum = 0;
            for i in 0..N_SPECIAL {
                if work[i] {
                    cost_sum += graph.special_costs[i];
                }
            }
            state.packet_special_cost[packet_i] = Some(cost_sum);
        }

        // TODO: バッチ内に間に合わなそうなパケットが判明したら、分割して処理したり、別のコアに移したりする
    }

    let node_id = graph.paths[cur_task.packet_type].path[cur_task.path_index];
    let max_batch_size = graph.nodes[node_id].costs.len() - 1;
    let is_chunk = cur_task.ids.len() > max_batch_size || cur_task.is_chunked;

    if is_chunk {
        let mut chunk_ids = Vec::with_capacity(max_batch_size);
        for (i, id) in cur_task.ids.iter().enumerate() {
            if chunk_ids.len() >= max_batch_size {
                break;
            }
            if cur_task.is_advanced[i] {
                continue;
            }
            cur_task.is_advanced[i] = true;
            chunk_ids.push(*id);
        }

        let dt = if node_id == SPECIAL_NODE_ID {
            graph.nodes[node_id].costs[chunk_ids.len()]
                + chunk_ids
                    .iter()
                    .map(|&id| state.packet_special_cost[id].unwrap())
                    .sum::<i64>()
        } else {
            graph.nodes[node_id].costs[chunk_ids.len()]
        };
        let next_t = t + dt;
        q.push((Reverse(next_t), Event::ResumeCore(core_id)));

        tester.send_execute(t, core_id, node_id, chunk_ids.len(), &chunk_ids);

        // チャンクが完了したか確認する
        let all_chunk_finished = cur_task.is_advanced.iter().all(|&b| b);
        if all_chunk_finished {
            // 次のノードへ進む
            cur_task.path_index += 1;
            cur_task.is_chunked = false;
            for e in cur_task.is_advanced.iter_mut() {
                *e = false;
            }
        }
    } else {
        // dtを計算する
        let dt = if node_id == SPECIAL_NODE_ID {
            graph.nodes[node_id].costs[cur_task.ids.len()]
                + cur_task
                    .ids
                    .iter()
                    .map(|&id| state.packet_special_cost[id].unwrap())
                    .sum::<i64>()
        } else {
            graph.nodes[node_id].costs[cur_task.ids.len()]
        };
        let next_t = t + dt;
        q.push((Reverse(next_t), Event::ResumeCore(core_id)));

        tester.send_execute(t, core_id, node_id, cur_task.ids.len(), &cur_task.ids);

        // 次のノードへ進む
        cur_task.path_index += 1;
    }
}

/// タスク完了時の処理
fn complete_task(
    state: &mut State,
    core_id: usize,
    t: i64,
    graph: &Graph,
    q: &mut BinaryHeap<(Reverse<i64>, Event)>,
) {
    // タスク完了
    state.cur_tasks[core_id] = None;

    // idle_tasksからタスクを取得する
    if let Some(task) = state.idle_tasks[core_id].pop() {
        state.cur_tasks[core_id] = Some(task);
        q.push((Reverse(t), Event::ResumeCore(core_id)));
        return;
    }

    // idle_taskがなければ、最も優先度の高いタスクを割り当てて開始する
    let mut tasks = create_tasks(&state, t, &graph);
    if let Some(task) = tasks.pop() {
        // await_packetsから削除する
        for &packet_i in &task.ids {
            state.await_packets.remove(packet_i);
        }
        state.cur_tasks[core_id] = Some(task);
        q.push((Reverse(t), Event::ResumeCore(core_id)));
        return;
    }

    // TODO: パケットが残っていなければ、他のコアから分割してタスクをもらってくる
}

/// パケット受信イベントの処理
fn receive_packet(
    state: &mut State,
    t: i64,
    tester: &mut Tester<impl Interactor>,
    input: &Input,
    graph: &Graph,
    q: &mut BinaryHeap<(Reverse<i64>, Event)>,
) {
    // パケットを登録する
    let packets = tester.send_receive_packets(t);
    eprintln!("Received packets at t={}, {:?}", t, packets);

    for mut packet in packets {
        let i = packet.i;
        packet.arrive += input.cost_r; // 受信にかかった時間を加算する
        state.packets[i] = Some(packet);
        state.await_packets.add(i);
    }

    // `packet_type`ごとにタスクを作成して、優先度を計算する
    let mut tasks = create_tasks(&state, t, &graph);

    // 空いているコアがある限り優先度順にタスクを割り当てる
    for core_id in 0..input.n_cores {
        if state.cur_tasks[core_id].is_some() {
            continue;
        }
        if let Some(task) = tasks.pop() {
            // await_packetsから削除する
            for &packet_i in &task.ids {
                state.await_packets.remove(packet_i);
            }
            state.cur_tasks[core_id] = Some(task);
            let start_t = t + input.cost_r;
            q.push((Reverse(start_t), Event::ResumeCore(core_id)));
        }
    }

    // TODO: 残っているタスクで、割り込むべきタスクがあればコアのタスクに差し込む

    // 次のパケット受信イベントを登録する
    let next_t = t + input.cost_r * 10;
    if next_t <= LAST_PACKET_T {
        q.push((Reverse(next_t), Event::ReceivePacket));
    }
}

/// packet_typeをbatch_sizeで処理する場合の所要時間を見積もる
/// TODO: 前計算
fn estimate_path_duration(packet_type: usize, batch_size: usize, graph: &Graph) -> i64 {
    let mut ret = 0;
    for node_id in &graph.paths[packet_type].path {
        let max_batch_size = graph.nodes[*node_id].costs.len() - 1;
        let full_chunk_count = batch_size / max_batch_size;
        ret += graph.nodes[*node_id].costs[max_batch_size] * full_chunk_count as i64;

        let remainder = batch_size % max_batch_size;
        ret += graph.nodes[*node_id].costs[remainder];

        if *node_id == SPECIAL_NODE_ID {
            // TODO: worksを開示したら更新する
            ret += batch_size as i64 * graph.special_costs.iter().sum::<i64>() / 2;
        }
    }
    ret
}

/// タスクを作成する
/// 後ろほど優先度が高い
fn create_tasks(state: &State, cur_t: i64, graph: &Graph) -> Vec<Task> {
    let mut packets = vec![vec![]; N_PACKET_TYPE];
    for await_packet in state.await_packets.iter() {
        let packet = state.packets[*await_packet].as_ref().unwrap();
        packets[packet.packet_type].push(packet);
    }

    let mut tasks = vec![];
    for packet_type in 0..N_PACKET_TYPE {
        // パケットを締め切り順にソートする
        packets[packet_type].sort_by_key(|&packet| packet.arrive + packet.timeout);

        let mut cur_ids = vec![];
        let mut min_time_limit = i64::MAX;
        for &packet in &packets[packet_type] {
            // TODO: そもそも間に合わないパケットは無視して良い
            // TODO: バッチサイズの上限を設ける
            let new_duration = estimate_path_duration(packet.packet_type, cur_ids.len() + 1, graph);
            let new_departure = cur_t + new_duration * (1 + 0);
            let packet_time_limit = packet.arrive + packet.timeout;
            if min_time_limit.min(packet_time_limit) < new_departure && cur_ids.len() > 0 {
                // バッチを分割する
                let batch_duration =
                    estimate_path_duration(packet.packet_type, cur_ids.len(), graph);
                let afford = min_time_limit - (cur_t + batch_duration + (1 + 0));
                tasks.push((afford, packet_type, cur_ids.clone()));
                cur_ids.clear();
                min_time_limit = packet_time_limit;
            }
            min_time_limit = min_time_limit.min(packet_time_limit);
            cur_ids.push(packet.i);
        }

        // 残っているidsでタスクを作成する
        if cur_ids.len() > 0 {
            let batch_duration = estimate_path_duration(packet_type, cur_ids.len(), graph);
            let afford = min_time_limit - (cur_t + batch_duration + (1 + 0));
            tasks.push((afford, packet_type, cur_ids.clone()));
        }
    }

    // 優先度が低い順にソートする（後ろほど優先度が高い、stackとして扱う）
    tasks.sort_by_key(|&(afford, _, _)| -afford);
    tasks
        .into_iter()
        .map(|(_, packet_type, ids)| {
            let s = ids.len();
            Task {
                packet_type,
                path_index: 0,
                ids: ids,
                is_chunked: false,
                is_advanced: vec![false; s],
            }
        })
        .collect()
}
