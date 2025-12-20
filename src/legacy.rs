pub struct Tester<'a, I: Interactor> {
    pub n: usize,
    pub input: Input,
    pub graph: Graph,
    interactor: &'a mut I,
    packets: Vec<Option<Packet>>,
    packet_works: Vec<Option<[bool; N_SPECIAL]>>,
    packet_history: Vec<Vec<PacketHistory>>,
    core_last_t: Vec<i64>,
    core0_last_t: i64,
    last_t: i64,
    pub task_logs: Vec<TaskLog>,
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
            core_last_t: vec![1; num_cores],
            core0_last_t: 0,
            last_t: 0,
            task_logs: Vec::with_capacity(n * 32),
        }
    }

    pub fn calc_score(&self) -> Score {
        let mut max_departure = 0;
        let mut min_arrive = i64::MAX;
        let mut timeout_packets = 0;
        for i in 0..self.n {
            let packet = self.packets[i].as_ref().expect("packet should be received");
            let arrive = packet.arrive;
            min_arrive = min_arrive.min(arrive);

            let path_len = self.graph.paths[packet.packet_type].l;
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
        Score {
            throughput,
            timeout_rate,
        }
    }

    pub fn send_receive_packets(&mut self, t: i64) -> Vec<Packet> {
        assert!(
            t >= self.last_t,
            "t should be greater than or equal to {}",
            self.last_t
        );
        self.last_t = t;

        assert!(
            t >= self.core0_last_t,
            "core 0 is busy until {}, but ReceivePacket requested at {}",
            self.core0_last_t,
            t
        );
        self.core0_last_t = t + self.input.cost_r;

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
            let next_path_index = self.packet_history[*id].len();
            let start_t = packet.received_t + self.input.cost_r; // 受信にかかった時間を考慮
            let last_t = self.packet_history[*id]
                .last()
                .map_or(start_t, |history| history.end_t);
            assert!(
                t >= last_t,
                "packet {} is busy until {}, but task starts at {}, history: {:?}",
                id,
                last_t,
                t,
                self.packet_history[*id]
            );

            let next_node = *self.graph.paths[packet.packet_type]
                .path
                .get(next_path_index)
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
                None => true, // 最初はcore=0に受け取っているので、switchとみなす
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

        self.task_logs.push(TaskLog {
            core_id,
            start_t: t,
            end_t: t + t_a,
            batch_size: ids.len(),
            packet_type: self.packets[ids[0]]
                .as_ref()
                .expect("packet should be received")
                .packet_type,
            path_index: self.packet_history[ids[0]].len() - 1,
        });

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

    pub fn send_finish(&mut self) -> Score {
        let score = self.calc_score();
        self.interactor.send_finish();
        score
    }
}

#[derive(Debug, Clone)]
pub struct ProblemParams {
    pub n: usize,
    pub n_cores: usize,
    pub arrive_term: i64,
}

fn generate_mock_problem(
    rnd: &mut Rnd,
    p: &ProblemParams,
) -> (usize, Input, Vec<Packet>, Vec<[bool; N_SPECIAL]>) {
    // let n = rnd.gen_range(2, 10_001);
    let n = p.n;
    let input = Input {
        cost_r: 20,
        // n_cores: rnd.gen_range(1, 33),
        n_cores: p.n_cores,
        cost_switch: rnd.gen_range(1, 21) as i64,
    };
    const ARRIVE_START: usize = 1_000_000;
    let packets = (0..n)
        .map(|i| Packet {
            i,
            // arrive: rnd.gen_range(1, LAST_PACKET_T + 1) as i64,
            arrive: rnd.gen_range(ARRIVE_START, ARRIVE_START + p.arrive_term as usize + 1) as i64,
            packet_type: rnd.gen_range(0, N_PACKET_TYPE),
            timeout: rnd.gen_range(1, 100_000) as i64,
            received_t: -1,
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
    p: ProblemParams,
}

impl MockInteractor {
    pub fn new(seed: u32, p: ProblemParams) -> Self {
        let mut rnd = Rnd::new(seed as u32);
        let graph = fixed_graph();
        let (n, input, packets, packet_works) = generate_mock_problem(&mut rnd, &p);
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
            p,
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
        let mut packets: Vec<Packet> = self
            .packets
            .iter()
            .filter(|p| last_core0_t < p.arrive && p.arrive <= t)
            .cloned()
            .collect();
        // received_tを更新する
        for p in &mut packets {
            p.received_t = t;
        }
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
        let (n, input, packets, packet_works) = generate_mock_problem(&mut self.rnd, &self.p);
        self.n = n;
        self.input = input;
        self.packets = packets;
        self.packet_works = packet_works;
        self.last_core0_t = None;
        self.last_t = 0;
        self.queried_works = vec![false; n];
    }
}
