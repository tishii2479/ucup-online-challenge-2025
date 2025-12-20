set -e

g++-15 problem/interactor.cpp -o problem/interactor.o
expander > sol.rs
rustc --edition=2024 -O sol.rs

echo "n=1000, n_cores=8, arrive_terms=1000"
python3 problem/interactive_runner.py ./problem/interactor.o in/0000.txt out.txt -- ./sol

echo "n=1000, n_cores=4, arrive_terms=1000"
python3 problem/interactive_runner.py ./problem/interactor.o in/0001.txt out.txt -- ./sol

echo "n=1000, n_cores=16, arrive_terms=1000"
python3 problem/interactive_runner.py ./problem/interactor.o in/0002.txt out.txt -- ./sol

echo "n=500, n_cores=8, arrive_terms=2000"
python3 problem/interactive_runner.py ./problem/interactor.o in/0003.txt out.txt -- ./sol

echo "n=200, n_cores=1, arrive_terms=1000"
python3 problem/interactive_runner.py ./problem/interactor.o in/0004.txt out.txt -- ./sol

echo "seed=2, n=200, n_cores=2, arrive_terms=10"
python3 problem/interactive_runner.py ./problem/interactor.o in/0005.txt out.txt -- ./sol

# echo "seed=13, n=200, n_cores=2, arrive_terms=10"
# python3 problem/interactive_runner.py ./problem/interactor.o in/0006.txt out.txt -- ./sol
