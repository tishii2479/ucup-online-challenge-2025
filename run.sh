set -e

g++-15 problem/interactor.cpp -o problem/interactor.o
expander > sol.rs
rustc --edition=2024 -O sol.rs

echo "n=1000, n_cores=8, arrive_terms=1000"
echo "best: 389782"
python3 problem/interactive_runner.py ./problem/interactor.o in/0000.txt out.txt -- ./sol

echo "n=1000, n_cores=4, arrive_terms=1000"
echo "best: 237287"
python3 problem/interactive_runner.py ./problem/interactor.o in/0001.txt out.txt -- ./sol

echo "n=1000, n_cores=16, arrive_terms=1000"
echo "best: 387497"
python3 problem/interactive_runner.py ./problem/interactor.o in/0002.txt out.txt -- ./sol

echo "n=500, n_cores=8, arrive_terms=2000"
echo "best: 348173"
python3 problem/interactive_runner.py ./problem/interactor.o in/0003.txt out.txt -- ./sol

echo "n=200, n_cores=1, arrive_terms=1000"
echo "best: 265482"
python3 problem/interactive_runner.py ./problem/interactor.o in/0004.txt out.txt -- ./sol

