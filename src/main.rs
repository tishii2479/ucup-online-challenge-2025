mod core;
mod interactor;
mod libb;

use std::{cmp::Reverse, collections::BinaryHeap};

use crate::{core::*, interactor::*, libb::*};

const TRACK: bool = true;
const MAX_BATCH_SIZE: usize = 32;
const SPECIAL_COST_SUM: i64 = 3 + 5 + 8 + 11 + 14 + 17 + 29 + 73;

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
struct Task {
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
    /// cur_tasks[core_id] := core_idで次に実行するタスク
    cur_tasks: Vec<Option<Task>>,
    /// idle_tasks[core_id] := core_idで待機中のタスク
    /// 上に積まれているほど優先度が高い
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

        let mut q: EventQueue = BinaryHeap::new();
        q.push((Reverse(1), Event::ReceivePacket));

        let mut state = State::new(n, input);
        while let Some((t, event)) = q.pop() {
            match event {
                Event::ReceivePacket => {
                    receive_packet(&mut state, t.0, interactor, input, graph, &mut q);
                }
                Event::ResumeCore(core_id) => {
                    // タスクが完了したか確認する
                    if let Some(cur_task) = &state.cur_tasks[core_id] {
                        if cur_task.path_index >= graph.paths[cur_task.packet_type].path.len() {
                            complete_task(&mut state, core_id, t.0, input, graph, &mut q);
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

        tracker.dump_score(n, &state.packets, graph, input);
        tracker.dump_task_logs();
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
    let cur_task = state.cur_tasks[core_id]
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
        }

        // TODO: バッチ内に間に合わなそうなパケットが判明したら、分割して処理したり、別のコアに移したりする
    }

    let node_id = graph.paths[cur_task.packet_type].path[cur_task.path_index];
    let max_batch_size = graph.nodes[node_id].costs.len() - 1;
    let is_chunk = cur_task.packets.len() > max_batch_size || cur_task.is_chunked;

    if is_chunk {
        cur_task.is_chunked = true;

        let mut chunk_packets = Vec::with_capacity(max_batch_size);
        for p in cur_task.packets.iter_mut() {
            if chunk_packets.len() >= max_batch_size {
                break;
            }
            if p.is_advanced {
                continue;
            }
            p.is_advanced = true;
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
    t: i64,
    input: &Input,
    graph: &Graph,
    q: &mut EventQueue,
) {
    // タスク完了
    state.cur_tasks[core_id] = None;

    // idle_tasksからタスクを取得する
    if let Some(task) = state.idle_tasks[core_id].pop() {
        q.push((Reverse(task.next_t), Event::ResumeCore(core_id)));
        state.cur_tasks[core_id] = Some(task);
        return;
    }

    // idle_taskがなければ、最も優先度の高いタスクを割り当てて開始する
    let mut tasks = create_tasks(&state, t, input, &graph);
    if let Some(task) = tasks.pop() {
        // await_packetsから削除する
        for &p in &task.packets {
            state.await_packets.remove(p.id);
        }
        q.push((Reverse(task.next_t), Event::ResumeCore(core_id)));
        state.cur_tasks[core_id] = Some(task);
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
                state.cur_tasks[core_id] = Some(task);
                return;
            }
        }

        let busiest_core_id = (0..input.n_cores)
            .filter(|&id| id != core_id)
            .max_by_key(|&id| estimate_core_duration(&state, id, input, graph).estimate())
            .unwrap();

        // idle_tasksがあるならそのままもらってくる
        if let Some(task) = state.idle_tasks[busiest_core_id].pop() {
            q.push((Reverse(task.next_t), Event::ResumeCore(core_id)));
            state.cur_tasks[core_id] = Some(task);
            return;
        }

        // busiest_coreのcur_taskを半分に分ける
        let Some(task) = &state.cur_tasks[busiest_core_id] else {
            return;
        };

        let Some((task1, mut task2)) = split_task(task) else {
            return;
        };

        // task2が完了している場合は分割する必要がない
        if task2.path_index == graph.paths[task2.packet_type].path.len() {
            return;
        }

        // NOTE: busiest_core_idはすでにqに追加されているのでq.pushは不要
        state.cur_tasks[busiest_core_id] = Some(task1);

        task2
            .packets
            .iter_mut()
            .for_each(|p| p.is_switching_core = true);
        q.push((Reverse(task2.next_t), Event::ResumeCore(core_id)));
        state.cur_tasks[core_id] = Some(task2);
    }

    // NOTE: なければパケットが来るまで待機で良い
}

fn build_task_from_split(original: &Task, mut packets: Vec<PacketStatus>) -> Task {
    let is_chunked =
        packets.iter().any(|p| p.is_advanced) && packets.iter().any(|p| !p.is_advanced);
    let path_index = if original.is_chunked && packets.iter().all(|p| p.is_advanced) {
        for p in packets.iter_mut() {
            p.is_advanced = false;
        }
        original.path_index + 1
    } else {
        original.path_index
    };
    Task {
        next_t: original.next_t,
        packet_type: original.packet_type,
        path_index,
        is_chunked,
        packets,
    }
}

/// タスクを分割する
fn split_task(task: &Task) -> Option<(Task, Task)> {
    if task.packets.len() < 2 {
        return None;
    }
    let mid = task.packets.len() / 2;

    let mut packets1 = Vec::with_capacity(mid + 1);
    let mut packets2 = Vec::with_capacity(mid + 1);

    // idsを交互に追加する
    for id in 0..task.packets.len() {
        if id % 2 == 0 {
            packets1.push(task.packets[id]);
        } else {
            packets2.push(task.packets[id]);
        }
    }

    let task1 = build_task_from_split(task, packets1);
    let task2 = build_task_from_split(task, packets2);

    Some((task1, task2))
}

fn estimate_core_duration(state: &State, core_id: usize, input: &Input, graph: &Graph) -> Duration {
    let mut ret = Duration::new(0, 0);
    if let Some(cur_task) = &state.cur_tasks[core_id] {
        ret.add(&estimate_task_duration(
            cur_task,
            input,
            graph,
            &state.packet_special_cost,
        ));
    }
    for idle_task in &state.idle_tasks[core_id] {
        ret.add(&estimate_task_duration(
            idle_task,
            input,
            graph,
            &state.packet_special_cost,
        ));
    }
    ret
}

/// パケット受信イベントの処理
fn receive_packet(
    state: &mut State,
    t: i64,
    interactor: &mut impl Interactor,
    input: &Input,
    graph: &Graph,
    q: &mut EventQueue,
) {
    // パケットを登録する
    let (_, packets) = interactor.send_receive_packets(t);
    for packet in packets {
        let i = packet.i;
        state.packets[i] = Some(packet);
        state.await_packets.add(i);
    }

    // packet_typeごとにタスクを作成して、優先度を計算する
    let mut tasks = create_tasks(&state, t, input, &graph);

    // 空いているコアがある限り優先度順にタスクを割り当てる
    for core_id in 0..input.n_cores {
        if state.cur_tasks[core_id].is_some() {
            continue;
        }
        if let Some(task) = tasks.pop() {
            // await_packetsから削除する
            for &p in &task.packets {
                state.await_packets.remove(p.id);
            }
            q.push((Reverse(task.next_t), Event::ResumeCore(core_id)));
            state.cur_tasks[core_id] = Some(task);
        }
    }

    // 残っているタスクで、割り込むべきタスクがあればコアのタスクに差し込む
    // while let Some(task) = tasks.pop() {
    // タスクを差し込まないとtimeoutが発生するかどうか調べる
    // }

    // 次のパケット受信イベントを登録する
    let next_t = t + input.cost_r * 10; // TODO: 調整
    if next_t <= LAST_PACKET_T {
        q.push((Reverse(next_t), Event::ReceivePacket));
    }
}

/// タスクを終了するのにかかる時間を見積もる
fn estimate_task_duration(
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
        let need_switch = p.is_switching_core && (p.is_advanced == false || has_next_node);
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

/// packet_typeをbatch_sizeで処理する場合の所要時間を見積もる
/// TODO: 前計算
fn estimate_path_duration(
    packet_type: usize,
    batch_size: usize,
    input: &Input,
    graph: &Graph,
) -> Duration {
    let mut ret = Duration::new(0, 0);

    // core=0から移るswitch costを考慮する
    ret.fixed += batch_size as i64 * input.cost_switch;

    for node_id in &graph.paths[packet_type].path {
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

/// タスクを作成する
/// 後ろほど優先度が高い
fn create_tasks(state: &State, cur_t: i64, input: &Input, graph: &Graph) -> Vec<Task> {
    const INF: i64 = 1_000_000_000_000;

    let mut packets = vec![vec![]; N_PACKET_TYPE];
    for await_packet in state.await_packets.iter() {
        let packet = state.packets[*await_packet].as_ref().unwrap();
        packets[packet.packet_type].push(packet);
    }

    let mut tasks = vec![];

    fn next_t(received_t: i64, cur_t: i64, cost_r: i64) -> i64 {
        (received_t + cost_r).max(cur_t)
    }
    fn is_timeouted(packet: &Packet, cur_t: i64, cost_r: i64, duration_b1: &Duration) -> bool {
        packet.time_limit < next_t(packet.received_t, cur_t, cost_r) + duration_b1.lower_bound()
    }

    let mut push_task = |packet_type: usize, cur_ids: &Vec<usize>, min_time_limit: i64| {
        // timeoutしたものは無視されているので、ここでmax_received_tを計算し直す
        let mut max_received_t = -INF;
        for &id in cur_ids {
            let packet = state.packets[id].as_ref().unwrap();
            max_received_t = max_received_t.max(packet.received_t);
        }

        let batch_duration = estimate_path_duration(packet_type, cur_ids.len(), input, graph);
        let next_t = next_t(max_received_t, cur_t, input.cost_r);
        let afford = min_time_limit - (next_t + batch_duration.estimate());
        tasks.push((afford, max_received_t, packet_type, cur_ids.clone()));
    };

    for packet_type in 0..N_PACKET_TYPE {
        let duration_b1 = estimate_path_duration(packet_type, 1, input, graph);
        // パケットを締め切り順にソートする
        packets[packet_type].sort_by_key(|&packet| {
            if is_timeouted(&packet, cur_t, input.cost_r, &duration_b1) {
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
                estimate_path_duration(packet.packet_type, cur_ids.len() + 1, input, graph);
            let is_timeouted_packet = is_timeouted(&packet, cur_t, input.cost_r, &duration_b1);
            let changed_to_timeout_batch = if is_timeouted_packet {
                // そもそも間に合わないパケットは無視して、バッチを分割しなくて良い
                // バッチサイズが大きくなることで、既存のパケットがtimeoutする場合は分割する
                let next_t = next_t(max_received_t, cur_t, input.cost_r);
                min_time_limit < next_t + new_duration.estimate() && cur_ids.len() >= 1
            } else {
                let next_t = next_t(packet.received_t.max(max_received_t), cur_t, input.cost_r);
                min_time_limit.min(packet.time_limit) < next_t + new_duration.estimate()
                    && cur_ids.len() >= 1
            };

            if changed_to_timeout_batch || cur_ids.len() >= MAX_BATCH_SIZE {
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
    tasks.sort_by_key(|&(afford, _, _, _)| -afford);
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
                        is_advanced: false,
                        is_switching_core: true, // 最初はcore=0から必ずコアを移る
                    })
                    .collect(),
            }
        })
        .collect()
}

#[derive(Debug, Clone)]
struct Duration {
    fixed: i64,
    special_node_count: i64,
}

impl Duration {
    fn new(fixed: i64, special_node_count: i64) -> Self {
        Self {
            fixed,
            special_node_count,
        }
    }

    fn add(&mut self, other: &Duration) {
        self.fixed += other.fixed;
        self.special_node_count += other.special_node_count;
    }

    fn lower_bound(&self) -> i64 {
        self.fixed
    }

    fn upper_bound(&self) -> i64 {
        self.fixed + self.special_node_count * SPECIAL_COST_SUM
    }

    fn estimate(&self) -> i64 {
        // TODO: worksを開示したら更新する
        self.upper_bound()
    }
}
