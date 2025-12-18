use crate::libb::*;
use crate::values::*;

use std::{
    io::{BufRead, Write},
    str::FromStr,
};

pub const N_SUBTASK: usize = 5;
pub const N_PACKET: usize = 7;
pub const N_NODE: usize = 20;
pub const N_SPECIAL: usize = 8;
pub const SPECIAL_NODE_ID: usize = 8;
pub const LAST_PACKET_T: i64 = 5_000_000;

#[derive(Debug, Clone)]
pub struct PacketPath {
    /// length of path
    pub l: usize,
    /// computation path of nodes in order
    pub path: Vec<usize>,
}

#[derive(Debug, Clone)]
pub struct Node {
    /// cost[j] := cost to process j packets
    pub costs: Vec<i64>,
}

#[derive(Debug, Clone)]
pub struct Graph {
    pub paths: Vec<PacketPath>,
    pub nodes: Vec<Node>,
    pub special_costs: Vec<i64>,
}

#[derive(Debug, Clone)]
pub struct Packet {
    /// index
    pub i: usize,
    /// arrive time
    pub arrive: i64,
    /// type
    pub packet_type: usize,
    /// timeout
    pub timeout: i64,
}

#[derive(Debug, Clone)]
pub struct PacketHistory {
    pub start_t: i64,
    pub end_t: i64,
    pub core_id: usize,
}

#[derive(Debug, Clone)]
pub struct Input {
    pub n_cores: usize,
    pub cost_switch: i64,
    pub cost_r: i64,
}

pub struct Tester<'a, I: Interactor> {
    pub n: usize,
    pub input: Input,
    pub graph: Graph,
    interactor: &'a mut I,
    packets: Vec<Option<Packet>>,
    packet_works: Vec<Option<[bool; N_SPECIAL]>>,
    packet_history: Vec<Vec<PacketHistory>>,
    core_last_t: Vec<i64>,
    last_t: i64,
}

impl<'a, I: Interactor> Tester<'a, I> {
    pub fn new(interactor: &'a mut I, n: usize, input: Input, graph: Graph) -> Self {
        let num_cores = input.n_cores;
        Self {
            n,
            input,
            graph,
            interactor,
            packets: vec![None; n],
            packet_works: vec![None; n],
            packet_history: vec![vec![]; n],
            core_last_t: vec![1; num_cores + 1], // 0 is used for core 0
            last_t: 0,
        }
    }

    pub fn calc_score(&self) -> i64 {
        let mut max_departure = 0;
        let mut min_arrive = i64::MAX;
        let mut timeout_packets = 0;
        for i in 0..self.n {
            let packet = self.packets[i].as_ref().expect("packet should be received");
            let arrive = packet.arrive;
            min_arrive = min_arrive.min(arrive);

            let path_len = self.graph.paths[packet.i].l;
            let path_history = &self.packet_history[i];
            assert_eq!(
                path_history.len(),
                path_len,
                "packet {} has not finished its path",
                i
            );
            let departure = path_history[path_len - 1].end_t;
            max_departure = max_departure.max(departure);

            let timeout = packet.timeout;
            if departure - arrive > timeout {
                timeout_packets += 1;
            }
        }

        let throughput = ((self.n - 1) as f64 * 1e6)
            / (self.input.n_cores as f64 * (max_departure - min_arrive) as f64);
        let timeout_rate = (timeout_packets as f64) / (self.n as f64);
        let score = (throughput - 1e4 * timeout_rate) * 1e2;
        score.ceil().max(0.) as i64
    }

    pub fn send_receive_packets(&mut self, t: i64) -> Vec<Packet> {
        assert!(
            t >= self.last_t,
            "t should be greater than or equal to {}",
            self.last_t
        );
        self.last_t = t;

        let core0_last_t = self.core_last_t[0];
        assert!(
            t >= core0_last_t,
            "core 0 is busy until {}, but ReceivePacket requested at {}",
            core0_last_t,
            t
        );
        self.core_last_t[0] = t + self.input.cost_r;

        let (_, packets) = self.interactor.send_receive_packet(t);
        for packet in packets.iter().cloned() {
            let i = packet.i;
            self.packets[i] = Some(packet);
        }
        packets
    }

