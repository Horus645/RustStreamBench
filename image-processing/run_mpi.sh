#!/bin/sh

if [ $# -lt 2 ]; then
	echo "usage: $0 <inputs directory> <maximum number of workers> "
	exit 1
fi

set -e

RUNTIMES="mpi spar-rust-mpi"
WORKERS=$2

LOG_DIR="log-$(date '+%Y-%m-%d_%H:%M:%S:%N')"
LOG_FILE="$LOG_DIR/log"

cargo build --release
mkdir "$LOG_DIR"

log() {
	printf "%s - %s\n" "$(date '+%Y-%m-%d|%H:%M:%S:%N')" "$1" | tee -a "$LOG_FILE"
}

REPETITIONS=5
for _ in $(seq 1 $REPETITIONS); do
	workers="$WORKERS"
	while [ "$workers" -ge 6 ]; do 
		for input in "$1"/*; do 
			threads=$(((workers - 1) / 5))
			for runtime in $RUNTIMES; do
				LOG="${LOG_DIR}/${runtime}/$(basename "$input")/"
				mkdir -p "$LOG"

				log "Running ${input} with $runtime - $workers"
				mpirun -n "$workers" --oversubscribe \
					./target/release/image-processing "$runtime" $threads "$input" \
					| tee -a "${LOG}/${workers}" "$LOG_FILE"
			done
		done
		workers=$((workers >> 1))
	done
done
