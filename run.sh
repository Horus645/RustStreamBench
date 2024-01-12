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
REPETITIONS=$(seq 1 10 | tr '\n' ' ')
NTHREADS=$(seq 1 "$(nproc)" | tr '\n' ' ')
CHECKSUM_ERROR_MSG="!!!ERROR!!! Checksums failed to verify:"

check_and_mkdir() {
	if [ ! -d "$1" ]; then
		mkdir -pv "$1" | tee --append $LOG_FILE
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
	CORRECT_CHECKSUM=$(awk '{print $1}' "$1")
	TESTING_CHECKSUM=$(md5sum "$2" | awk '{print $1}')

	if [ "$CORRECT_CHECKSUM" != "$TESTING_CHECKSUM" ]; then
		log "
$CHECKSUM_ERROR_MSG
$1 - $CORRECT_CHECKSUM
$2 - $TESTING_CHECKSUM
"
	else
		log "$(basename "$1") $(basename "$2") MATCH"
	fi
}

run_bzip2() {
	log "BZIP START"

	build_app bzip2
	BENCH_DIR=benchmarks/bzip2
	CHECKSUMS_DIR="$BENCH_DIR"/checksums
	check_and_mkdir "$BENCH_DIR"
	check_and_mkdir "$CHECKSUMS_DIR"

	for I in $REPETITIONS; do
		log "Running bzip sequential: $I"
		# compression
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

		# decompression
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

	for RUNTIME in rust-ssp rust-spp-io spar-rust spar-rust-io spar-rust-v2 spar-rust-v2-io std-threads std-threads-io tokio tokio-io rayon; do
		for I in $REPETITIONS; do
			for T in $NTHREADS; do
				log "Running bzip $RUNTIME with $T threads: $I"

				# compression
				for INPUT in inputs/bzip2/*; do
					INPUT_FILENAME=$(basename "$INPUT")
					check_and_mkdir "$BENCH_DIR"/"$INPUT_FILENAME"/"$RUNTIME"
					BENCHFILE="$BENCH_DIR"/"$INPUT_FILENAME"/"$RUNTIME"/"$T"

					log "writting benchmark to $BENCHFILE"
					./bzip2/target/release/bzip2 "$RUNTIME" "$T" compress "$INPUT" >> "$BENCHFILE"
					OUTFILE="$INPUT".bz2
					verify_checksum "$CHECKSUMS_DIR"/"$(basename "$OUTFILE")".checksum "$OUTFILE"
				done

				# decompression
				for INPUT in inputs/bzip2/*; do
					INPUT_FILENAME=$(basename "$INPUT")
					check_and_mkdir "$BENCH_DIR"/"$INPUT_FILENAME"/"$RUNTIME"
					BENCHFILE="$BENCH_DIR"/"$INPUT_FILENAME"/"$RUNTIME"/"$T"

					log "writting benchmark to $BENCHFILE"
					./bzip2/target/release/bzip2 "$RUNTIME" "$T" decompress "$INPUT" >> "$BENCHFILE"
					OUTFILE=$(dirname "$INPUT")/$(basename --suffix=.bz2 "$INPUT")
					verify_checksum "$CHECKSUMS_DIR"/"$(basename "$OUTFILE")".checksum "$OUTFILE"
				done
			done
		done
	done

	log "BZIP END"
}

run_micro_bench() {
	log "MICRO-BENCH START"
	build_app micro-bench

	MD=2048
	ITER1=3000
	ITER2=2000
	INPUT="${MD}-${ITER1}-${ITER2}"

	BENCH_DIR=benchmarks/micro-bench
	CHECKSUMS_DIR="$BENCH_DIR"/checksums
	CHECKSUMS_FILE="$CHECKSUMS_DIR"/"$INPUT".checksum
	check_and_mkdir "$BENCH_DIR"
	check_and_mkdir "$CHECKSUMS_DIR"

	for I in $REPETITIONS; do
		log "Running micro-bench sequential: $I"
		check_and_mkdir "$BENCH_DIR"/"$INPUT"
		BENCHFILE="$BENCH_DIR"/"$INPUT"/sequential
		./micro-bench/target/release/micro-bench sequential $MD 1 $ITER1 $ITER2 "$INPUT" >> "$BENCHFILE"

		OUTFILE=result_sequential.txt
		if [ ! -f "$CHECKSUMS_FILE" ]; then
			log "Creating checksum for $INPUT"
			md5sum "$OUTFILE" > "$CHECKSUMS_FILE"
		fi
		verify_checksum "$CHECKSUMS_FILE" "$OUTFILE"
		rm "$OUTFILE"
	done

	for RUNTIME in rust-ssp spar-rust spar-rust-v2 std-threads tokio rayon; do
		OUTFILE=result_"$RUNTIME".txt
		for I in $REPETITIONS; do
			for T in $NTHREADS; do
				log "Running micro-bench $RUNTIME with $T threads: $I"

				check_and_mkdir "$BENCH_DIR"/"$INPUT"/"$RUNTIME"
				BENCHFILE="$BENCH_DIR"/"$INPUT"/"$RUNTIME"/"$T"
				./micro-bench/target/release/micro-bench "$RUNTIME" $MD "$T" $ITER1 $ITER2 "$INPUT" >> "$BENCHFILE"
				verify_checksum "$CHECKSUMS_FILE" "$OUTFILE"
				rm "$OUTFILE"
			done
		done
	done

	log "MICRO-BENCH END"
}

run_image_processing_bench() {
	log "IMAGE-PROCESSING START"
	build_app image-processing

	BENCH_DIR=benchmarks/image-processing
	check_and_mkdir "$BENCH_DIR"

	for I in $REPETITIONS; do
		log "Running image-processing sequential: $I"
		for INPUT in ./inputs/image-processing/*; do
			check_and_mkdir "$BENCH_DIR"/"$INPUT"
			BENCHFILE="$BENCH_DIR"/"$INPUT"/sequential
			./image-processing/target/release/image-processing sequential 1 "$INPUT" >> "$BENCHFILE"
		done
	done

	for RUNTIME in rust-ssp spar-rust spar-rust-v2 std-threads tokio rayon; do
		for I in $REPETITIONS; do
			for T in $NTHREADS; do
				log "Running image-processing $RUNTIME with $T threads: $I"
				for INPUT in ./inputs/image-processing/*; do
					check_and_mkdir "$BENCH_DIR"/"$INPUT"/"$RUNTIME"
					BENCHFILE="$BENCH_DIR"/"$INPUT"/"$RUNTIME"/"$T"
					./image-processing/target/release/image-processing "$RUNTIME" "$T" "$INPUT" >> "$BENCHFILE"
				done
			done
		done
	done

	log "IMAGE-PROCESSING END"
}

run_eye_detector_bench() {
	log "EYE-DETECTOR START"
	. ./eye-detector/config_opencv_vars.sh
	build_app eye-detector

	BENCH_DIR=benchmarks/eye-detector
	CHECKSUMS_DIR="$BENCH_DIR"/checksums

	check_and_mkdir "$BENCH_DIR"
	check_and_mkdir "$CHECKSUMS_DIR"

	OUTFILE="output.avi"

	for I in $REPETITIONS; do
		log "Running eye-detector sequential: $I"
		for INPUT in ./inputs/eye-detector/*.mp4; do
			INPUT_FILENAME="$(basename "$INPUT")"
			check_and_mkdir "$BENCH_DIR"/"$INPUT_FILENAME"
			CHECKSUMS_FILE="$CHECKSUMS_DIR"/"$INPUT_FILENAME".checksum
			BENCHFILE="$BENCH_DIR"/"$INPUT_FILENAME"/sequential
			./eye-detector/target/release/eye-detector seq 1 "$INPUT" >> "$BENCHFILE"

			if [ ! -f "$CHECKSUMS_FILE" ]; then
				log "Creating checksum for $INPUT_FILENAME"
				md5sum "$OUTFILE" > "$CHECKSUMS_FILE"
			fi
			verify_checksum "$CHECKSUMS_FILE" $OUTFILE
		done
	done

	for RUNTIME in rust-ssp spar-rust spar-rust-v2 tokio better; do
		for I in $REPETITIONS; do
			for T in $NTHREADS; do
				log "Running eye-detector $RUNTIME with $T threads: $I"
				for INPUT in ./inputs/eye-detector/*.mp4; do
					INPUT_FILENAME="$(basename "$INPUT")"
					check_and_mkdir "$BENCH_DIR"/"$INPUT_FILENAME"/"$RUNTIME"
					CHECKSUMS_FILE="$CHECKSUMS_DIR"/"$INPUT_FILENAME".checksum
					BENCHFILE="$BENCH_DIR"/"$INPUT_FILENAME"/"$RUNTIME"/"$T"
					./eye-detector/target/release/eye-detector "$RUNTIME" "$T" "$INPUT" >> "$BENCHFILE"
					verify_checksum "$CHECKSUMS_FILE" $OUTFILE
				done
			done
		done
	done

	log "EYE-DETECTOR END"
}

if  [ ! -d benchmarks ]; then
	mkdir -pv benchmarks
fi

log "START"
echo >> $LOG_FILE
for APP in $APPS; do
	log "BENCHMARK $APP"

	case "$APP" in
		bzip2) run_bzip2 ;;
		micro-bench) run_micro_bench ;;
		eye-detector) run_eye_detector_bench ;;
		image-processing) run_image_processing_bench ;;
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
