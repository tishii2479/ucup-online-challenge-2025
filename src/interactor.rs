use crate::core::*;
use std::str::FromStr;

pub trait Interactor {
    fn read_graph(&mut self) -> Graph;
    fn read_input(&mut self) -> Input;
    fn read_n(&mut self) -> usize;
    fn send_receive_packets(&mut self, t: i64) -> (usize, Vec<Packet>);
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
                received_t: t,
            });
        }
        (p, packets)
    }

    fn send_execute(&mut self, t: i64, core_id: usize, node_id: usize, s: usize, ids: &[usize]) {
        self.io.write_line(&format!(
            "E {} {} {} {} {}",
            t,
            core_id + 1,
            node_id + 1,
            s,
            ids.iter()
                .map(|x| (x + 1).to_string())
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
