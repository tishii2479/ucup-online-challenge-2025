mod calculator;
mod core;
mod fallback;
mod interactor;
mod libb;

use std::{cmp::Reverse, collections::BinaryHeap};

use crate::{calculator::*, core::*, fallback::FallbackSolver, interactor::*, libb::*};

const TRACKER_ENABLED: bool = true;
const INF: i64 = 1_000_000_000_000;
const FALLBACK_SEC: f64 = 10.;

const B: usize = 16;
const MAX_BATCH_SIZE: [usize; N_PACKET_TYPE] = [B * 3 / 2, B, B, B, B, B, B * 3 / 2];
const MIN_BATCH_SIZE: usize = 2;
const ALPHA: f64 = 0.8;
const SPECIAL_NODE_CHUNK: usize = 4;

const PERMUTE_TASK_THRESHOLD: usize = 6;
const COMPLETE_TASK_TOP_K: usize = PERMUTE_TASK_THRESHOLD;
const RECEIVE_TASK_TOP_K: usize = 10;
const OPTIMIZE_TASK_ITERATION: usize = 100;

impl Duration {
    fn estimate(&self) -> i64 {
        (self.lower_bound() as f64 + ALPHA * (self.upper_bound() - self.lower_bound()) as f64)
            .round() as i64
    }
}

fn should_fallback(elapsed: f64) -> bool {
    elapsed > FALLBACK_SEC
}

fn get_base_max_batch_size(packet_type: usize, _input: &Input, _await_packet_size: usize) -> usize {
    MAX_BATCH_SIZE[packet_type]
}

fn get_desired_batch_size(packet_count: usize, base_max_batch_size: usize) -> usize {
    let batch_count = if packet_count % base_max_batch_size <= base_max_batch_size / 2 {
        packet_count / base_max_batch_size
    } else {
        packet_count / base_max_batch_size + 1
    };
    packet_count.div_ceil(batch_count.max(1))
}

fn get_special_node_chunk_size(cur_task: &Task) -> usize {
    cur_task.packets.len().div_ceil(SPECIAL_NODE_CHUNK).max(1)
}

fn get_receive_dt(cur_t: i64, state: &State, input: &Input) -> i64 {
    const MIN_AWAIT_INTERVAL: i64 = 40;
    const LAST_RECEIVED_T_THRESHOLD: i64 = 100_000;

    let is_receiving = cur_t <= state.last_received_t + LAST_RECEIVED_T_THRESHOLD
        && state.received_packets.size() > 0;
    if is_receiving {
        input.cost_r
    } else {
        MIN_AWAIT_INTERVAL.max(input.cost_r)
    }
}

fn should_chunk_special_node_task(
    node_id: usize,
    cur_task: &Task,
    state: &State,
    core_id: usize,
    input: &Input,
    graph: &Graph,
) -> bool {
    if node_id != SPECIAL_NODE_ID {
        return false;
    }
    if state.await_packets.size() > 0 {
        return false;
    }

    let dt = graph.nodes[node_id].costs[cur_task.packets.len()]
        + cur_task
            .packets
            .iter()
            .map(|p| state.packet_special_cost[p.id].unwrap())
            .sum::<i64>();

    for other_core_id in 0..input.n_cores {
        if other_core_id == core_id {
            continue;
        }
        let other_core_duration =
            estimate_core_duration(state, other_core_id, input, graph).estimate();
        if other_core_duration < dt {
            return true;
        }
    }
    false
}

fn main() {
    let io = StdIO::new(false);
    let mut interactor = IOInteractor::new(io);
    let graph = interactor.read_graph();
    let input = interactor.read_input();
    eprintln!("input: {:?}", input);

    let mut is_fallback = false;
    let solver = GreedySolver::new(TRACKER_ENABLED);
    let fallback_solver = FallbackSolver;

    for _ in 0..N_SUBTASK {
        let n = interactor.read_n();
        if is_fallback {
            fallback_solver.solve(n, &mut interactor, &input, &graph);
        } else {
            solver.solve(n, &mut interactor, &input, &graph);
        }

        let elapsed = interactor.get_elapsed_time();
        if should_fallback(elapsed) {
            eprintln!("switch to fallback solver: {:.3}", elapsed);
            is_fallback = true;
        }
    }

    eprintln!("elapsed: {:.3}", interactor.get_elapsed_time());
}

