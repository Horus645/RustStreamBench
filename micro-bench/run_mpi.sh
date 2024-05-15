#!/bin/sh

if [ $# -lt 1 ]; then
	echo "usage: $0 <maximum number of workers> "
	exit 1
fi

set -e

RUNTIMES="mpi spar-rust-mpi"
WORKERS=$1
MD=4096 # 2 ^ 12
ITER1=4000
ITER2=3000


LOG_DIR="log-$(date '+%Y-%m-%d_%H:%M:%S:%N')"
LOG_FILE="${LOG_DIR}/log"

cargo build --release --quiet
mkdir "$LOG_DIR"

log() {
	printf "%s - %s\n" "$(date '+%Y-%m-%d|%H:%M:%S:%N')" "$1" | tee -a "$LOG_FILE"
}

REPETITIONS=10
for _ in $(seq 1 $REPETITIONS); do
	workers="$WORKERS"
	while [ "$workers" -ge 3 ]; do 
		threads=$(((workers >> 1) - 1))
		for runtime in $RUNTIMES; do
			LOG_TIME="${LOG_DIR}/${runtime}/${MD}-${ITER1}-${ITER2}"
			mkdir -p "$LOG_TIME"

			log "Running ${MD}-${ITER1}-${ITER2} with $runtime - $workers"
			mpirun -n "$workers" --oversubscribe ./target/release/micro-bench "$runtime" $MD $threads $ITER1 $ITER2 | tee -a "${LOG_TIME}/${workers}"
		done
		workers=$((workers >> 1))
	done
done
