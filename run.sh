#!/bin/sh

set -e

if [ $# -gt 0 ]; then
	for arg in "$@"; do
		APPS="$APPS $arg"
	done
else
	APPS="
bzip2
micro-bench
eye-detector
image-processing
"
fi

LOG_FILE=benchmarks/log
REPETITIONS=$(seq 1 10)
NTHREADS=$(seq 1 "$(nproc)")
CHECKSUM_ERROR_MSG="!!!ERROR!!! Checksums failed to verify:"

check_and_mkdir() {
	if [ ! -d "$1" ]; then
		mkdir -pv "$1" | tee --apend $LOG_FILE
	fi
}

log() {
	printf "%s - %s\n" "$(date '+%Y-%m-%d|%H:%M:%S:%N')" "$1" | tee --append $LOG_FILE
}

build_app() {
	log "building $1"
	cd "$1"
	cargo build --release
	cd ..
	log "finished building $1"
}

verify_checksum() {
	CORRECT_CHECKSUM=$(cat "$1")	
	TESTING_CHECKSUM=$(md5sum "$2")	
	
	if [ "$CORRECT_CHECKSUM" != "$TESTING_CHECKSUM" ]; then
		log "
$CHECKSUM_ERROR_MSG
$1 - $CORRECT_CHECKSUM
$2 - $TESTING_CHECKSUM
"
	else
		log "checksums match"
	fi
}

run_bzip2() {
	log "BZIP START"

	build_app bzip2
	BENCH_DIR=benchmarks/"$APP"
	CHECKSUMS_DIR="$BENCH_DIR"/checksums
	check_and_mkdir "$BENCH_DIR"
	check_and_mkdir "$CHECKSUMS_DIR"

	for I in $REPETITIONS; do
		log "Running bzip sequential: $I of $REPETITIONS"
		for INPUT in inputs/bzip2/*; do
			INPUT_FILENAME=$(basename "$INPUT")
			if [ ! -f "$CHECKSUMS_DIR"/"$INPUT_FILENAME".checksum ]; then
				log "Creating checksum for $INPUT"
				md5sum "$INPUT" > "$CHECKSUMS_DIR"/"$INPUT_FILENAME".checksum
			fi

			check_and_mkdir "$BENCH_DIR"/"$INPUT_FILENAME"
			BENCHFILE="$BENCH_DIR"/"$INPUT_FILENAME"/sequential
			./bzip2/target/release/bzip2 sequential 1 compress "$INPUT" >> "$BENCHFILE"
		done

		for INPUT in inputs/bzip2/*; do
			INPUT_FILENAME=$(basename "$INPUT")
			if [ ! -f "$CHECKSUMS_DIR"/"$INPUT_FILENAME".checksum ]; then
				log "Creating checksum for $INPUT"
				md5sum "$INPUT" > "$CHECKSUMS_DIR"/"$INPUT_FILENAME".checksum
			fi

			check_and_mkdir "$BENCH_DIR"/"$INPUT_FILENAME"
			BENCHFILE="$BENCH_DIR"/"$INPUT_FILENAME"/sequential
			./bzip2/target/release/bzip2 sequential 1 decompress "$INPUT" >> "$BENCHFILE"
		done
	done

	for RUNTIME in rust-ssp spar-rust spar-rust-io std-threads std-threads-io tokio tokio-io rayon pipeliner; do
		for I in $REPETITIONS; do
			for T in $NTHREADS; do
				log "Running bzip $RUNTIME with $T threads: $I of $REPETITIONS"
				for INPUT in inputs/bzip2/*; do
					INPUT_FILENAME=$(basename "$INPUT")
					BENCHFILE="$BENCH_DIR"/"$INPUT_FILENAME"/"$RUNTIME"

					./bzip2/target/release/bzip2 "$RUNTIME" "$T" compress "$INPUT" >> "$BENCHFILE"
					OUTFILE="$INPUT".bz2
					verify_checksum "$CHECKSUMS_DIR"/"$(basename "$OUTFILE")" "$OUTFILE"
				done

				for INPUT in inputs/bzip2/*; do
					INPUT_FILENAME=$(basename "$INPUT")
					BENCHFILE="$BENCH_DIR"/"$INPUT_FILENAME"/"$RUNTIME"

					./bzip2/target/release/bzip2 "$RUNTIME" "$T" decompress "$INPUT" >> "$BENCHFILE"
					OUTFILE=$(dirname "$INPUT")/$(basename --suffix=.bz2 "$INPUT")
					verify_checksum "$CHECKSUMS_DIR"/"$(basename "$OUTFILE")" "$OUTFILE"
				done
			done
		done
	done

	log "BZIP END"
}

log "START"
check_and_mkdir benchmarks
echo >> $LOG_FILE
for APP in $APPS; do
	log "BENCHMARK $APP"

	case "$APP" in
		bzip2) run_bzip2 ;;
		*)
			log "ERROR: ${APP}'s execution has not been implemented"
			exit 1
			;;
	esac

	log "BENCHMARK FINISH $APP"
done
echo >> $LOG_FILE

if grep -q "$CHECKSUM_ERROR_MSG" "$LOG_FILE"; then
	log "
!!!IMPORTANT!!! FOUND CHECKSUM ERRORS IN $LOG_FILE.
SOME COMMANDS HAVE GENERATED BAD OUTPUTS
"
else
	log "No checksum errors found!"
fi

log "FINISH"
