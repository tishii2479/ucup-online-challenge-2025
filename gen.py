import sys

SUBTASK = 5
COST_R = 20
N_PACKET_TYPE = 7
TIMEOUT_MIN = 1
TIMEOUT_MAX = 100_000
N_SPECIAL = 8


def gen(
    seed: int,
    n: int | None,
    n_cores: int | None,
    arrive_term: int | None,
    arrive_start: int | None,
    packet_types: int | None,
) -> None:
    import random

    rnd = random.Random(seed)

    if n is None:
        n = rnd.randrange(1, 10001)
    if n_cores is None:
        n_cores = rnd.randrange(1, 33)
    if arrive_term is None:
        arrive_term = n * rnd.randrange(1, 10)
    if arrive_start is None:
        arrive_start = rnd.randrange(1, 4_000_000)
    if packet_types is None:
        packet_types = rnd.randrange(1, N_PACKET_TYPE + 1)

    # stderr
    print(
        f"{seed=} {n=} {n_cores=} {arrive_term=} {arrive_start=} {packet_types=}",
        file=sys.stderr,
    )

    # for fixed graph
    with open("problem/example.in", "r") as f:
        for _ in range(28):
            print(f.readline().rstrip())

    cost_switch = rnd.randrange(1, 21)
    print(n_cores, cost_switch, COST_R)

    if packet_types == N_PACKET_TYPE:
        p_types = list(range(1, N_PACKET_TYPE + 1))
    else:
        p_types = rnd.sample(range(1, N_PACKET_TYPE + 1), k=packet_types)

    for _ in range(SUBTASK):
        print(n)
        data = []
        for i in range(n):
            arrive = arrive_start + rnd.randrange(0, arrive_term)
            packet_type = rnd.choice(p_types)
            timeout = rnd.randint(TIMEOUT_MIN, TIMEOUT_MAX)
            works = rnd.randrange(0, 1 << N_SPECIAL)
            data.append((arrive, packet_type, timeout, works))
        data.sort()

        for i, (arrive, packet_type, timeout, works) in enumerate(data):
            print(i, arrive, packet_type, timeout, works)


def main() -> None:
    import argparse

    parser = argparse.ArgumentParser()
    parser.add_argument("--seed", type=int, required=False, default=0)
    parser.add_argument("--n", type=int, required=False, default=None)
    parser.add_argument("--n-cores", type=int, required=False, default=None)
    parser.add_argument("--arrive-term", type=int, required=False, default=None)
    parser.add_argument("--arrive-start", type=int, required=False, default=None)
    parser.add_argument("--packet-types", type=int, required=False, default=None)

    args = parser.parse_args()
    gen(
        args.seed,
        args.n,
        args.n_cores,
        args.arrive_term,
        args.arrive_start,
        args.packet_types,
    )


if __name__ == "__main__":

    main()