#[derive(Clone, Debug)]
pub struct Task {
    next_t: i64,
    packet_type: usize,
    path_index: usize,
    is_chunked: bool,
    last_chunk_min_i: Option<usize>,
    packets: Vec<PacketStatus>,
}

#[derive(Clone, Debug)]
pub struct State {
    packets: Vec<Option<Packet>>,
    packet_special_cost: Vec<Option<i64>>,
    /// next_tasks[core_id] := core_idで次に実行するタスク
    next_tasks: Vec<Option<Task>>,
    await_packets: IndexSet,
    received_packets: IndexSet,
    last_received_t: i64,
}

impl State {
    fn new(n: usize, input: &Input) -> Self {
        Self {
            packets: vec![None; n],
            packet_special_cost: vec![None; n],
            next_tasks: vec![None; input.n_cores],
            await_packets: IndexSet::empty(n),
            received_packets: IndexSet::empty(n),
            last_received_t: 0,
        }
    }

    fn is_received_all(&self) -> bool {
        self.received_packets.size() == self.packets.len()
    }
}

#[derive(Clone, Debug, Copy, PartialEq, PartialOrd, Eq, Ord)]
enum Event {
    ReceivePacket,
    ResumeCore(usize), // core_id
}

type EventQueue = BinaryHeap<(Reverse<i64>, Event)>;

pub struct GreedySolver {
    tracker_enabled: bool,
}

impl GreedySolver {
    pub fn new(tracker_enabled: bool) -> Self {
        Self { tracker_enabled }
    }
}

impl Solver for GreedySolver {
    fn solve<I: Interactor>(&self, n: usize, interactor: &mut I, input: &Input, graph: &Graph) {
        let mut tracker = Tracker::new(n, self.tracker_enabled);

        let calculator = DurationCalculator::new(input, graph);

        let mut q: EventQueue = BinaryHeap::new();
        q.push((Reverse(1), Event::ReceivePacket));

        let mut state = State::new(n, input);
        while let Some((t, event)) = q.pop() {
            match event {
                Event::ReceivePacket => {
                    receive_packet(
                        &mut state,
                        t.0,
                        interactor,
                        input,
                        graph,
                        &calculator,
                        &mut q,
                    );
                }
                Event::ResumeCore(core_id) => {
                    // タスクが完了したか確認する
                    if let Some(cur_task) = &state.next_tasks[core_id] {
                        if cur_task.path_index >= graph.paths[cur_task.packet_type].path.len() {
                            complete_task(
                                &mut state,
                                core_id,
                                t.0,
                                input,
                                graph,
                                &calculator,
                                &mut q,
                            );
                            continue;
                        }
                    }

                    process_task(
                        &mut state,
                        core_id,
                        t.0,
                        interactor,
                        input,
                        graph,
                        &mut q,
                        &mut tracker,
                    );
                }
            }
        }

        interactor.send_finish();

        if self.tracker_enabled {
            let score = tracker.dump_score(n, &state.packets, graph, input);
            let calc_score_rate = calc_timeout_score_rate(&score);
            eprintln!("timeout score rate: {:.3}%", calc_score_rate * 100.0);
            tracker.dump_task_logs();
        }
    }
}

/// パケットを処理する
/// chunk_idsがSomeなら、そのチャンクのみ処理する
fn process_packets(
    cur_task: &mut Task,
    cur_t: i64,
    chunk_packets: Option<&[PacketStatus]>,
    core_id: usize,
    node_id: usize,
    packet_special_cost: &Vec<Option<i64>>,
    input: &Input,
    graph: &Graph,
    q: &mut EventQueue,
    interactor: &mut impl Interactor,
    tracker: &mut Tracker,
) {
    let packets = match chunk_packets {
        Some(v) => v,
        None => &cur_task.packets[..],
    };

    let dt = if node_id == SPECIAL_NODE_ID {
        graph.nodes[node_id].costs[packets.len()]
            + packets
                .iter()
                .map(|p| packet_special_cost[p.id].unwrap())
                .sum::<i64>()
    } else {
        graph.nodes[node_id].costs[packets.len()]
    };
    let switch_cost =
        packets.iter().filter(|p| p.is_switching_core).count() as i64 * input.cost_switch;

    cur_task.next_t = cur_t + dt + switch_cost;

    q.push((Reverse(cur_task.next_t), Event::ResumeCore(core_id)));

    interactor.send_execute(
        cur_t,
        core_id,
        node_id,
        packets.len(),
        packets.iter().map(|p| p.id).collect::<Vec<_>>(),
    );

    for p in packets {
        tracker.add_packet_history(
            p.id,
            PacketHistory {
                start_t: cur_t,
                end_t: cur_task.next_t,
                core_id,
            },
        );
    }
    tracker.add_task_log(TaskLog {
        core_id,
        start_t: cur_t,
        end_t: cur_task.next_t,
        node_id,
        batch_size: packets.len(),
        packet_type: cur_task.packet_type,
        path_index: cur_task.path_index,
    });
}

