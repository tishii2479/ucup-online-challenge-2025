set -e

IN_FILE=$1
echo "IN_FILE: $IN_FILE"
g++-15 problem/interactor.cpp -o problem/interactor.o
expander > sol.rs
rustc --edition=2024 -O sol.rs
python3 problem/interactive_runner.py ./problem/interactor.o $IN_FILE out.txt -- ./sol
