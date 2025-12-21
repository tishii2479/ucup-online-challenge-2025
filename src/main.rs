mod calculator;
mod core;
mod interactor;
mod libb;

use std::{cmp::Reverse, collections::BinaryHeap};

use crate::{calculator::*, core::*, interactor::*, libb::*};

const INF: i64 = 1_000_000_000_000;
const TRACK: bool = true;
const B: usize = 16;
const MAX_BATCH_SIZE: [usize; N_PACKET_TYPE] = [B, B, B, B, B, B, B];
const MIN_BATCH_SIZE: usize = 1;
const ALPHA: f64 = 0.8;
const INIT_WEIGHT: f64 = 50.;

fn main() {
    let io = StdIO::new(false);
    let interactor = IOInteractor::new(io);
    let solver = GreedySolver::new(TRACK);
    let runner = Runner;
    let _ = runner.run(solver, interactor);
}

pub struct Runner;

impl Runner {
    pub fn run(&self, solver: impl Solver, mut interactor: impl Interactor) {
        let graph = interactor.read_graph();
        let input = interactor.read_input();
        eprintln!("input: {:?}", input);

        for _ in 0..N_SUBTASK {
            let n = interactor.read_n();
            solver.solve(n, &mut interactor, &input, &graph);
        }
    }
}

#[derive(Clone, Debug)]
pub struct Task {
    next_t: i64,
    packet_type: usize,
    path_index: usize,
    is_chunked: bool,
    packets: Vec<PacketStatus>,
}

#[derive(Clone, Debug)]
pub struct State {
    packets: Vec<Option<Packet>>,
    packet_special_cost: Vec<Option<i64>>,
    /// next_tasks[core_id] := core_idで次に実行するタスク
    next_tasks: Vec<Option<Task>>,
    /// idle_tasks[core_id] := core_idで待機中のタスク
    /// 上に積まれているほど優先度が高い
    idle_tasks: Vec<Vec<Task>>,
    await_packets: IndexSet,
    received_packets: IndexSet,
    duration_estimator: DurationEstimator,
}

