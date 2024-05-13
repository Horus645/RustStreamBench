#!/bin/sh

if [ $# -lt 2 ]; then
	echo "usage: $0 <inputs directory> <maximum number of workers> "
	exit 1
fi

RUNTIMES="mpi spar-rust-mpi"
WORKERS=$2
INPUTS=$(find "$1" -type f)

LOG_DIR="log-$(date '+%Y-%m-%d_%H:%M:%S:%N')"

set -e
cargo build --release
mkdir "$LOG_DIR"

REPETITIONS=10
for _ in $(seq 1 $REPETITIONS); do
	for workers in $(seq 2 "$WORKERS"); do 
		for input in $INPUTS; do 
			threads=$((workers - 1))
			for runtime in $RUNTIMES; do
				LOG_COMPRESS="${LOG_DIR}/${runtime}/$(basename "$input")/compress/"
				mkdir -p "$LOG_COMPRESS"
				LOG_DECOMPRESS="${LOG_DIR}/${runtime}/$(basename "$input")/decompress/"
				mkdir -p "$LOG_DECOMPRESS"

				echo "Running $input compression with $runtime - $workers"
				mpirun -n "$workers" --oversubscribe ./target/release/bzip2 "$runtime" $threads compress "$input" | tee -a "${LOG_COMPRESS}/${workers}"
				echo "Running ${input}.bz decompression with $runtime - $workers"
				mpirun -n "$workers" --oversubscribe ./target/release/bzip2 "$runtime" $threads decompress "$input".bz2 | tee -a "${LOG_DECOMPRESS}/${workers}"
			done
		done
	done
done
