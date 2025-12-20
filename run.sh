set -e

g++-15 problem/interactor.cpp -o problem/interactor.o
expander > sol.rs
rustc --edition=2024 -O sol.rs

echo "n=1000, n_cores=8, arrive_terms=1000"
echo "best: 296265"
python3 problem/interactive_runner.py ./problem/interactor.o in/0000.txt out.txt -- ./sol
echo "n=1000, n_cores=4, arrive_terms=1000"
echo "best: 204488"
python3 problem/interactive_runner.py ./problem/interactor.o in/0001.txt out.txt -- ./sol
echo "n=1000, n_cores=16, arrive_terms=1000"
echo "best: 209787"
python3 problem/interactive_runner.py ./problem/interactor.o in/0002.txt out.txt -- ./sol
echo "n=500, n_cores=8, arrive_terms=2000"
echo "best: 287241"
python3 problem/interactive_runner.py ./problem/interactor.o in/0003.txt out.txt -- ./sol