    pub fn send_execute(
        &mut self,
        t: i64,
        core_id: usize,
        node_id: usize,
        s: usize,
        ids: &[usize],
    ) {
        assert!(
            t >= self.last_t,
            "t should be greater than or equal to {}",
            self.last_t
        );
        self.last_t = t;

        assert!(
            t >= self.core_last_t[core_id],
            "core {} is busy until {}",
            core_id,
            self.core_last_t[core_id]
        );

        let mut num_switches = 0;
        for id in ids.iter() {
            let packet = self.packets[*id].as_ref().unwrap();
            let j = self.packet_history[*id].len();
            let last_t = self.packet_history[*id]
                .last()
                .map_or(0, |history| history.end_t);
            assert!(
                t >= last_t,
                "packet {} is busy until {}, but task starts at {}",
                id,
                last_t,
                t
            );

            let next_node = *self.graph.paths[packet.i]
                .path
                .get(j + 1)
                .expect("packet has already finished its path, but still assigned to a task");
            assert_eq!(
                node_id, next_node,
                "packet {} is not ready at node {}",
                id, node_id
            );

            let last_core = self.packet_history[*id]
                .last()
                .map(|history| history.core_id);
            let is_switched_core = match last_core {
                Some(c) => core_id != c,
                None => false,
            };
            if is_switched_core {
                num_switches += 1;
            }
        }

        let mut t_a =
            self.graph.nodes[node_id].costs[s] + (num_switches as i64) * self.input.cost_switch;

        if node_id == SPECIAL_NODE_ID {
            for id in ids.iter() {
                if self.packet_works[*id].is_none() {
                    let works = self.send_query_works(t, *id);
                    if let Some(bits) = works {
                        self.packet_works[*id] = Some(bits);
                    }
                }

                let work_bits = self.packet_works[*id]
                    .as_ref()
                    .expect("packet works should be available after QueryValueOfWork");
                for j in 0..N_SPECIAL {
                    if work_bits[j] {
                        t_a += self.graph.special_costs[j];
                    }
                }
            }
        }

        for id in ids.iter() {
            self.packet_history[*id].push(PacketHistory {
                start_t: t,
                end_t: t + t_a,
                core_id,
            });
        }
        self.core_last_t[core_id] = t + t_a;

        self.interactor.send_execute(t, core_id, node_id, s, ids);
    }

    pub fn send_query_works(&mut self, t: i64, i: usize) -> Option<[bool; N_SPECIAL]> {
        assert!(
            t >= self.last_t,
            "t should be greater than or equal to {}",
            self.last_t
        );
        self.last_t = t;

        if self.packet_works[i].is_some() {
            eprintln!(
                "Warning: QueryValueOfWork requested for packet {} whose works is already known",
                i
            );
        }

        let res = self.interactor.send_query_works(t, i);
        if let Some(bits) = res {
            self.packet_works[i] = Some(bits);
        } else {
            eprintln!("Warning: QueryValueOfWork returned -1");
        }
        res
    }

    pub fn send_finish(&mut self) -> i64 {
        let score = self.calc_score();
        self.interactor.send_finish();
        score
    }
}

pub trait IO {
    fn read_line(&self) -> String;
    fn write_line(&mut self, line: &str);
}

pub struct StdIO {
    stdin: std::io::Stdin,
    stdout: std::io::Stdout,
}

impl StdIO {
    pub fn new() -> Self {
        Self {
            stdin: std::io::stdin(),
            stdout: std::io::stdout(),
        }
    }
}

impl IO for StdIO {
    fn read_line(&self) -> String {
        let mut line = String::new();
        self.stdin
            .lock()
            .read_line(&mut line)
            .expect("Failed to read line");
        line.trim().to_string()
    }

    fn write_line(&mut self, line: &str) {
        writeln!(self.stdout.lock(), "{}", line).expect("Failed to write line");
        self.stdout.lock().flush().expect("Failed to flush stdout");
    }
}

pub trait Interactor {
    fn read_graph(&mut self) -> Graph;
    fn read_input(&mut self) -> Input;
    fn read_n(&mut self) -> usize;
    fn send_receive_packet(&mut self, t: i64) -> (usize, Vec<Packet>);
    fn send_execute(&mut self, t: i64, core_id: usize, node_id: usize, s: usize, ids: &[usize]);
    fn send_query_works(&mut self, t: i64, i: usize) -> Option<[bool; N_SPECIAL]>;
    fn send_finish(&mut self);
}

