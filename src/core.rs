use std::io::BufRead;
use std::io::Write;

use crate::interactor::Interactor;

pub const N_SUBTASK: usize = 5;
pub const N_PACKET_TYPE: usize = 7;
pub const N_NODE: usize = 20;
pub const N_SPECIAL: usize = 8;
pub const SPECIAL_NODE_ID: usize = 8 - 1; // 0-indexed
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
    pub i: usize,
    pub arrive: i64,
    pub packet_type: usize,
    pub timeout: i64,
    pub received_t: i64,
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

#[derive(Debug, Clone)]
pub struct Score {
    pub throughput: f64,
    pub timeout_rate: f64,
}

impl Score {
    pub fn to_score(&self) -> f64 {
        (self.throughput - 1e4 * self.timeout_rate) * 1e2
    }
}

#[derive(Debug, Clone)]
pub struct TaskLog {
    pub core_id: usize,
    pub start_t: i64,
    pub end_t: i64,
    pub batch_size: usize,
    pub packet_type: usize,
    pub path_index: usize,
}

pub trait IO {
    fn read_line(&self) -> String;
    fn write_line(&mut self, line: &str);
}

pub struct StdIO {
    stdin: std::io::Stdin,
    stdout: std::io::Stdout,
    to_stderr: bool,
}

impl StdIO {
    pub fn new(to_stderr: bool) -> Self {
        Self {
            stdin: std::io::stdin(),
            stdout: std::io::stdout(),
            to_stderr,
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
        if self.to_stderr {
            eprintln!("Sent: {}", line);
        }
        self.stdout.lock().flush().expect("Failed to flush stdout");
    }
}

pub trait Solver {
    fn solve<I: Interactor>(&self, n: usize, interactor: &mut I, input: &Input, graph: &Graph);
}

#[derive(Debug, Clone)]
pub struct Tracker {
    pub enabled: bool,
    pub packet_history: Vec<Vec<PacketHistory>>,
    pub task_logs: Vec<TaskLog>,
}

impl Tracker {
    pub fn new(n: usize, enabled: bool) -> Self {
        let n = if enabled { n } else { 1 };
        Tracker {
            enabled,
            packet_history: vec![Vec::with_capacity(32); n],
            task_logs: Vec::with_capacity(32 * n),
        }
    }

    pub fn add_packet_history(&mut self, packet_id: usize, history: PacketHistory) {
        if self.enabled {
            self.packet_history[packet_id].push(history);
        }
    }

    pub fn add_task_log(&mut self, log: TaskLog) {
        if self.enabled {
            self.task_logs.push(log);
        }
    }

    pub fn calc_score(
        &self,
        n: usize,
        packets: &Vec<Option<Packet>>,
        graph: &Graph,
        input: &Input,
    ) -> Score {
        if !self.enabled {
            eprintln!("Tracker is disabled. Score can not be calculated.");
            return Score {
                throughput: 0.0,
                timeout_rate: 0.0,
            };
        }

        let mut max_departure = 0;
        let mut min_arrive = i64::MAX;
        let mut timeout_packets = 0;
        for i in 0..n {
            let packet = packets[i].as_ref().expect("packet should be received");
            let arrive = packet.arrive;
            min_arrive = min_arrive.min(arrive);

            let path_len = graph.paths[packet.packet_type].l;
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

        let throughput =
            ((n - 1) as f64 * 1e6) / (input.n_cores as f64 * (max_departure - min_arrive) as f64);
        let timeout_rate = (timeout_packets as f64) / (n as f64);
        Score {
            throughput,
            timeout_rate,
        }
    }
}
