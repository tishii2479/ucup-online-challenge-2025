import argparse
import re
import subprocess

import pandas as pd

CASES = [
    {"seed": 0, "n": 200, "n_cores": 1, "arrive_terms": 10},
    {"seed": 1, "n": 200, "n_cores": 2, "arrive_terms": 10},
    {"seed": 2, "n": 1000, "n_cores": 4, "arrive_terms": 1000},
    {"seed": 3, "n": 1000, "n_cores": 8, "arrive_terms": 1000},
    {"seed": 4, "n": 1000, "n_cores": 16, "arrive_terms": 1000},
    {"seed": 5, "n": 1000, "n_cores": 32, "arrive_terms": 1000},
    {"seed": 6, "n": 200, "n_cores": 4, "arrive_terms": 1000},
    {"seed": 7, "n": 2000, "n_cores": 4, "arrive_terms": 5000},
]


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("-t", "--tag", type=str, default="current")
    args = parser.parse_args()

    subprocess.run(
        [
            "g++-15",
            "problem/interactor.cpp",
            "-o",
            "problem/interactor.o",
        ]
    )
    subprocess.run("expander > sol.rs", shell=True)
    subprocess.run(["rustc", "--edition=2024", "-O", "sol.rs", "-o", "sol"])

    scores = []

    df = pd.read_csv("doc/scores.csv")

    for case in CASES:
        in_file = f"{str(case['seed']).zfill(4)}.txt"
        print(
            f"\n--- Running case: n={case['n']}, n_cores={case['n_cores']}, "
            f"arrive_terms={case['arrive_terms']} ({in_file}) ---"
        )
        subprocess.run(
            [
                "python3",
                "gen.py",
                "--seed",
                str(case["seed"]),
                "--n",
                str(case["n"]),
                "--n-cores",
                str(case["n_cores"]),
                "--arrive-term",
                str(case["arrive_terms"]),
            ],
            stdout=open(f"in/{in_file}", "w"),
        )

        result = subprocess.run(
            [
                "python3",
                "problem/interactive_runner.py",
                "./problem/interactor.o",
                f"in/{in_file}",
                "out.txt",
                "--",
                "./sol",
            ],
            capture_output=True,
            text=True,
        )

        cur_max_score = df[in_file].max()
        score = 0
        if result.stderr:
            # `judge: points {score}` のような出力を探す
            match = re.search(r"judge: points (\d+)", result.stderr)
            if match:
                score = int(match.group(1))
                scores.append(score)
                print(f"Score: {score}: best {cur_max_score}")
            else:
                scores.append(0)
                print("Score not found in judge output.")
        else:
            scores.append(0)
            print("No stderr from judge.")

        if result.returncode != 0:
            print(f"Runner exited with code {result.returncode}")
            print("--- stdout ---")
            print(result.stdout)
            print("--- stderr ---")
            print(result.stderr)

    new_df = pd.DataFrame(
        [[args.tag, "-"] + scores],
        columns=df.columns,
    )
    df = pd.concat([df, new_df], ignore_index=True)

    if args.tag != "current":
        print("Updating doc/scores.csv")
        df.to_csv("doc/scores.csv", index=False)

    new_df = pd.DataFrame(
        [["best", "-"] + df.iloc[:, 2:].max().tolist()],
        columns=df.columns,
    )
    df = pd.concat([df, new_df], ignore_index=True)
    df["total"] = df.iloc[:, 2:].sum(axis=1)

    print(df)


if __name__ == "__main__":
    main()