pub struct IOInteractor<I: IO> {
    io: I,
}

impl<I: IO> IOInteractor<I> {
    pub fn new(io: I) -> Self {
        Self { io }
    }

    fn read_single<T>(&mut self) -> T
    where
        T: FromStr,
        <T as FromStr>::Err: std::fmt::Debug,
    {
        self.io.read_line().parse::<T>().unwrap()
    }
}

impl<I: IO> Interactor for IOInteractor<I> {
    fn read_n(&mut self) -> usize {
        self.read_single::<usize>()
    }

    fn read_graph(&mut self) -> Graph {
        let mut paths = Vec::with_capacity(N_PACKET);
        for _ in 0..N_PACKET {
            let line = self.io.read_line();
            let mut parts = line.split_whitespace();
            let l = parts.next().unwrap().parse::<usize>().unwrap();
            let path = parts
                .take(l)
                .map(|x| x.parse::<usize>().unwrap())
                .collect::<Vec<_>>();
            paths.push(PacketPath { l, path });
        }

        let mut nodes = Vec::with_capacity(N_NODE);
        for _ in 0..N_NODE {
            let line = self.io.read_line();
            let mut parts = line.split_whitespace();
            let b = parts.next().unwrap().parse::<usize>().unwrap();
            let costs = parts
                .take(b)
                .map(|x| x.parse::<i64>().unwrap())
                .collect::<Vec<_>>();
            nodes.push(Node { costs });
        }

        let line = self.io.read_line();
        let special_costs = line
            .split_whitespace()
            .take(N_SPECIAL)
            .map(|x| x.parse::<i64>().unwrap())
            .collect::<Vec<_>>();

        Graph {
            paths,
            nodes,
            special_costs,
        }
    }

    fn read_input(&mut self) -> Input {
        let line = self.io.read_line();
        let mut parts = line.split_whitespace();
        let num_cores = parts.next().unwrap().parse::<usize>().unwrap();
        let cost_switch = parts.next().unwrap().parse::<i64>().unwrap();
        let cost_r = parts.next().unwrap().parse::<i64>().unwrap();
        Input {
            n_cores: num_cores,
            cost_switch,
            cost_r,
        }
    }

    fn send_receive_packet(&mut self, t: i64) -> (usize, Vec<Packet>) {
        self.io.write_line(&format!("R {}", t));

        let p = self.read_single::<i64>();
        assert_ne!(p, -1, "ReceivePacket returned -1");

        let p = p as usize;
        let mut packets = Vec::with_capacity(p);
        for _ in 0..p {
            let line = self.io.read_line();
            let mut parts = line.split_whitespace();
            let i = parts.next().unwrap().parse::<usize>().unwrap();
            let arrive = parts.next().unwrap().parse::<i64>().unwrap();
            let packet_type = parts.next().unwrap().parse::<usize>().unwrap();
            let timeout = parts.next().unwrap().parse::<i64>().unwrap();
            packets.push(Packet {
                i,
                arrive,
                packet_type,
                timeout,
            });
        }
        (p, packets)
    }

    fn send_execute(&mut self, t: i64, core_id: usize, node_id: usize, s: usize, ids: &[usize]) {
        self.io.write_line(&format!(
            "E {} {} {} {} {}",
            t,
            core_id,
            node_id,
            s,
            ids.iter()
                .map(|x| x.to_string())
                .collect::<Vec<_>>()
                .join(" ")
        ));
    }

    fn send_query_works(&mut self, t: i64, i: usize) -> Option<[bool; N_SPECIAL]> {
        self.io.write_line(&format!("Q {} {}", t, i));

        let bitmap = self.read_single::<i64>();
        if bitmap == -1 {
            eprintln!("Warning: QueryValueOfWork returned -1");
            None
        } else {
            let mut bits = [false; N_SPECIAL];
            for j in 0..N_SPECIAL {
                if (bitmap & (1 << j)) != 0 {
                    bits[j] = true;
                }
            }
            Some(bits)
        }
    }

    fn send_finish(&mut self) {
        self.io.write_line("F");
    }
}

pub trait Solver {
    fn solve<I: Interactor>(&self, n: usize, tester: &mut Tester<I>, input: &Input, graph: &Graph);
}

