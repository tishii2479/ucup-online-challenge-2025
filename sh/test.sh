set -e

g++-15 problem/interactor.cpp -o problem/interactor.o
expander > sol.rs
rustc --edition=2024 -O sol.rs

for seed in {607..607}; do
    echo "Running test with seed $seed"
    # python gen.py --seed $seed --n 10000 --n-cores 32 --arrive-term 10000 --arrive-start 4900000 > in/test.txt
    # python gen.py --seed $seed --n 200 --n-cores 32 --arrive-term 200 --arrive-start 3000000 > in/test.txt
    # python gen.py --seed $seed --n 10000 --n-cores 32 --arrive-term 10000 --arrive-start 1000 > in/test.txt
    # python gen.py --seed $seed --n 10000 --n-cores 32 --arrive-term 10000 --arrive-start 1000 --packet-types 1 > in/test.txt
    # python3 problem/interactive_runner.py ./problem/interactor.o in/test.txt out.txt -- ./sol

    # python gen.py --seed $seed --n 10000 --n-cores 32 --arrive-term 90000 --arrive-start 4900000 --packet-types 1 > in/test.txt
    python gen.py --seed $seed > in/test.txt
    time python3 problem/interactive_runner.py ./problem/interactor.o in/test.txt out.txt -- ./sol
done
