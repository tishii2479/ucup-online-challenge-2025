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

use crate::core::*;

pub fn fixed_graph() -> Graph {
    let mut graph = Graph {
        paths: vec![
            PacketPath {
                l: 9,
                path: vec![1, 2, 3, 7, 6, 11, 12, 13, 14],
            },
            PacketPath {
                l: 14,
                path: vec![1, 3, 8, 9, 3, 8, 9, 3, 9, 6, 11, 12, 13, 14],
            },
            PacketPath {
                l: 18,
                path: vec![1, 2, 3, 7, 6, 9, 3, 8, 7, 6, 16, 18, 19, 20, 3, 4, 6, 10],
            },
            PacketPath {
                l: 13,
                path: vec![1, 3, 7, 6, 9, 3, 8, 9, 3, 7, 6, 16, 17],
            },
            PacketPath {
                l: 25,
                path: vec![
                    1, 2, 3, 8, 7, 6, 15, 20, 3, 8, 7, 6, 15, 20, 3, 8, 7, 6, 15, 20, 3, 7, 6, 16,
                    17,
                ],
            },
            PacketPath {
                l: 19,
                path: vec![
                    1, 3, 4, 5, 6, 16, 18, 19, 20, 3, 8, 9, 6, 9, 6, 11, 12, 13, 14,
                ],
            },
            PacketPath {
                l: 7,
                path: vec![1, 2, 3, 4, 5, 6, 10],
            },
        ],
        nodes: vec![
            Node {
                costs: vec![
                    20, 23, 26, 29, 32, 35, 38, 41, 44, 47, 50, 53, 56, 59, 62, 65, 68, 71, 74, 77,
                    80, 83, 86, 89, 92, 95, 98, 101, 104, 107, 110, 113, 116, 119, 122, 125, 128,
                    131, 134, 137, 140, 143, 146, 149, 152, 155, 158, 161, 164, 167, 170, 173, 176,
                    179, 182, 185, 188, 191, 194, 197, 200, 203, 206, 209, 212, 215, 218, 221, 224,
                    227, 230, 233, 236, 239, 242, 245, 248, 251, 254, 257, 260, 263, 266, 269, 272,
                    275, 278, 281, 284, 287, 290, 293, 296, 299, 302, 305, 308, 311, 314, 317, 320,
                    323, 326, 329, 332, 335, 338, 341, 344, 347, 350, 353, 356, 359, 362, 365, 368,
                    371, 374, 377, 380, 383, 386, 389, 392, 395, 398, 401,
                ],
            },
            Node {
                costs: vec![
                    30, 36, 42, 48, 55, 61, 67, 74, 80, 86, 93, 99, 105, 112, 118, 124, 131, 137,
                    143, 150, 156, 162, 169, 175, 181, 188, 194, 200, 207, 213, 219, 226, 235, 244,
                    253, 263, 272, 281, 290, 300, 309, 318, 328, 337, 346, 355, 365, 374, 383, 393,
                    401, 410, 419, 428, 437, 446, 455, 463, 472, 481, 490, 499, 508, 517,
                ],
            },
            Node {
                costs: vec![
                    17, 27, 37, 48, 58, 69, 79, 90, 100, 111, 121, 132, 143, 153, 164, 174, 185,
                    196, 206, 217, 228, 238, 249, 259, 270, 281, 291, 302, 313, 323, 334, 345, 357,
                    370, 383, 395, 408, 421, 433, 446, 459, 471, 484, 497, 509, 522, 535, 548, 561,
                    575, 589, 602, 616, 630, 643, 657, 671, 684, 698, 712, 725, 739, 753, 767, 780,
                    794, 808, 822, 836, 850, 864, 878, 891, 905, 919, 933, 947, 961, 975, 988,
                    1002, 1016, 1030, 1044, 1058, 1072, 1086, 1099, 1113, 1127, 1141, 1155, 1169,
                    1183, 1197, 1210, 1224, 1238, 1252, 1266, 1280, 1294, 1308, 1322, 1336, 1349,
                    1363, 1377, 1391, 1405, 1419, 1433, 1447, 1461, 1475, 1488, 1502, 1516, 1530,
                    1544, 1558, 1572, 1586, 1600, 1614, 1627, 1641, 1655,
                ],
            },
            Node {
                costs: vec![
                    5, 6, 7, 8, 10, 11, 12, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 26, 27, 28,
                    29, 30, 32, 33, 34, 35, 36, 38, 39, 40, 41, 43, 45, 47, 49, 51, 52, 54, 56, 58,
                    60, 62, 64, 66, 68, 70, 71, 73, 75, 77, 78, 80, 82, 83, 85, 87, 88, 90, 92, 94,
                    95, 97, 99, 100, 102, 104, 106, 108, 109, 111, 113, 115, 116, 118, 120, 122,
                    124, 125, 127, 129, 131, 133, 134, 136, 138, 140, 142, 143, 145, 147, 149, 151,
                    152, 154, 156, 158, 160, 163, 165, 167, 169, 171, 174, 176, 178, 180, 182, 185,
                    187, 189, 191, 194, 196, 198, 200, 202, 205, 207, 209, 211, 214, 216, 218, 220,
                    222, 225, 227,
                ],
            },
            Node {
                costs: vec![
                    13, 20, 27, 34, 41, 48, 55, 62, 67, 76, 83, 90, 98, 104, 111, 118, 125, 132,
                    139, 146, 153, 160, 167, 174, 181, 188, 195, 202, 209, 216, 223, 230, 233, 244,
                    251, 258, 265, 272, 279, 286, 293, 300, 307, 314, 321, 328, 335, 342, 349, 356,
                    363, 370, 377, 384, 391, 398, 405, 412, 419, 426, 430, 440, 447, 454, 461, 468,
                    475, 482, 489, 496, 503, 510, 517, 524, 531, 538, 545, 552, 559, 566, 573, 580,
                    587, 594, 601, 608, 615, 622, 629, 636, 643, 650, 657, 664, 671, 678,
                ],
            },
            Node {
                costs: vec![
                    15, 21, 27, 33, 40, 46, 52, 59, 65, 72, 78, 85, 91, 98, 104, 111, 119, 126,
                    134, 142, 150, 157, 165, 173, 180, 188, 196, 204, 211, 219, 227, 235, 244, 253,
                    262, 271, 280, 289, 298, 307, 316, 325, 334, 344, 353, 362, 371, 380, 392, 403,
                    415, 427, 439, 450, 462, 474, 486, 497, 509, 521, 533, 544, 556, 568, 577, 587,
                    596, 606, 615, 625, 634, 644, 653, 663, 672, 682, 691, 701, 710, 720, 729, 739,
                    748, 758, 767, 777, 786, 796, 805, 815, 824, 834, 843, 853, 862, 872, 884, 897,
                    910, 923, 936, 948, 961, 974, 987, 1000, 1012, 1025, 1038, 1051, 1064, 1076,
                    1089, 1102, 1115, 1128, 1141, 1153, 1166, 1179, 1192, 1205, 1217, 1230, 1243,
                    1256, 1269, 1281,
                ],
            },
            Node {
                costs: vec![
                    25, 38, 52, 65, 79, 92, 106, 119, 132, 146, 159, 172, 185, 199, 212, 225, 240,
                    255, 270, 284, 299, 314, 329, 344, 358, 373, 388, 403, 418, 432, 446, 459, 472,
                    486, 499, 512, 526, 539, 552, 566, 579, 592, 606, 619, 632, 646, 659, 674, 690,
                    705, 720, 735, 751, 766, 781, 796, 812, 827, 842, 858, 873, 888, 903, 917, 932,
                    946, 961, 975, 990, 1004, 1019, 1033, 1048, 1062, 1077, 1091, 1106, 1121, 1135,
                    1150, 1164, 1179, 1193, 1208, 1222, 1237, 1251, 1266, 1280, 1295, 1309, 1324,
                    1338, 1353, 1367, 1382, 1398, 1414, 1430, 1446, 1462, 1478, 1494, 1510, 1526,
                    1542, 1558, 1574, 1590, 1606, 1621, 1637, 1653, 1669, 1685, 1701, 1717, 1733,
                    1749, 1765, 1781, 1797, 1813, 1829, 1845, 1861, 1877, 1893,
                ],
            },
            Node {
                costs: vec![
                    5, 9, 13, 17, 21, 24, 28, 31, 35, 38, 41, 45, 48, 51, 54, 57, 60, 63, 66, 69,
                    72, 75, 77, 80, 83, 86, 88, 91, 94, 96, 99, 102, 104, 107, 109, 112, 115, 117,
                    120, 122, 125, 128, 130, 133, 135, 138, 141, 144, 146, 149, 152, 155, 157, 160,
                    163, 166, 169, 172, 175, 179, 182, 185, 188, 192,
                ],
            },
            Node {
                costs: vec![
                    27, 37, 48, 58, 69, 80, 90, 101, 109, 117, 125, 133, 141, 149, 157, 165, 173,
                    181, 189, 197, 206, 214, 222, 230, 238, 246, 254, 262, 271, 279, 287, 295, 315,
                    336, 356, 376, 396, 417, 437, 457, 478, 498, 518, 538, 559, 579, 599, 620, 609,
                    599, 589, 579, 568, 558, 548, 537, 527, 517, 507, 496, 486, 476, 466, 455, 470,
                    484, 499, 513, 528, 542, 557, 571, 586, 600, 615, 629, 644, 658, 673, 687, 702,
                    716, 731, 745, 760, 774, 788, 803, 817, 832, 846, 861, 875, 890, 904, 919, 930,
                    942, 954, 965, 977, 989, 1000, 1012, 1024, 1035, 1047, 1059, 1070, 1082, 1094,
                    1105, 1117, 1129, 1140, 1152, 1163, 1175, 1187, 1198, 1210, 1222, 1233, 1245,
                    1257, 1268, 1280, 1292,
                ],
            },
            Node {
                costs: vec![
                    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
                    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5,
                    5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 7, 7, 7,
                    7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 8, 8, 8, 8, 8, 8, 8, 8,
                    8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
                ],
            },
            Node {
                costs: vec![
                    7, 12, 18, 24, 30, 36, 42, 48, 54, 60, 66, 72, 78, 84, 90, 96, 102, 108, 114,
                    120, 126, 132, 138, 144, 150, 156, 162, 168, 174, 180, 186, 192, 196, 201, 206,
                    211, 216, 221, 226, 231, 235, 240, 245, 250, 255, 260, 265, 270, 275,
                ],
            },
            Node {
                costs: vec![
                    5, 11, 17, 22, 27, 32, 37, 42, 47, 52, 56, 60, 64, 69, 73, 77, 80, 84, 88, 92,
                    95, 99, 102, 105, 109, 112, 116, 119, 122, 126, 129, 132, 135, 139, 142, 146,
                    149, 152, 156, 160, 163, 167, 171, 174, 178, 182, 187, 191, 195, 200, 204, 209,
                    214, 219, 224, 229, 234, 240, 246, 252, 258, 264, 270, 277,
                ],
            },
            Node {
                costs: vec![
                    4, 6, 8, 11, 13, 15, 18, 20, 22, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35,
                    35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 44, 45, 46, 47, 48, 48, 49, 50, 51, 52,
                    52, 53, 54, 55, 56, 56, 57, 58, 59, 60,
                ],
            },
            Node {
                costs: vec![
                    6, 13, 20, 27, 33, 40, 46, 53, 59, 65, 71, 77, 83, 89, 95, 100, 106, 112, 117,
                    122, 128, 133, 138, 144, 149, 154, 159, 164, 169, 174, 179, 184, 188, 193, 198,
                    203, 207, 212, 216, 221, 225, 230, 234, 239, 243, 248, 252, 257, 261, 266, 270,
                    274, 279, 283, 287, 292, 296, 301, 305, 309, 314, 318, 323, 327, 332, 336, 341,
                    345, 350, 354, 359, 364, 368, 373, 378, 382, 387, 392, 397, 402, 407, 412, 417,
                    422, 428, 433, 438, 444, 449, 455, 460, 466, 471, 477, 483, 489, 495, 501, 507,
                    514, 520, 526, 533, 539, 546, 553, 560, 567, 574, 581, 588, 595, 603, 610, 618,
                    626, 634, 642, 650, 658, 666, 675, 683, 692, 701, 710, 719, 728,
                ],
            },
            Node {
                costs: vec![
                    10, 18, 26, 34, 43, 49, 56, 63, 70, 76, 80, 86, 93, 100, 110, 113,
                ],
            },
            Node {
                costs: vec![
                    7, 15, 23, 31, 38, 46, 53, 60, 68, 75, 82, 89, 96, 102, 109, 116, 122, 129,
                    135, 142, 148, 155, 161, 167, 173, 179, 185, 192, 198, 204, 209, 215, 221, 227,
                    233, 239, 245, 250, 256, 262, 268, 273, 279, 285, 291, 296, 302, 308, 314, 320,
                    325,
                ],
            },
            Node {
                costs: vec![10, 12, 15, 18, 21, 24, 27, 30],
            },
            Node {
                costs: vec![
                    13, 20, 27, 34, 41, 48, 55, 62, 67, 73, 78, 83, 88, 93, 98, 104, 110, 116, 122,
                    128, 134, 141, 147, 153, 159, 165, 171, 177, 184, 190, 196, 202, 208, 214, 223,
                    232, 241, 249, 258, 267, 276, 285, 293, 302, 311, 320, 329, 337, 346, 354, 363,
                    371, 380, 388, 396, 405, 413, 422, 430, 438, 447, 455, 464, 472, 482, 491, 501,
                    510, 520, 529, 539, 548, 558, 568, 577, 587, 596, 606, 615, 625, 635, 644, 654,
                    663, 673, 682, 692, 702, 711, 721, 730, 740, 749, 759, 769, 778, 789, 799, 810,
                    821, 831, 842, 853, 863, 874, 885, 895, 906, 917, 927, 938, 949, 959, 970, 981,
                    991, 1002, 1013, 1023, 1034, 1045, 1055, 1066, 1076, 1087, 1098, 1108, 1119,
                ],
            },
            Node {
                costs: vec![
                    4, 9, 14, 19, 24, 29, 33, 38, 43, 47, 52, 56, 61, 65, 70, 74, 78, 83, 87, 91,
                    95, 99, 104, 108, 112, 116, 120, 124, 128, 132, 135, 139, 143, 147, 151, 154,
                    158, 162, 165, 169, 173, 176, 180, 183, 187, 191, 194, 198, 201, 205, 208, 211,
                    215, 218, 222, 225, 228, 232, 235, 238, 242, 245, 248, 252, 255, 258, 261, 265,
                    268, 271, 274, 278, 281, 284, 287, 291, 294, 297, 300, 304, 307,
                ],
            },
            Node {
                costs: vec![
                    10, 15, 21, 27, 32, 38, 44, 50, 55, 61, 67, 72, 78, 84, 89, 95, 101, 106, 112,
                    117, 123, 128, 134, 139, 145, 150, 156, 161, 167, 172, 178, 183, 190, 196, 202,
                    209, 215, 221, 227, 234, 240, 246, 253, 259, 265, 271, 278, 284, 291, 297, 304,
                    311, 317, 324, 331, 337, 344, 351, 357, 364, 371, 377, 384, 390, 397, 403, 409,
                    416, 422, 428, 435, 441, 447, 454, 460, 466, 473, 479, 485, 492, 498, 504, 511,
                    517, 523, 530, 536, 542, 548, 555, 561, 567, 574, 580, 586, 593, 599, 606, 613,
                    619, 626, 633, 640, 646, 653, 660, 666, 673, 680, 686, 693, 700, 706, 713, 720,
                    726, 733, 740, 747, 753, 760, 767, 773, 780, 787, 793, 800, 807,
                ],
            },
        ],
        special_costs: vec![3, 5, 8, 11, 14, 17, 29, 73],
    };

    // to 0-indexed
    for path in &mut graph.paths {
        for node in &mut path.path {
            *node -= 1;
        }
    }

    for node in &mut graph.nodes {
        node.costs.insert(0, 0); // Insert cost for packet size 0
    }

    graph
}

