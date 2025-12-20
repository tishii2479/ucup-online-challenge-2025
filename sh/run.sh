set -e

CASE=$1

g++-15 problem/interactor.cpp -o problem/interactor.o
expander > sol.rs
rustc --edition=2024 -O sol.rs

python3 problem/interactive_runner.py ./problem/interactor.o in/$CASE.txt out.txt -- ./sol
