mod core;
mod libb;
mod values;

use std::collections::BinaryHeap;

use crate::core::*;

fn main() {
    // let interactor = IOInteractor::new(StdIO::new());
    let interactor = MockInteractor::new(12345);
    let solver = GreedySolver;
    let runner = Runner;
    runner.run(solver, interactor);
}

#[derive(Clone, Debug)]
struct Execution {
    end_t: i64,
    /// (packet_id, path_index)
    packet: Vec<(usize, usize)>,
}

#[derive(Clone, Debug)]
pub struct State {
    packets: Vec<Option<Packet>>,
    /// cur_execs[core_id][node_id]
    cur_execs: Vec<Vec<Option<Execution>>>,
    packet_works: Vec<Option<[bool; N_SPECIAL]>>,
    /// await_packets[core_id][node_id]
    await_packets: Vec<Vec<usize>>,
}

impl State {
    pub fn new(n: usize, input: &Input) -> Self {
        Self {
            packets: vec![None; n],
            cur_execs: vec![vec![None; N_NODE]; input.n_cores],
            packet_works: vec![None; n],
            await_packets: vec![vec![]; input.n_cores],
        }
    }
}

#[derive(Clone, Debug, Copy, PartialEq, PartialOrd, Eq, Ord)]
enum Event {
    ReceivePacket,
    FinishExecution(usize, usize), // (core_id, node_id)
}

pub struct GreedySolver;

impl Solver for GreedySolver {
    fn solve<I: Interactor>(
        &self,
        n: usize,
        tester: &mut Tester<I>,
        input: &Input,
        _graph: &Graph,
    ) {
        let mut q: BinaryHeap<(i64, Event)> = BinaryHeap::new();
        q.push((0, Event::ReceivePacket));

        let mut state = State::new(n, input);
        while let Some((t, event)) = q.pop() {
            match event {
                Event::ReceivePacket => {
                    let packets = tester.send_receive_packets(t);
                    for packet in packets {
                        let i = packet.i;
                        state.packets[i] = Some(packet);
                    }

                    let next_t = t + input.cost_r;
                    if next_t <= LAST_PACKET_T {
                        q.push((next_t, Event::ReceivePacket));
                    }
                }
                Event::FinishExecution(_core_id, _node_id) => {
                    // a
                }
            }
        }

        let score = tester.send_finish();
        eprintln!("Score: {}", score);

        // todo!()
    }
}