/// タスクを進める
fn process_task(
    state: &mut State,
    core_id: usize,
    cur_t: i64,
    interactor: &mut impl Interactor,
    input: &Input,
    graph: &Graph,
    q: &mut EventQueue,
    tracker: &mut Tracker,
) {
    let cur_task = state.next_tasks[core_id]
        .as_ref()
        .expect("cur_task should be Some");

    let node_id = graph.paths[cur_task.packet_type].path[cur_task.path_index];
    if node_id == SPECIAL_NODE_ID {
        // 特殊ノードに到達した場合は`works`を問い合わせる
        for p in &cur_task.packets {
            if state.packet_special_cost[p.id].is_some() {
                continue;
            }
            let work = interactor.send_query_works(cur_t, p.id).unwrap();
            let mut cost_sum = 0;
            for i in 0..N_SPECIAL {
                if work[i] {
                    cost_sum += graph.special_costs[i];
                }
            }
            state.packet_special_cost[p.id] = Some(cost_sum);
        }
    }

    // node_id = [7,11,13,15,18]は分割して処理する
    // - node_id = SPECIAL_NODE_ID -> 小さく分けて処理する
    // - node_id = CHUNK_NODES -> 1つずつ処理する
    let desired_batch_size = if CHUNK_NODES.contains(&node_id) {
        1
    } else if node_id == SPECIAL_NODE_ID
        && should_chunk_special_node_task(node_id, cur_task, state, core_id, input, graph)
    {
        get_special_node_chunk_size(cur_task)
    } else {
        graph.nodes[node_id].costs.len() - 1
    };

    let cur_task = state.next_tasks[core_id]
        .as_mut()
        .expect("cur_task should be Some");
    cur_task.last_chunk_min_i = None;
    let is_chunk = cur_task.packets.len() > desired_batch_size || cur_task.is_chunked;

    if is_chunk {
        cur_task.is_chunked = true;

        let mut chunk_packets = Vec::with_capacity(desired_batch_size);
        let mut min_i = cur_task.packets.len();
        for (i, p) in cur_task.packets.iter_mut().enumerate() {
            // 処理中だったものを処理済みに変える
            if chunk_packets.len() >= desired_batch_size {
                continue;
            }
            if p.is_advanced {
                continue;
            }

            p.is_advanced = true;
            min_i = min_i.min(i);
            chunk_packets.push(*p);

            // 一度処理をしたパケットはコアのswitchingを消す
            p.is_switching_core = false;
        }

        cur_task.last_chunk_min_i = Some(min_i);

        process_packets(
            cur_task,
            cur_t,
            Some(&chunk_packets),
            core_id,
            node_id,
            &state.packet_special_cost,
            input,
            graph,
            q,
            interactor,
            tracker,
        );

        // チャンクが完了したか確認する
        let all_chunk_finished = cur_task.packets.iter().all(|p| p.is_advanced);
        if all_chunk_finished {
            cur_task.path_index += 1;
            cur_task.is_chunked = false;
            for p in cur_task.packets.iter_mut() {
                p.is_advanced = false;
            }
        }
    } else {
        process_packets(
            cur_task,
            cur_t,
            None,
            core_id,
            node_id,
            &state.packet_special_cost,
            input,
            graph,
            q,
            interactor,
            tracker,
        );

        for p in cur_task.packets.iter_mut() {
            p.is_switching_core = false;
        }

        cur_task.path_index += 1;
    }
}