fn generate_mock_problem(rnd: &mut Rnd) -> (usize, Input, Vec<Packet>, Vec<[bool; N_SPECIAL]>) {
    let n = rnd.gen_range(2, 10_001);
    let input = Input {
        cost_r: 20,
        n_cores: rnd.gen_range(1, 33),
        cost_switch: rnd.gen_range(1, 21) as i64,
    };
    let packets = (0..n)
        .map(|i| Packet {
            i,
            arrive: rnd.gen_range(1, LAST_PACKET_T as usize + 1) as i64,
            packet_type: rnd.gen_range(1, N_PACKET + 1) as usize,
            timeout: rnd.gen_range(1, 100_000) as i64,
        })
        .collect::<Vec<_>>();
    let packet_works = (0..n)
        .map(|_| {
            let mut bits = [false; N_SPECIAL];
            for j in 0..N_SPECIAL {
                bits[j] = rnd.gen_index(2) == 0;
            }
            bits
        })
        .collect::<Vec<_>>();
    (n, input, packets, packet_works)
}

pub struct MockInteractor {
    n: usize,
    graph: Graph,
    input: Input,
    packets: Vec<Packet>,
    packet_works: Vec<[bool; N_SPECIAL]>,
    last_core0_t: Option<i64>,
    last_t: i64,
    queried_works: Vec<bool>,
    rnd: Rnd,
}

impl MockInteractor {
    pub fn new(seed: u32) -> Self {
        let mut rnd = Rnd::new(seed as u32);
        let graph = fixed_graph();
        let (n, input, packets, packet_works) = generate_mock_problem(&mut rnd);
        Self {
            n,
            graph,
            input,
            packets,
            packet_works,
            last_core0_t: None,
            last_t: 0,
            queried_works: vec![false; n],
            rnd,
        }
    }
}

impl Interactor for MockInteractor {
    fn read_n(&mut self) -> usize {
        self.n
    }

    fn read_graph(&mut self) -> Graph {
        self.graph.clone()
    }

    fn read_input(&mut self) -> Input {
        self.input.clone()
    }

    fn send_receive_packet(&mut self, t: i64) -> (usize, Vec<Packet>) {
        assert!(
            t >= self.last_t,
            "t should be greater than or equal to {}",
            self.last_t
        );
        self.last_t = t;

        let last_core0_t = self.last_core0_t.unwrap_or(-self.input.cost_r);
        assert!(
            t >= last_core0_t + self.input.cost_r,
            "core 0 is busy until {}, but ReceivePacket requested at {}",
            last_core0_t + self.input.cost_r,
            t
        );
        let packets: Vec<Packet> = self
            .packets
            .iter()
            .filter(|p| last_core0_t < p.arrive && p.arrive <= t)
            .cloned()
            .collect();
        self.last_core0_t = Some(t);
        (packets.len(), packets)
    }

    fn send_execute(
        &mut self,
        t: i64,
        _core_id: usize,
        _node_id: usize,
        _s: usize,
        _ids: &[usize],
    ) {
        assert!(
            t >= self.last_t,
            "t should be greater than or equal to {}",
            self.last_t
        );
        self.last_t = t;
    }

    fn send_query_works(&mut self, t: i64, i: usize) -> Option<[bool; N_SPECIAL]> {
        assert!(
            t >= self.last_t,
            "t should be greater than or equal to {}",
            self.last_t
        );
        self.last_t = t;

        if self.queried_works[i] {
            eprintln!(
                "Warning: QueryValueOfWork requested for packet {} whose works is already known",
                i
            );
            return None;
        }
        self.queried_works[i] = true;
        Some(self.packet_works[i])
    }

    fn send_finish(&mut self) {
        // to next problem
        let (n, input, packets, packet_works) = generate_mock_problem(&mut self.rnd);
        self.n = n;
        self.input = input;
        self.packets = packets;
        self.packet_works = packet_works;
        self.last_core0_t = None;
        self.last_t = 0;
        self.queried_works = vec![false; n];
    }
}

pub struct Runner;

impl Runner {
    pub fn run(&self, solver: impl Solver, mut interactor: impl Interactor) {
        let graph = interactor.read_graph();
        let input = interactor.read_input();
        for _s in 0..N_SUBTASK {
            let n = interactor.read_n();
            let mut tester = Tester::new(&mut interactor, n, input.clone(), graph.clone());
            solver.solve(n, &mut tester, &input, &graph);
        }
    }
}
