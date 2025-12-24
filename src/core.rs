use crate::interactor::Interactor;

#[allow(dead_code)]
pub const TIME_LIMIT: f64 = 9.8;

pub const N_SUBTASK: usize = 5;
pub const N_PACKET_TYPE: usize = 7;
pub const N_NODE: usize = 20;
pub const N_SPECIAL: usize = 8;
pub const SPECIAL_NODE_ID: usize = 8 - 1; // 0-indexed
pub const MAX_T: i64 = 10_000_000;
pub const LAST_PACKET_T: i64 = 5_000_000;
pub const SPECIAL_COST_SUM: i64 = 3 + 5 + 8 + 11 + 14 + 17 + 29 + 73;
pub const CHUNK_NODES: [usize; 4] = [11, 13, 15, 18];

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

#[derive(Copy, Clone, Debug)]
pub struct PacketStatus {
    pub id: usize,
    pub is_advanced: bool,
    pub is_switching_core: bool,
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
    pub node_id: usize,
    pub batch_size: usize,
    pub packet_type: usize,
    pub path_index: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct Duration {
    pub fixed: i64,
    pub special_node_count: i64,
}

impl Duration {
    pub fn new(fixed: i64, special_node_count: i64) -> Self {
        Self {
            fixed,
            special_node_count,
        }
    }

    pub fn add(&mut self, other: &Duration) {
        self.fixed += other.fixed;
        self.special_node_count += other.special_node_count;
    }

    #[inline]
    pub fn lower_bound(&self) -> i64 {
        self.fixed
    }

    #[inline]
    pub fn upper_bound(&self) -> i64 {
        self.fixed + self.special_node_count * SPECIAL_COST_SUM
    }
}

pub trait Solver {
    fn solve<I: Interactor>(&self, n: usize, interactor: &mut I, input: &Input, graph: &Graph);
}

pub fn calc_timeout_score_rate(score: &Score) -> f64 {
    let score1 = score.to_score();
    let no_timeout_score = Score {
        throughput: score.throughput,
        timeout_rate: 0.0,
    };
    let score2 = no_timeout_score.to_score();
    let score_rate = (score2 - score1) / score2;
    score_rate
}