/// 割り込み用のタスクを挿入する
fn insert_interrupt_tasks(
    cur_t: i64,
    state: &mut State,
    input: &Input,
    graph: &Graph,
    q: &mut EventQueue,
) {
    // 1. コアに挿入できるタスクの最大時間を計算する
    let mut afford_durations = Vec::with_capacity(input.n_cores);
    for core_id in 0..input.n_cores {
        if !state.idle_tasks[core_id].is_empty() {
            continue;
        }
        let complete_t = cur_t + estimate_core_duration(state, core_id, input, graph).upper_bound();
        let Some(cur_task) = state.cur_tasks[core_id].as_ref() else {
            continue;
        };
        let min_time_limit = cur_task
            .packets
            .iter()
            .map(|p| state.packets[p.id].as_ref().unwrap().time_limit)
            .min()
            .unwrap();
        let afford_duration = min_time_limit - complete_t;
        afford_durations.push((afford_duration, core_id));
    }
    afford_durations.sort_by_key(|&(duration, _)| duration);

    // 2. packet_typeごとにs=INSERT_BATCH_SIZEのタスクを作成する
    let min_task_start_t = (0..input.n_cores)
        .map(|core_id| cur_t + estimate_core_duration(state, core_id, input, graph).upper_bound())
        .min()
        .unwrap();
    let mut packets = vec![vec![]; N_PACKET_TYPE];
    for await_packet in state.await_packets.iter() {
        let packet = state.packets[*await_packet].as_ref().unwrap();
        // 1. min_task_start_tから処理を開始したらtimeoutするパケット
        let duration_b1 = estimate_path_duration(packet.packet_type, 1, input, graph);
        let next_t = (packet.received_t + input.cost_r).max(min_task_start_t);
        if next_t + duration_b1.upper_bound() <= packet.time_limit {
            continue;
        }

        // 2. next_tから開始したら間に合うパケット
        let duration_b =
            estimate_path_duration(packet.packet_type, INSERT_BATCH_SIZE, input, graph);
        let next_t = (packet.received_t + input.cost_r).max(cur_t);
        if next_t + duration_b.lower_bound() > packet.time_limit {
            continue;
        }

        packets[packet.packet_type].push(packet);
    }

    let mut batch_cands = vec![];
    for packet_type in 0..N_PACKET_TYPE {
        if packets[packet_type].len() < INSERT_BATCH_SIZE {
            continue;
        }

        let duration_b = estimate_path_duration(packet_type, INSERT_BATCH_SIZE, input, graph);
        packets[packet_type].sort_by_key(|p| p.time_limit);

        let mut batch_ids = vec![];
        let mut timeout_count = 0;
        let mut min_time_limit = INF;
        for &p in packets[packet_type].iter().take(INSERT_BATCH_SIZE) {
            let next_t = (p.received_t + input.cost_r).max(min_task_start_t);
            let is_timeout = next_t + duration_b.upper_bound() > p.time_limit;
            if is_timeout {
                timeout_count += 1;
            }
            min_time_limit = min_time_limit.min(p.time_limit);
            batch_ids.push(p.i);
        }
        if timeout_count == 0 {
            eprintln!(
                "[{:8}] Skip insert interrupt task: packet_type={}, all packets can be processed before timeout",
                cur_t, packet_type
            );
            eprintln!("batch-ids={:?}", batch_ids);
            continue;
        }

        batch_cands.push((packet_type, batch_ids, timeout_count, min_time_limit));
    }

    // 3. timeoutする個数が多い順に、afford_durationが小さい順に挿入を試す
    batch_cands.sort_by_key(|&(_, _, timeout_count, min_time_limit)| {
        (Reverse(timeout_count), min_time_limit)
    });

    for (packet_type, batch_ids, _, min_time_limit) in batch_cands {
        eprintln!("t={:8}", cur_t);
        eprintln!(
            "Attempt insert interrupt task: packet_type={}, batch_ids={:?}",
            packet_type, batch_ids
        );

        let duration_b = estimate_path_duration(packet_type, INSERT_BATCH_SIZE, input, graph);
        for &(max_duration, core_id) in &afford_durations {
            if state.idle_tasks[core_id].len() > 0 {
                continue;
            }

            let core_resume_t = state.cur_tasks[core_id].as_ref().unwrap().next_t;
            let max_received_t = batch_ids
                .iter()
                .map(|&id| state.packets[id].as_ref().unwrap().received_t)
                .max()
                .unwrap();
            let next_t = (max_received_t + input.cost_r).max(core_resume_t);
            if duration_b.upper_bound() > max_duration {
                eprintln!(
                    "  Skip core_id={}: not enough afford_duration (afford={}, need={})",
                    core_id,
                    max_duration,
                    duration_b.upper_bound()
                );
                continue;
            }
            if next_t + duration_b.upper_bound() > min_time_limit {
                eprintln!(
                    "  Skip core_id={}: cannot meet time limit (tl={}, end={})",
                    core_id,
                    min_time_limit,
                    next_t + duration_b.upper_bound()
                );
                continue;
            }

            let task = Task {
                next_t,
                packet_type,
                path_index: 0,
                is_chunked: false,
                packets: batch_ids
                    .iter()
                    .map(|&id| PacketStatus {
                        id,
                        is_advanced: false,
                        is_switching_core: true,
                    })
                    .collect(),
            };

            eprintln!(
                "Insert interrupt task: core_id={}, packet_type={}, batch_ids={:?}",
                core_id, packet_type, batch_ids
            );

            // 挿入する
            // await_packetsから削除する
            for &p in &task.packets {
                state.await_packets.remove(p.id);
            }
            let cur_task = state.cur_tasks[core_id].take().unwrap();

            state.idle_tasks[core_id].push(cur_task);
            state.cur_tasks[core_id] = Some(task);
            q.push((Reverse(next_t), Event::ResumeCore(core_id)));

            break;
        }
    }
}

fn should_chunk_special_node_task(
    cur_t: i64,
    node_id: usize,
    cur_task: &Task,
    state: &State,
    core_id: usize,
    input: &Input,
    graph: &Graph,
) -> bool {
    if state.await_packets.size() > 0 {
        return false;
    }

    let dt = if node_id == SPECIAL_NODE_ID {
        graph.nodes[node_id].costs[cur_task.packets.len()]
            + cur_task
                .packets
                .iter()
                .map(|p| state.packet_special_cost[p.id].unwrap())
                .sum::<i64>()
    } else {
        graph.nodes[node_id].costs[cur_task.packets.len()]
    };

    for other_core_id in 0..input.n_cores {
        if other_core_id == core_id {
            continue;
        }
        let Some(other_task) = &state.next_tasks[other_core_id] else {
            continue;
        };
        if cur_t <= other_task.next_t && other_task.next_t < cur_t + dt {
            return true;
        }
    }
    false
}
