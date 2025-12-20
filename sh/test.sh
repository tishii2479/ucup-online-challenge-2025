set -e

g++-15 problem/interactor.cpp -o problem/interactor.o
expander > sol.rs
rustc --edition=2024 -O sol.rs

for seed in {0..100}; do
    echo "Running test with seed $seed"
    python gen.py --seed $seed --n 200 --n-cores 2 --arrive-term 10 > in/test.txt
    python3 problem/interactive_runner.py ./problem/interactor.o in/test.txt out.txt -- ./sol
done
