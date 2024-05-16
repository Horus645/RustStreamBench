#!/bin/sh

if [ $# -lt 2 ]; then
	echo "usage: $0 <inputs directory> <maximum number of workers> "
	exit 1
fi

set -e

RUNTIMES="mpi spar-rust-mpi"
WORKERS=$2
INPUTS=$(find "$1" -type f -name '*.mp4')

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
	while [ "$workers" -ge 4 ]; do 
		for input in $INPUTS; do 
			threads=$(((workers - 1) / 3))
			for runtime in $RUNTIMES; do
				LOG="${LOG_DIR}/${runtime}/$(basename -s '.mp4' "$input")/compress/"
				mkdir -p "$LOG"

				log "Running ${input} with $runtime - $workers"
				mpirun -n "$workers" --oversubscribe \
					./target/release/eye-detector "$runtime" $threads "$input" \
					| tee -a "${LOG}/${workers}" "$LOG_FILE"
			done
		done
		workers=$((workers >> 1))
	done
done