/// タスク完了時の処理
fn complete_task(
    state: &mut State,
    core_id: usize,
    cur_t: i64,
    input: &Input,
    graph: &Graph,
    calculator: &DurationCalculator,
    q: &mut EventQueue,
) {
    // タスク完了
    state.next_tasks[core_id] = None;

    // 最も優先度の高いタスクを割り当てて開始する
    if state.await_packets.size() > 0 {
        let mut tasks = create_tasks(
            &state,
            cur_t,
            input,
            graph,
            &calculator,
            COMPLETE_TASK_TOP_K,
        );
        if let Some(task) = tasks.pop() {
            // await_packetsから削除する
            for &p in &task.packets {
                state.await_packets.remove(p.id);
            }
            q.push((Reverse(task.next_t), Event::ResumeCore(core_id)));
            state.next_tasks[core_id] = Some(task);
            return;
        }
    }

    // パケットが残っていなければ、他のコアから分割してタスクをもらってくる
    if input.n_cores > 1 {
        let mut other_cores = (0..input.n_cores)
            .filter(|&id| id != core_id)
            .filter(|&id| state.next_tasks[id].is_some())
            .map(|id| {
                (
                    id,
                    estimate_core_duration(state, id, input, graph).estimate(),
                )
            })
            .collect::<Vec<_>>();

        // コアごとに保持している処理が終了するまでの時間が長い順に試す
        other_cores.sort_unstable_by_key(|&(_, duration)| Reverse(cur_t + duration));

        let mut first_cand = None;

        for (other_core_id, _) in other_cores {
            let Some(task) = &state.next_tasks[other_core_id] else {
                continue;
            };

            let Some((task1, task2)) = split_task(cur_t, task) else {
                continue;
            };

            // task2が完了している場合は分割する必要がない
            if task2.path_index == graph.paths[task2.packet_type].path.len() {
                continue;
            }

            let task_end_t = task.next_t
                + estimate_task_duration(&task, input, graph, &state.packet_special_cost)
                    .estimate();
            let task1_end_t = task1.next_t
                + estimate_task_duration(&task1, input, graph, &state.packet_special_cost)
                    .estimate();
            let task2_duration =
                estimate_task_duration(&task2, input, graph, &state.packet_special_cost);
            let task2_end_t = task2.next_t + task2_duration.estimate();

            if task_end_t <= task1_end_t.max(task2_end_t) {
                continue;
            }

            if first_cand.is_none() {
                first_cand = Some((other_core_id, task.next_t, task1, task2));
                continue;
            }

            let task2_end_t = task2.next_t + task2_duration.upper_bound();

            if task2_end_t <= first_cand.as_ref().unwrap().1 {
                first_cand = Some((other_core_id, task.next_t, task1, task2));
                break;
            }
        }

        if let Some((other_core_id, _, task1, mut task2)) = first_cand {
            // NOTE: other_core_idはすでにqに追加されているのでq.pushは不要
            state.next_tasks[other_core_id] = Some(task1);

            task2
                .packets
                .iter_mut()
                .for_each(|p| p.is_switching_core = true);

            q.push((Reverse(task2.next_t), Event::ResumeCore(core_id)));
            state.next_tasks[core_id] = Some(task2);
        }
    }

    // NOTE: なければパケットが来るまで待機で良い
}

