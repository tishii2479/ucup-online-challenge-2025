set -e

g++-15 problem/interactor.cpp -o problem/interactor.o
expander > sol.rs
rustc --edition=2024 -O sol.rs

python3 problem/interactive_runner.py ./problem/interactor.o in/0002.txt out.txt -- ./sol
