import argparse
import re
import subprocess

import pandas as pd

CASES = [
    {"name": "n=1000, n_cores=8, arrive_terms=1000", "file": "in/0000.txt"},
    {"name": "n=1000, n_cores=4, arrive_terms=1000", "file": "in/0001.txt"},
    {"name": "n=1000, n_cores=16, arrive_terms=1000", "file": "in/0002.txt"},
    {"name": "n=500, n_cores=8, arrive_terms=2000", "file": "in/0003.txt"},
    {"name": "n=200, n_cores=1, arrive_terms=1000", "file": "in/0004.txt"},
    {"name": "seed=2, n=200, n_cores=2, arrive_terms=10", "file": "in/0005.txt"},
    {"name": "seed=13, n=200, n_cores=2, arrive_terms=10", "file": "in/0006.txt"},
]


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--version", type=str, default="current")
    parser.add_argument("--add-to-csv", action="store_true")
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
        in_file = case["file"]
        print(f"\n--- Running case: {case['name']} ({in_file}) ---")
        result = subprocess.run(
            [
                "python3",
                "problem/interactive_runner.py",
                "./problem/interactor.o",
                in_file,
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
        [[args.version, "-"] + scores],
        columns=df.columns,
    )
    df = pd.concat([df, new_df], ignore_index=True)
    print(df)

    if args.add_to_csv:
        print("Updating doc/scores.csv")
        df.to_csv("doc/scores.csv", index=False)


if __name__ == "__main__":
    main()