/// タスクを分割する
/// 割り当てられたパケットが全て処理中でなければ、next_tを現時刻に設定する
fn split_task(cur_t: i64, task: &Task) -> Option<(Task, Task)> {
    fn build_task_from_split(original: &Task, mut packets: Vec<PacketStatus>, next_t: i64) -> Task {
        let has_non_advanced = packets.iter().any(|p| !p.is_advanced);
        let has_advanced = packets.iter().any(|p| p.is_advanced);
        let all_advanced = !has_non_advanced;
        let is_chunked = has_advanced && has_non_advanced;
        let path_index = if original.is_chunked && all_advanced {
            for p in packets.iter_mut() {
                p.is_advanced = false;
            }
            original.path_index + 1
        } else {
            original.path_index
        };
        Task {
            next_t,
            packet_type: original.packet_type,
            path_index,
            is_chunked,
            packets,
            last_chunk_min_i: None,
        }
    }

    if task.packets.len() < 2 {
        return None;
    }
    let mid = task.packets.len() / 2;

    let mut used = vec![false; task.packets.len()];
    let mut packets1 = Vec::with_capacity(mid + 1);
    let mut packets2 = Vec::with_capacity(mid + 1);

    // バッチ内で、すでに処理が完了しているパケット
    if let Some(min_i) = task.last_chunk_min_i {
        for i in 0..min_i {
            if packets2.len() >= mid {
                break;
            }
            packets2.push(task.packets[i]);
            used[i] = true;
        }
    }
    // チャンクされているバッチ内で、まだ処理されていないパケット
    if task.is_chunked && packets2.len() < mid {
        used.fill(false);
        packets2.clear();

        for i in 0..task.packets.len() {
            if packets2.len() >= mid {
                break;
            }
            if !task.packets[i].is_advanced {
                packets2.push(task.packets[i]);
                used[i] = true;
            }
        }
    }

    let task2_ready = packets2.len() >= mid;

    let next_t1 = task.next_t;
    let next_t2 = if task2_ready { cur_t } else { task.next_t };

    // 残りを追加する
    for i in 0..task.packets.len() {
        if used[i] {
            continue;
        }
        if packets1.len() < mid || task2_ready {
            packets1.push(task.packets[i]);
        } else {
            packets2.push(task.packets[i]);
        }
    }

    let task1 = build_task_from_split(task, packets1, next_t1);
    let task2 = build_task_from_split(task, packets2, next_t2);

    Some((task1, task2))
}

/// パケット受信イベントの処理
fn receive_packet(
    state: &mut State,
    cur_t: i64,
    interactor: &mut impl Interactor,
    input: &Input,
    graph: &Graph,
    calculator: &DurationCalculator,
    q: &mut EventQueue,
) {
    // パケットを登録する
    if let Some(packets) = interactor.send_receive_packets(cur_t) {
        for packet in packets {
            let i = packet.i;
            state.packets[i] = Some(packet);
            state.await_packets.add(i);
            state.received_packets.add(i);
        }

        state.last_received_t = cur_t;
    }

    let n_idle_cores = (0..input.n_cores)
        .filter(|core_id| state.next_tasks[*core_id].is_none())
        .count();
    if n_idle_cores > 0 {
        // packet_typeごとにタスクを作成して、優先度を計算する
        let top_k = RECEIVE_TASK_TOP_K.max(n_idle_cores);
        let mut tasks = create_tasks(&state, cur_t, input, graph, &calculator, top_k);

        // 空いているコアがある限り優先度順にタスクを割り当てる
        for core_id in 0..input.n_cores {
            if state.next_tasks[core_id].is_some() {
                continue;
            }
            if let Some(task) = tasks.pop() {
                // await_packetsから削除する
                for &p in &task.packets {
                    state.await_packets.remove(p.id);
                }
                q.push((Reverse(task.next_t), Event::ResumeCore(core_id)));
                state.next_tasks[core_id] = Some(task);
            }
        }
    }

    // 全てのパケットを受信していれば次の受信イベントは登録しない
    if state.is_received_all() {
        return;
    }

    // 次のパケット受信イベントを登録する
    let dt = get_receive_dt(cur_t, state, input);
    let next_t = cur_t + dt;
    if next_t <= MAX_T {
        q.push((Reverse(next_t), Event::ReceivePacket));
    }
}

