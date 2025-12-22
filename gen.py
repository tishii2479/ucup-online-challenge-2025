SUBTASK = 5
COST_R = 20
N_PACKET_TYPE = 7
TIMEOUT_MIN = 1
TIMEOUT_MAX = 100_000
N_SPECIAL = 8


def gen(seed: int, n: int, n_cores: int, arrive_term: int, arrive_start: int) -> None:
    import random

    rnd = random.Random(seed)

    # for fixed graph
    with open("problem/example.in", "r") as f:
        for _ in range(28):
            print(f.readline().rstrip())

    cost_switch = rnd.randrange(1, 21)
    print(n_cores, cost_switch, COST_R)

    for _ in range(SUBTASK):
        print(n)
        data = []
        for i in range(n):
            arrive = arrive_start + rnd.randrange(0, arrive_term)
            packet_type = rnd.randrange(0, N_PACKET_TYPE) + 1
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
    parser.add_argument("--n", type=int, required=True)
    parser.add_argument("--n-cores", type=int, required=True)
    parser.add_argument("--arrive-term", type=int, required=True)
    parser.add_argument("--arrive-start", type=int, default=1_000)

    args = parser.parse_args()
    gen(args.seed, args.n, args.n_cores, args.arrive_term, args.arrive_start)


if __name__ == "__main__":

    main()
