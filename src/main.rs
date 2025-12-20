mod core;
mod interactor;
mod libb;

use std::{cmp::Reverse, collections::BinaryHeap};

use crate::{core::*, interactor::*, libb::*};

const DEBUG: bool = true;

fn main() {
    let interactor = IOInteractor::new(StdIO::new(false));
    let solver = GreedySolver::new(DEBUG);
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
    ids: Vec<usize>,
    is_chunked: bool,
    is_advanced: Vec<bool>,
}

#[derive(Clone, Debug)]
pub struct State {
    packets: Vec<Option<Packet>>,
    packet_special_cost: Vec<Option<i64>>,
    /// cur_tasks[core_id] := core_idで次に実行するタスク
    cur_tasks: Vec<Option<Task>>,
    /// idle_tasks[core_id] := core_idで待機中のタスク
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
    chunk_ids: Option<&[usize]>,
    core_id: usize,
    node_id: usize,
    packet_special_cost: &Vec<Option<i64>>,
    input: &Input,
    graph: &Graph,
    q: &mut EventQueue,
    interactor: &mut impl Interactor,
    tracker: &mut Tracker,
) {
    let packet_ids = match chunk_ids {
        Some(v) => v,
        None => &cur_task.ids,
    };

    let dt = if node_id == SPECIAL_NODE_ID {
        graph.nodes[node_id].costs[packet_ids.len()]
            + packet_ids
                .iter()
                .map(|&id| packet_special_cost[id].unwrap())
                .sum::<i64>()
    } else {
        graph.nodes[node_id].costs[packet_ids.len()]
    };
    // 最初はcore=0に受け取っているので、switch costが発生する
    let switch_cost = if cur_task.path_index == 0 {
        packet_ids.len() as i64 * input.cost_switch
    } else {
        0
    };
    cur_task.next_t = cur_t + dt + switch_cost;
    q.push((Reverse(cur_task.next_t), Event::ResumeCore(core_id)));

    interactor.send_execute(cur_t, core_id, node_id, packet_ids.len(), &packet_ids);

    for id in packet_ids {
        tracker.add_packet_history(
            *id,
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
        batch_size: packet_ids.len(),
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
        for &packet_i in &cur_task.ids {
            if state.packet_special_cost[packet_i].is_some() {
                continue;
            }
            let work = interactor.send_query_works(t, packet_i).unwrap();
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

        process_packets(
            cur_task,
            t,
            Some(&chunk_ids),
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

        // 次のノードへ進む
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
        for &packet_i in &task.ids {
            state.await_packets.remove(packet_i);
        }
        q.push((Reverse(task.next_t), Event::ResumeCore(core_id)));
        state.cur_tasks[core_id] = Some(task);
        return;
    }

    // パケットが残っていなければ、他のコアから分割してタスクをもらってくる
    // 最も処理終了時間が長いコアを見つける
    let busiest_core_id = (0..input.n_cores)
        .filter(|&id| id != core_id)
        .max_by_key(|&id| estimate_core_end_t(&state, id, input, graph))
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
    let Some((task1, task2)) = split_task(task) else {
        return;
    };

    // eprintln!("core {} is busiest", busiest_core_id);
    // eprintln!("core: {}", core_id);
    // dbg!(&task);
    // dbg!(&task1);
    // dbg!(&task2);

    // NOTE: busiest_core_idはすでにqに追加されているのでq.pushは不要
    state.cur_tasks[busiest_core_id] = Some(task1);

    // let switch_cost = task2.ids.len() as i64 * input.cost_switch;
    // task2.next_t += switch_cost;
    q.push((Reverse(task2.next_t), Event::ResumeCore(core_id)));
    state.cur_tasks[core_id] = Some(task2);

    // NOTE: なければパケットが来るまで待機で良い
}

/// タスクを分割する
fn split_task(task: &Task) -> Option<(Task, Task)> {
    let Some(mid) = task.ids.len().checked_div(2) else {
        return None;
    };

    // idsを交互に追加する
    let mut task1_ids = Vec::with_capacity(mid + 1);
    let mut task1_is_advanced = Vec::with_capacity(mid + 1);
    let mut task2_ids = Vec::with_capacity(mid + 1);
    let mut task2_is_advanced = Vec::with_capacity(mid + 1);

    for id in 0..task.ids.len() {
        if id % 2 == 0 {
            task1_ids.push(task.ids[id]);
            task1_is_advanced.push(task.is_advanced[id]);
        } else {
            task2_ids.push(task.ids[id]);
            task2_is_advanced.push(task.is_advanced[id]);
        }
    }

    let task1_is_chunked =
        task1_is_advanced.iter().any(|&b| b) && task1_is_advanced.iter().any(|&b| !b);
    let task2_is_chunked =
        task2_is_advanced.iter().any(|&b| b) && task2_is_advanced.iter().any(|&b| !b);

    let task1 = Task {
        next_t: task.next_t,
        packet_type: task.packet_type,
        path_index: task.path_index,
        ids: task1_ids,
        is_chunked: task1_is_chunked,
        is_advanced: task1_is_advanced,
    };
    let task2 = Task {
        next_t: task.next_t,
        packet_type: task.packet_type,
        path_index: task.path_index,
        ids: task2_ids,
        is_chunked: task2_is_chunked,
        is_advanced: task2_is_advanced,
    };
    Some((task1, task2))
}

fn estimate_core_end_t(state: &State, core_id: usize, input: &Input, graph: &Graph) -> i64 {
    let mut ret = 0;
    if let Some(cur_task) = &state.cur_tasks[core_id] {
        ret += estimate_task_duration(cur_task, input, graph, &state.packet_special_cost)
    }
    for idle_task in &state.idle_tasks[core_id] {
        ret += estimate_task_duration(idle_task, input, graph, &state.packet_special_cost)
    }
    ret
}

/// タスクを終了するのにかかる時間を見積もる
fn estimate_task_duration(
    task: &Task,
    input: &Input,
    graph: &Graph,
    packet_special_cost: &Vec<Option<i64>>,
) -> i64 {
    // TODO: worksを開示したら更新する
    let special_cost_estimate = graph.special_costs.iter().sum::<i64>() / 2;
    let first_packets = if task.is_chunked {
        task.is_advanced.iter().filter(|&&b| !b).count()
    } else {
        task.ids.len()
    };

    let mut ret = 0;

    // core=0から移るswitch costを考慮する
    if task.path_index == 0 {
        ret += first_packets as i64 * input.cost_switch;
    }

    for (i, node_id) in graph.paths[task.packet_type].path[task.path_index..]
        .iter()
        .enumerate()
    {
        let packets_count = if i == 0 {
            first_packets
        } else {
            task.ids.len()
        };
        let max_batch_size = graph.nodes[*node_id].costs.len() - 1;
        let full_chunk_count = packets_count / max_batch_size;
        ret += graph.nodes[*node_id].costs[max_batch_size] * full_chunk_count as i64;

        let remainder = packets_count % max_batch_size;
        ret += graph.nodes[*node_id].costs[remainder];

        if *node_id == SPECIAL_NODE_ID {
            ret += task
                .ids
                .iter()
                .filter(|&&id| {
                    if i == 0 && task.is_chunked {
                        !task.is_advanced[id]
                    } else {
                        true
                    }
                })
                .map(|&id| packet_special_cost[id].unwrap_or(special_cost_estimate))
                .sum::<i64>();
        }
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

    // `packet_type`ごとにタスクを作成して、優先度を計算する
    let mut tasks = create_tasks(&state, t, input, &graph);

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
            q.push((Reverse(task.next_t), Event::ResumeCore(core_id)));
            state.cur_tasks[core_id] = Some(task);
        }
    }

    // TODO: 残っているタスクで、割り込むべきタスクがあればコアのタスクに差し込む

    // 次のパケット受信イベントを登録する
    let next_t = t + input.cost_r * 10; // TODO: 調整
    if next_t <= LAST_PACKET_T {
        q.push((Reverse(next_t), Event::ReceivePacket));
    }
}

/// packet_typeをbatch_sizeで処理する場合の所要時間を見積もる
/// TODO: 前計算
fn estimate_path_duration(
    packet_type: usize,
    batch_size: usize,
    input: &Input,
    graph: &Graph,
) -> i64 {
    // TODO: worksを開示したら更新する
    let special_cost_estimate = graph.special_costs.iter().sum::<i64>() / 2;
    let mut ret = 0;

    // core=0から移るswitch costを考慮する
    ret += batch_size as i64 * input.cost_switch;

    for node_id in &graph.paths[packet_type].path {
        let max_batch_size = graph.nodes[*node_id].costs.len() - 1;
        let full_chunk_count = batch_size / max_batch_size;
        ret += graph.nodes[*node_id].costs[max_batch_size] * full_chunk_count as i64;

        let remainder = batch_size % max_batch_size;
        ret += graph.nodes[*node_id].costs[remainder];

        if *node_id == SPECIAL_NODE_ID {
            // TODO: worksを開示したら更新する
            ret += batch_size as i64 * special_cost_estimate;
        }
    }
    ret
}

/// タスクを作成する
/// 後ろほど優先度が高い
fn create_tasks(state: &State, cur_t: i64, input: &Input, graph: &Graph) -> Vec<Task> {
    let mut packets = vec![vec![]; N_PACKET_TYPE];
    for await_packet in state.await_packets.iter() {
        let packet = state.packets[*await_packet].as_ref().unwrap();
        packets[packet.packet_type].push(packet);
    }

    let mut tasks = vec![];

    let mut push_task =
        |packet_type: usize, cur_ids: &Vec<usize>, min_time_limit: i64, max_received_t: i64| {
            let batch_duration = estimate_path_duration(packet_type, cur_ids.len(), input, graph);
            let next_t = (max_received_t + input.cost_r).max(cur_t);
            let afford = min_time_limit - (next_t + batch_duration);
            tasks.push((afford, max_received_t, packet_type, cur_ids.clone()));
        };

    for packet_type in 0..N_PACKET_TYPE {
        // パケットを締め切り順にソートする
        packets[packet_type].sort_by_key(|&packet| packet.time_limit);

        let mut cur_ids = vec![];
        let mut min_time_limit = i64::MAX;
        let mut max_received_t = i64::MIN;
        for &packet in &packets[packet_type] {
            // TODO: そもそも間に合わないパケットは無視して良い
            // TODO: バッチサイズの上限を設ける
            let new_duration =
                estimate_path_duration(packet.packet_type, cur_ids.len() + 1, input, graph);
            if min_time_limit.min(packet.time_limit)
                < max_received_t.max(packet.received_t) + new_duration
                && cur_ids.len() > 0
            {
                // バッチを分割する
                push_task(packet_type, &cur_ids, min_time_limit, max_received_t);

                cur_ids.clear();
                min_time_limit = i64::MAX;
                max_received_t = i64::MIN;
            }

            min_time_limit = min_time_limit.min(packet.time_limit);
            max_received_t = max_received_t.max(packet.received_t);
            cur_ids.push(packet.i);
        }

        // 残っているidsでタスクを作成する
        if cur_ids.len() > 0 {
            push_task(packet_type, &cur_ids, min_time_limit, max_received_t);
        }
    }

    // 優先度が低い順にソートする（後ろほど優先度が高い、stackとして扱う）
    tasks.sort_by_key(|&(afford, _, _, _)| -afford);
    tasks
        .into_iter()
        .map(|(_, max_received_t, packet_type, ids)| {
            let s = ids.len();
            Task {
                next_t: (max_received_t + input.cost_r).max(cur_t),
                packet_type,
                path_index: 0,
                ids: ids,
                is_chunked: false,
                is_advanced: vec![false; s],
            }
        })
        .collect()
}