/// タスクを作成する
/// 後ろほど優先度が高い
fn create_tasks(
    state: &State,
    cur_t: i64,
    input: &Input,
    graph: &Graph,
    calculator: &DurationCalculator,
    top_k: usize,
) -> Vec<Task> {
    let mut packets = vec![vec![]; N_PACKET_TYPE];
    for await_packet in state.await_packets.iter() {
        let packet = state.packets[*await_packet].as_ref().unwrap();
        packets[packet.packet_type].push(packet);
    }

    let mut tasks = vec![];

    fn next_t(received_t: i64, cur_t: i64, cost_r: i64) -> i64 {
        (received_t + cost_r).max(cur_t)
    }
    fn is_timeouted(packet: &Packet, cur_t: i64, cost_r: i64, min_duration: &Duration) -> bool {
        packet.time_limit < next_t(packet.received_t, cur_t, cost_r) + min_duration.estimate()
    }

    let mut push_task = |packet_type: usize, cur_ids: &Vec<usize>, min_time_limit: i64| {
        // timeoutしたものは無視されているので、ここでmax_received_tを計算し直す
        let mut max_received_t = -INF;
        for &id in cur_ids {
            let packet = state.packets[id].as_ref().unwrap();
            max_received_t = max_received_t.max(packet.received_t);
        }

        let batch_duration = calculator
            .get_path_duration(packet_type, cur_ids.len(), 0)
            .estimate();
        let next_t = next_t(max_received_t, cur_t, input.cost_r);
        let afford = min_time_limit - (next_t + batch_duration);
        tasks.push((afford, max_received_t, packet_type, cur_ids.clone()));
    };

    for packet_type in 0..N_PACKET_TYPE {
        let min_duration = calculator.get_path_duration(packet_type, MIN_BATCH_SIZE, 0);
        // パケットを締め切り順にソートする
        packets[packet_type].sort_unstable_by_key(|&packet| {
            if is_timeouted(&packet, cur_t, input.cost_r, &min_duration) {
                // そもそも間に合わないパケットは優先度を一番下げる
                INF
            } else {
                packet.time_limit
            }
        });

        let base_max_batch_size =
            get_base_max_batch_size(packet_type, input, state.await_packets.size());
        let mut max_batch_size = base_max_batch_size;

        if state.is_received_all() {
            // 全てのパケットを受信していたら、均等になるようにバッチサイズを決める
            max_batch_size =
                get_desired_batch_size(packets[packet_type].len(), base_max_batch_size);
        }

        // timeoutしていないパケットでだけ集計する
        let mut min_time_limit = INF;
        let mut max_received_t = -INF;
        let mut cur_ids = Vec::with_capacity(base_max_batch_size);

        for (i, &packet) in packets[packet_type].iter().enumerate() {
            let new_duration = calculator
                .get_path_duration(packet.packet_type, cur_ids.len() + 1, 0)
                .estimate();
            let is_timeouted_packet = is_timeouted(&packet, cur_t, input.cost_r, &min_duration);
            let changed_to_timeout_batch = if is_timeouted_packet {
                // そもそも間に合わないパケットは無視して、バッチを分割しなくて良い
                // バッチサイズが大きくなることで、既存のパケットがtimeoutする場合は分割する
                let next_t = next_t(max_received_t, cur_t, input.cost_r);
                min_time_limit < next_t + new_duration && cur_ids.len() >= 1
            } else {
                let next_t = next_t(packet.received_t.max(max_received_t), cur_t, input.cost_r);
                min_time_limit.min(packet.time_limit) < next_t + new_duration && cur_ids.len() >= 1
            };

            if (changed_to_timeout_batch || cur_ids.len() >= max_batch_size)
                && cur_ids.len() >= MIN_BATCH_SIZE
            {
                // バッチを分割する
                push_task(packet_type, &cur_ids, min_time_limit);

                cur_ids.clear();
                min_time_limit = INF;
                max_received_t = -INF;

                if state.is_received_all() {
                    // 全てのパケットを受信していたら、均等になるようにバッチサイズを決める
                    max_batch_size =
                        get_desired_batch_size(packets[packet_type].len() - i, base_max_batch_size);
                }
            }

            if !is_timeouted_packet {
                min_time_limit = min_time_limit.min(packet.time_limit);
                max_received_t = max_received_t.max(packet.received_t);
            }
            cur_ids.push(packet.i);
        }

        // 残っているidsでタスクを作成する
        if cur_ids.len() > 0 {
            push_task(packet_type, &cur_ids, min_time_limit);
        }
    }

    // 優先度が低い順にソートする（後ろほど優先度が高い、stackとして扱う）
    tasks.sort_unstable_by_key(|&(afford, _, _, _)| afford);
    let tasks = tasks
        .into_iter()
        .take(top_k)
        .map(|(_, max_received_t, packet_type, ids)| {
            Task {
                next_t: next_t(max_received_t, cur_t, input.cost_r),
                packet_type,
                path_index: 0,
                is_chunked: false,
                packets: ids
                    .into_iter()
                    .map(|id| PacketStatus {
                        id,
                        is_advanced: false,
                        is_switching_core: true, // 最初はcore=0から必ずコアを移る
                    })
                    .collect(),
                last_chunk_min_i: None,
            }
        })
        .collect::<Vec<_>>();

    let batches = task_to_batches(&tasks, state, calculator);
    let next_ts: Vec<i64> = (0..input.n_cores)
        .map(|core_id| cur_t + estimate_core_duration(state, core_id, input, graph).estimate())
        .collect();

    let (_, mut best_order) = optimize_task(next_ts, batches);

    best_order.reverse(); // stackとして扱うため、逆順にする
    best_order
        .into_iter()
        .map(|idx| tasks[idx].clone())
        .collect::<Vec<_>>()
}

