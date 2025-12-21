use crate::core::*;
use std::str::FromStr;

pub trait Interactor {
    fn read_graph(&mut self) -> Graph;
    fn read_input(&mut self) -> Input;
    fn read_n(&mut self) -> usize;
    fn send_receive_packets(&mut self, t: i64) -> (usize, Vec<Packet>);
    fn send_execute(
        &mut self,
        t: i64,
        core_id: usize,
        node_id: usize,
        s: usize,
        packets: &[PacketStatus],
    );
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
        let mut paths = Vec::with_capacity(N_PACKET_TYPE);
        for _ in 0..N_PACKET_TYPE {
            let line = self.io.read_line();
            let mut parts = line.split_whitespace();
            let l = parts.next().unwrap().parse::<usize>().unwrap();
            let path = parts
                .take(l)
                .map(|x| x.parse::<usize>().unwrap() - 1) // 0-indexed
                .collect::<Vec<_>>();
            paths.push(PacketPath { l, path });
        }

        let mut nodes = Vec::with_capacity(N_NODE);
        for _ in 0..N_NODE {
            let line = self.io.read_line();
            let mut parts = line.split_whitespace();
            let b = parts.next().unwrap().parse::<usize>().unwrap();
            let mut costs = parts
                .take(b)
                .map(|x| x.parse::<i64>().unwrap())
                .collect::<Vec<_>>();
            costs.insert(0, 0); // add cost for 0 packets
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

    fn send_receive_packets(&mut self, t: i64) -> (usize, Vec<Packet>) {
        self.io.write_line(&format!("R {}", t));

        let p = self.read_single::<i64>();
        assert_ne!(p, -1, "ReceivePacket returned -1");

        let p = p as usize;
        let mut packets = Vec::with_capacity(p);
        for _ in 0..p {
            let line = self.io.read_line();
            let mut parts = line.split_whitespace();
            let i = parts.next().unwrap().parse::<usize>().unwrap() - 1;
            let arrive = parts.next().unwrap().parse::<i64>().unwrap();
            let packet_type = parts.next().unwrap().parse::<usize>().unwrap() - 1;
            let timeout = parts.next().unwrap().parse::<i64>().unwrap();
            packets.push(Packet {
                i,
                arrive,
                packet_type,
                timeout,
                time_limit: arrive + timeout,
                received_t: t,
            });
        }
        (p, packets)
    }

    fn send_execute(
        &mut self,
        t: i64,
        core_id: usize,
        node_id: usize,
        s: usize,
        packets: &[PacketStatus],
    ) {
        self.io.write_line(&format!(
            "E {} {} {} {} {}",
            t,
            core_id + 1,
            node_id + 1,
            s,
            packets
                .iter()
                .map(|p| (p.id + 1).to_string())
                .collect::<Vec<_>>()
                .join(" ")
        ));
    }

    fn send_query_works(&mut self, t: i64, i: usize) -> Option<[bool; N_SPECIAL]> {
        self.io.write_line(&format!("Q {} {}", t, i + 1));

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
            // eprintln!("Tracker is disabled. Score can not be calculated.");
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

    #[allow(dead_code)]
    pub fn dump_score(
        &self,
        n: usize,
        packets: &Vec<Option<Packet>>,
        graph: &Graph,
        input: &Input,
    ) {
        if !self.enabled {
            // eprintln!("Tracker is disabled. Score can not be calculated.");
            return;
        }

        let score = self.calc_score(n, &packets, graph, input);
        eprintln!(
            "score: {:10.2} (throughput: {:8.2}, timeout_rate: {:6.5})",
            score.to_score(),
            score.throughput,
            score.timeout_rate
        );
    }

    #[allow(dead_code)]
    pub fn dump_task_logs(&self) {
        if !self.enabled {
            // eprintln!("Tracker is disabled. Task logs can not be dumped.");
            return;
        }

        use std::io::Write;
        let mut file = std::fs::File::create("log.json").unwrap();
        let mut json = String::new();
        json.push('[');
        for (i, log) in self.task_logs.iter().enumerate() {
            if i > 0 {
                json.push(',');
            }
            json.push_str(&format!(
            r#"{{"core_id":{},"start_t":{},"end_t":{},"batch_size":{},"packet_type":{},"path_index":{},"node_id":{}}}"#,
            log.core_id, log.start_t, log.end_t, log.batch_size, log.packet_type, log.path_index, log.node_id
        ));
        }
        json.push(']');
        file.write_all(json.as_bytes()).unwrap();
    }
}
