set -e

g++-15 problem/interactor.cpp -o problem/interactor.o
expander > sol.rs
rustc --edition=2024 -O sol.rs

for seed in {0..100}; do
    echo "Running test with seed $seed"
    # python gen.py --seed $seed --n 10000 --n-cores 32 --arrive-term 10000 --arrive-start 4900000 > in/test.txt
    python gen.py --seed $seed --n 2000 --n-cores 2 --arrive-term 2000 --arrive-start 1000 > in/test.txt
    time python3 problem/interactive_runner.py ./problem/interactor.o in/test.txt out.txt -- ./sol
done