#[derive(Clone, Debug)]
struct Batch {
    time_limits: Vec<i64>,
    duration: Duration,
    next_t: i64,
}

fn optimize_task(next_ts: Vec<i64>, batches: Vec<Batch>) -> (f64, Vec<usize>) {
    struct Context {
        best_order: Vec<usize>,
        best_timeout: f64,
        next_ts: Vec<i64>,
        cur_ts: Vec<i64>,
        batches: Vec<Batch>,
    }
    impl Context {
        fn evaluate_timeout(&mut self, order: &Vec<usize>) -> f64 {
            let mut timeouts = 0.;
            for i in 0..self.cur_ts.len() {
                self.cur_ts[i] = self.next_ts[i];
            }
            for &idx in order {
                let core_id = self
                    .cur_ts
                    .iter()
                    .enumerate()
                    .min_by_key(|&(_, &t)| t)
                    .unwrap()
                    .0;
                let batch = &self.batches[idx];
                let start_t = self.cur_ts[core_id].max(batch.next_t);
                let lb = (start_t + batch.duration.lower_bound()) as f64;
                let ub = (start_t + batch.duration.upper_bound()) as f64;
                for &tl in &batch.time_limits {
                    timeouts += (1. - (tl as f64 - lb) / (ub - lb)).clamp(0.0, 1.0);
                }
                self.cur_ts[core_id] = start_t + batch.duration.estimate();
            }
            timeouts
        }
    }
    let mut order = (0..batches.len()).collect::<Vec<_>>();
    let mut ctx = Context {
        best_order: order.clone(),
        best_timeout: 1e10,
        next_ts: next_ts.clone(),
        cur_ts: next_ts,
        batches: batches,
    };
    ctx.best_timeout = ctx.evaluate_timeout(&order);

    if order.len() <= PERMUTE_TASK_THRESHOLD {
        fn permute(order: &mut Vec<usize>, l: usize, ctx: &mut Context) {
            if l == order.len() {
                let timeout = ctx.evaluate_timeout(order);
                if timeout < ctx.best_timeout {
                    ctx.best_timeout = timeout;
                    ctx.best_order = order.clone();
                }
                return;
            }
            for i in l..order.len() {
                order.swap(l, i);
                permute(order, l + 1, ctx);
                order.swap(l, i);
            }
        }
        permute(&mut order, 0, &mut ctx);
    } else {
        let n = order.len();
        let mut rnd = Rnd::new(123456789);
        for _ in 0..OPTIMIZE_TASK_ITERATION {
            let i = rnd.gen_index(n - 1);
            let j = (i + 1 + rnd.gen_index(2)).min(n - 1);
            order.swap(i, j);
            let timeout = ctx.evaluate_timeout(&order);
            if timeout <= ctx.best_timeout {
                if timeout < ctx.best_timeout {
                    ctx.best_order = order.clone();
                }
                ctx.best_timeout = timeout;
            } else {
                order.swap(i, j);
            }
        }
    }

    (ctx.best_timeout, ctx.best_order)
}

fn task_to_batches(
    tasks: &Vec<Task>,
    state: &State,
    calculator: &DurationCalculator,
) -> Vec<Batch> {
    let mut batches = Vec::with_capacity(tasks.len());
    for task in tasks {
        let time_limits = task
            .packets
            .iter()
            .map(|p| state.packets[p.id].as_ref().unwrap().time_limit)
            .collect();
        let duration = calculator.get_path_duration(task.packet_type, task.packets.len(), 0);
        let next_t = task.next_t;
        batches.push(Batch {
            time_limits,
            duration,
            next_t,
        });
    }
    batches
}
