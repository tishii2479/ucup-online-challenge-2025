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
    pub time_limit: i64,
    pub received_t: i64,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
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