impl State {
    fn new(n: usize, input: &Input) -> Self {
        Self {
            packets: vec![None; n],
            packet_special_cost: vec![None; n],
            next_tasks: vec![None; input.n_cores],
            idle_tasks: vec![Vec::with_capacity(10); input.n_cores],
            await_packets: IndexSet::empty(n),
            received_packets: IndexSet::empty(n),
            duration_estimator: DurationEstimator::new(n, ALPHA, INIT_WEIGHT),
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
                    receive_packet(&mut state, t.0, interactor, input, &calculator, &mut q);
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

    interactor.send_execute(cur_t, core_id, node_id, packets.len(), &packets);

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
    t: i64,
    interactor: &mut impl Interactor,
    input: &Input,
    graph: &Graph,
    q: &mut EventQueue,
    tracker: &mut Tracker,
) {
    let cur_task = state.next_tasks[core_id]
        .as_mut()
        .expect("cur_task should be Some");

    let node_id = graph.paths[cur_task.packet_type].path[cur_task.path_index];
    if node_id == SPECIAL_NODE_ID {
        // 特殊ノードに到達した場合は`works`を問い合わせる
        for p in &cur_task.packets {
            if state.packet_special_cost[p.id].is_some() {
                continue;
            }
            let work = interactor.send_query_works(t, p.id).unwrap();
            let mut cost_sum = 0;
            for i in 0..N_SPECIAL {
                if work[i] {
                    cost_sum += graph.special_costs[i];
                }
            }
            state.packet_special_cost[p.id] = Some(cost_sum);
            state
                .duration_estimator
                .update_alpha(cur_task.packet_type, cost_sum);
        }

        // TODO: バッチ内に間に合わなそうなパケットが判明したら、分割して処理したり、別のコアに移したりする
        // TODO: insert task
    }

    // node_id = [7,11,13,15,18]は分割して処理する
    // - TODO: node_id = SPECIAL_NODE_ID -> 小さく分けて処理する
    // - node_id = 11 -> 1つずつ処理する
    let desired_batch_size = if [11, 13, 15, 18].contains(&node_id) {
        // 分割して処理する
        1
    } else {
        graph.nodes[node_id].costs.len() - 1
    };
    let is_chunk = cur_task.packets.len() > desired_batch_size || cur_task.is_chunked;

    if is_chunk {
        cur_task.is_chunked = true;

        let mut chunk_packets = Vec::with_capacity(desired_batch_size);
        for p in cur_task.packets.iter_mut() {
            // 処理中だったものを処理済みに変える
            if p.chunk_status == ChunkStatus::Processing {
                p.chunk_status = ChunkStatus::Advanced;
                continue;
            }
            if chunk_packets.len() >= desired_batch_size {
                continue;
            }
            if p.chunk_status == ChunkStatus::Advanced {
                continue;
            }
            p.chunk_status = ChunkStatus::Processing;
            chunk_packets.push(*p);

            // 一度処理をしたパケットはコアのswitchingを消す
            p.is_switching_core = false;
        }

        process_packets(
            cur_task,
            t,
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
        let all_chunk_finished = cur_task
            .packets
            .iter()
            .all(|p| p.chunk_status != ChunkStatus::NotAdvanced);
        if all_chunk_finished {
            cur_task.path_index += 1;

            cur_task.is_chunked = false;
            for p in cur_task.packets.iter_mut() {
                p.chunk_status = ChunkStatus::NotAdvanced;
            }
        }
    } else {
        process_packets(
            cur_task,
            t,
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
    // 遅延したパケット
    for p in &state.next_tasks[core_id].as_ref().unwrap().packets {
        let packet = state.packets[p.id].as_ref().unwrap();
        if cur_t > packet.time_limit {
            let d = calculator.get_path_duration(packet.packet_type, 1, 0);
            eprintln!(
                "timeout: id={:5} arr={:8}, to={:4} (lb={:4}, ub={:4}), tl={:8}, end={:8}",
                p.id,
                packet.arrive,
                packet.timeout,
                d.lower_bound(),
                d.upper_bound(),
                packet.time_limit,
                cur_t
            );
        }
    }

    // タスク完了
    state.next_tasks[core_id] = None;

    // idle_tasksからタスクを取得する
    if let Some(task) = state.idle_tasks[core_id].pop() {
        q.push((Reverse(cur_t), Event::ResumeCore(core_id)));
        state.next_tasks[core_id] = Some(task);
        return;
    }

    // idle_taskがなければ、最も優先度の高いタスクを割り当てて開始する
    let mut tasks = create_tasks(&state, cur_t, input, &calculator);
    if let Some(task) = tasks.pop() {
        // await_packetsから削除する
        for &p in &task.packets {
            state.await_packets.remove(p.id);
        }
        q.push((Reverse(task.next_t), Event::ResumeCore(core_id)));
        state.next_tasks[core_id] = Some(task);
        return;
    }

    // パケットが残っていなければ、他のコアから分割してタスクをもらってくる
    if input.n_cores > 1 {
        // idle_tasksがあるならそのままもらってくる
        for other_core_id in 0..input.n_cores {
            if other_core_id == core_id {
                continue;
            }
            if let Some(task) = state.idle_tasks[other_core_id].pop() {
                q.push((Reverse(task.next_t), Event::ResumeCore(core_id)));
                state.next_tasks[core_id] = Some(task);
                return;
            }
        }

        let mut other_cores = (0..input.n_cores)
            .filter(|&id| id != core_id)
            .filter(|&id| state.next_tasks[id].is_some())
            .collect::<Vec<usize>>();

        // コアごとに保持している処理が終了するまでの時間が長い順に試す
        other_cores.sort_by_key(|&id| {
            let duration = estimate_core_duration(state, id, input, graph);
            let duration = state.duration_estimator.estimate(
                &duration,
                state.next_tasks[id].as_ref().unwrap().packet_type,
            );
            Reverse(cur_t + duration)
        });

        for other_core_id in other_cores {
            let Some(task) = &state.next_tasks[other_core_id] else {
                continue;
            };

            let Some((task1, mut task2)) = split_task(cur_t, task) else {
                continue;
            };

            // task2が完了している場合は分割する必要がない
            if task2.path_index == graph.paths[task2.packet_type].path.len() {
                continue;
            }

            // NOTE: other_core_idはすでにqに追加されているのでq.pushは不要
            state.next_tasks[other_core_id] = Some(task1);

            task2
                .packets
                .iter_mut()
                .for_each(|p| p.is_switching_core = true);

            q.push((Reverse(task2.next_t), Event::ResumeCore(core_id)));
            state.next_tasks[core_id] = Some(task2);
            break;
        }
    }

    // NOTE: なければパケットが来るまで待機で良い
}

/// タスクを分割する
/// 割り当てられたパケットが全て処理中でなければ、next_tを現時刻に設定する
fn split_task(cur_t: i64, task: &Task) -> Option<(Task, Task)> {
    if task.packets.len() < 2 {
        return None;
    }
    let mid = task.packets.len() / 2;

    let mut used = vec![false; task.packets.len()];
    let mut packets1 = Vec::with_capacity(mid + 1);
    let mut packets2 = Vec::with_capacity(mid + 1);

    // 1. ChunkStatus::Advancedを優先的にpackets2に割り当てる
    for i in 0..task.packets.len() {
        if packets2.len() >= mid {
            break;
        }
        if task.packets[i].chunk_status == ChunkStatus::Advanced {
            packets2.push(task.packets[i]);
            used[i] = true;
        }
    }

    let task2_can_process_immediately = packets2.len() >= mid;

    let next_t1 = task.next_t;
    let next_t2 = if task2_can_process_immediately {
        cur_t
    } else {
        task.next_t
    };

    // 2. 残りを追加する
    for i in 0..task.packets.len() {
        if used[i] {
            continue;
        }
        if packets1.len() < mid {
            packets1.push(task.packets[i]);
        } else {
            packets2.push(task.packets[i]);
        }
    }

    let task1 = build_task_from_split(task, packets1, next_t1);
    let task2 = build_task_from_split(task, packets2, next_t2);

    Some((task1, task2))
}

fn build_task_from_split(original: &Task, mut packets: Vec<PacketStatus>, next_t: i64) -> Task {
    let is_chunked = packets
        .iter()
        .any(|p| p.chunk_status != ChunkStatus::NotAdvanced)
        && packets
            .iter()
            .any(|p| p.chunk_status == ChunkStatus::NotAdvanced);
    let path_index = if original.is_chunked
        && packets
            .iter()
            .all(|p| p.chunk_status != ChunkStatus::NotAdvanced)
    {
        for p in packets.iter_mut() {
            p.chunk_status = ChunkStatus::NotAdvanced;
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
    }
}

/// パケット受信イベントの処理
fn receive_packet(
    state: &mut State,
    cur_t: i64,
    interactor: &mut impl Interactor,
    input: &Input,
    calculator: &DurationCalculator,
    q: &mut EventQueue,
) {
    // パケットを登録する
    let (_, packets) = interactor.send_receive_packets(cur_t);
    let received_packet = packets.len() > 0;
    for packet in packets {
        let i = packet.i;
        state.packets[i] = Some(packet);
        state.await_packets.add(i);
        state.received_packets.add(i);
    }

    if received_packet {
        // packet_typeごとにタスクを作成して、優先度を計算する
        let mut tasks = create_tasks(&state, cur_t, input, &calculator);

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

        // TODO: 残っているタスクで、割り込むべき & 割り込めるタスクがあれば差し込む
    }

    // 全てのパケットを受信していれば次の受信イベントは登録しない
    if state.is_received_all() {
        return;
    }

    // 次のパケット受信イベントを登録する
    let dt = if received_packet {
        input.cost_r
    } else if state.received_packets.size() > 0 {
        input.cost_r
    } else {
        input.cost_r * 2
    };
    let next_t = cur_t + dt;
    if next_t <= LAST_PACKET_T {
        q.push((Reverse(next_t), Event::ReceivePacket));
    }
}

/// タスクを作成する
/// 後ろほど優先度が高い
fn create_tasks(
    state: &State,
    cur_t: i64,
    input: &Input,
    calculator: &DurationCalculator,
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
    fn is_timeouted(
        packet: &Packet,
        cur_t: i64,
        cost_r: i64,
        min_duration: &Duration,
        state: &State,
    ) -> bool {
        let duration = state
            .duration_estimator
            .estimate(min_duration, packet.packet_type);
        packet.time_limit < next_t(packet.received_t, cur_t, cost_r) + duration
    }

    let mut push_task = |packet_type: usize, cur_ids: &Vec<usize>, min_time_limit: i64| {
        // timeoutしたものは無視されているので、ここでmax_received_tを計算し直す
        let mut max_received_t = -INF;
        for &id in cur_ids {
            let packet = state.packets[id].as_ref().unwrap();
            max_received_t = max_received_t.max(packet.received_t);
        }

        let batch_duration = calculator.get_path_duration(packet_type, cur_ids.len(), 0);
        let next_t = next_t(max_received_t, cur_t, input.cost_r);
        let duration = state
            .duration_estimator
            .estimate(&batch_duration, packet_type);
        let afford = min_time_limit - (next_t + duration);
        tasks.push((afford, max_received_t, packet_type, cur_ids.clone()));
    };

    for packet_type in 0..N_PACKET_TYPE {
        let min_duration = calculator.get_path_duration(packet_type, MIN_BATCH_SIZE, 0);
        // パケットを締め切り順にソートする
        packets[packet_type].sort_by_key(|&packet| {
            if is_timeouted(&packet, cur_t, input.cost_r, &min_duration, state) {
                // そもそも間に合わないパケットは優先度を一番下げる
                INF
            } else {
                packet.time_limit
            }
        });

        let mut cur_ids = vec![];

        // timeoutしていないパケットでだけ集計する
        let mut min_time_limit = INF;
        let mut max_received_t = -INF;

        for &packet in &packets[packet_type] {
            let new_duration =
                calculator.get_path_duration(packet.packet_type, cur_ids.len() + 1, 0);
            let is_timeouted_packet =
                is_timeouted(&packet, cur_t, input.cost_r, &min_duration, state);
            let duration = state
                .duration_estimator
                .estimate(&new_duration, packet.packet_type);
            let changed_to_timeout_batch = if is_timeouted_packet {
                // そもそも間に合わないパケットは無視して、バッチを分割しなくて良い
                // バッチサイズが大きくなることで、既存のパケットがtimeoutする場合は分割する
                let next_t = next_t(max_received_t, cur_t, input.cost_r);
                min_time_limit < next_t + duration && cur_ids.len() >= 1
            } else {
                let next_t = next_t(packet.received_t.max(max_received_t), cur_t, input.cost_r);
                min_time_limit.min(packet.time_limit) < next_t + duration && cur_ids.len() >= 1
            };

            if changed_to_timeout_batch || cur_ids.len() >= MAX_BATCH_SIZE[packet_type] {
                // バッチを分割する
                push_task(packet_type, &cur_ids, min_time_limit);

                cur_ids.clear();
                min_time_limit = INF;
                max_received_t = -INF;
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
    tasks.sort_by_key(|&(afford, _, _, _)| Reverse(afford));
    tasks
        .into_iter()
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
                        chunk_status: ChunkStatus::NotAdvanced,
                        is_switching_core: true, // 最初はcore=0から必ずコアを移る
                    })
                    .collect(),
            }
        })
        .collect()
}

#[derive(Clone, Debug)]
struct DurationEstimator {
    special_costs: Vec<Vec<i64>>,
    cur_alpha: Vec<f64>,
    init_weight: f64,
}

impl DurationEstimator {
    fn new(n: usize, init_alpha: f64, init_weight: f64) -> Self {
        DurationEstimator {
            special_costs: vec![Vec::with_capacity(n); N_PACKET_TYPE],
            cur_alpha: vec![init_alpha; N_PACKET_TYPE],
            init_weight,
        }
    }

    fn update_alpha(&mut self, packet_type: usize, cost: i64) {
        let alpha = cost as f64 / SPECIAL_COST_SUM as f64;
        let w = self.init_weight + self.special_costs[packet_type].len() as f64;
        self.cur_alpha[packet_type] = (self.cur_alpha[packet_type] as f64 * w + alpha) / (w + 1.0);
        self.special_costs[packet_type].push(cost);
    }

    fn estimate(&self, duration: &Duration, packet_type: usize) -> i64 {
        (duration.lower_bound() as f64
            + self.cur_alpha[packet_type]
                * (duration.upper_bound() - duration.lower_bound()) as f64)
            .round() as i64
    }
}
