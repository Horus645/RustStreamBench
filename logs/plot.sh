#!/bin/sh

set -e

if [ "$#" -ne 5 ]; then
	cat << EOF
Error: Please, enter correct arguments as follows:
	$0 <log_dir> <workers_limit> <APP> <Filter> <Filter column>\n

<APP> is used to determine the graph's name
<Filter> is used to filter the logs
<Filter column> is the column that will contain the data.

    For bzip2, micro-bench, eye-detector and image-processing, <Filter> is
        '^Execution time'
    For bzip2, eye-detector and image-processing, <Filter column> is '3'
	For micro-bench, <Filter column> is '4'

For Bzip2, <Filter> is 'Wall' and <Filter column> is 3
EOF
	exit 1
fi

LOG_DIR="$1"
WORKERS="$2"
APP="$3"
FILTER="$4"
FILCOL="$5"

PLOT_DIR="plot_${3}"
DAT_DIR="$PLOT_DIR/dat"
GNP_DIR="$PLOT_DIR/gnp"
GNP_DATA_EXT=".dat"

if [ ! -d "$DAT_DIR" ]; then
	mkdir -vp "$DAT_DIR"
fi

if [ ! -d "$GNP_DIR" ]; then
	mkdir -vp "$GNP_DIR"
fi

RUNTIMES=""
append_runtime() {
	if echo "$RUNTIMES" | grep -q "$1"; then
		return
	else
		RUNTIMES="$RUNTIMES $1"
	fi
}

filter_output() {
	printf "%s" "$DAT_DIR"/"${APP}-${INPUT}-${RUNTIME}-${NTHREADS}-time$GNP_DATA_EXT"
}

# filter all logs
for INPUT in "$LOG_DIR"/*; do
	INPUT="$(basename "$INPUT")"
	if [ "$INPUT" = "checksums" ]; then
		continue
	fi

	for RUNTIME in "$LOG_DIR/$INPUT"/*; do
		RUNTIME="$(basename "$RUNTIME")"

		if [ "$RUNTIME" = "sequential" ]; then
			SEQ_DAT_FILE="$DAT_DIR"/"${APP}-seq-time$GNP_DATA_EXT"
			awk "/${FILTER}/{ print \$$FILCOL }" \
				"$LOG_DIR"/"$INPUT"/"$RUNTIME" > "$SEQ_DAT_FILE"
			continue
		fi
		append_runtime "$RUNTIME"

		for NTHREADS in "$LOG_DIR/$INPUT/$RUNTIME"/*; do
			NTHREADS="$(basename "$NTHREADS")"
			LOG_FILE="$LOG_DIR"/"$INPUT"/"$RUNTIME"/"$NTHREADS"
			awk "/${FILTER}/{ print \$$FILCOL }" "$LOG_FILE" > "$(filter_output)"
			continue
		done
	done
done

#calculating means and stdv
#shellcheck disable=SC2016
AWK_MEANS_STDV_SCRIPT='{
	sum += $1
	y += $1 ^ 2
}

END {
	printf "%d\t%f\t%f\n", w, sum/NR, sqrt(y/NR-(sum/NR)^2)
}'

for INPUT in "$LOG_DIR"/*; do
	INPUT="$(basename "$INPUT")"
	if [ "$INPUT" = "checksums" ]; then
		continue
	fi
	for RUNTIME in $RUNTIMES; do
		MEANS_FILE="${DAT_DIR}/${APP}_${INPUT}_${RUNTIME}_time_means${GNP_DATA_EXT}"

		# The sequential is considered "parallelism = 0"
		awk -v w=0 "$AWK_MEANS_STDV_SCRIPT" "$SEQ_DAT_FILE" > "$MEANS_FILE"

		for NTHREADS in $(seq 1 "$WORKERS"); do
			awk -v w="$NTHREADS" "$AWK_MEANS_STDV_SCRIPT" "$(filter_output)" >> "$MEANS_FILE"
		done
	done
done

# general plot layout
DATE=$(date)
FONT_SIZE="15"
FONT_TYPE="Helvetica"

LINE_FORMTS='
set xlabel "Workers"

set style line 1 lt 1 lc rgb "#62BCFF" lw LW				#cyan
set style line 2 lt 2 lc rgb "#0BA825" lw LW				#green
set style line 3 lt 3 lc rgb "#0368FF" lw LW				#blue
set style line 4 lt 4 lc rgb "#CB1B00" lw LW				#red
set style line 5 lt 5 lc rgb "#000000" lw LW				#black
set style line 6 lt 6 lc rgb "#002C64" lw LW				#blue_dark
set style line 7 lt 7 lc rgb "#5B9AD1" lw LW				#blue_light
set style line 8 lt 8 lc rgb "#A9A9A9" lw LW				#dark gray
set style line 9 lt 9 lc rgb "#8A2BE2" lw LW				#blue violet
set style line 10 lt 10 lc rgb "#00CED1" lw LW				#dark turquoise
'

for INPUT in "$LOG_DIR"/*; do
	INPUT="$(basename "$INPUT")"
	if [ "$INPUT" = "checksums" ]; then
		continue
	fi

	#plotting all means
	cat << HEADER_PLOT_MEANS > "$GNP_DIR/${APP}-${INPUT}-time-means.gnp"
#Author: Leonardo Gibrowski Faé
#Email: leonardo.fae@edu.pucrs.br
#Version: $DATE
set encoding iso_8859_1
set terminal postscript eps solid color font '$FONT_TYPE,$FONT_SIZE'
set output '${APP}-${INPUT}-time-means.eps'
set style data dots
set grid
set boxwidth 0.1
LW=2
$LINE_FORMTS
set logscale y 2
set key outside top horizontal nobox font '$FONT_TYPE,$FONT_SIZE'
set title '${APP}-${INPUT}-(Time)' offset -2,0,1 noenhanced

set ylabel "Seconds" offset 2,1,1
set xtics 2
HEADER_PLOT_MEANS

	PLOTING_STR='plot \'
	a=1
	for RUNTIME in $RUNTIMES; do
		MEANS_FILE="../$(basename "$DAT_DIR")/${APP}_${INPUT}_${RUNTIME}_time_means${GNP_DATA_EXT}"

		if [ "$a" -ne 1 ]; then
			PLOTING_STR="${PLOTING_STR}, \\"
		fi
		PLOTING_STR="$PLOTING_STR
	'$MEANS_FILE' with linespoints ls $a title \"${RUNTIME}\" axes x1y1,\\
		'' using :2:3 with errorbars ls $a notitle"
		a=$((a + 1))
	done
	printf "%s\n" "$PLOTING_STR" >> "$GNP_DIR/${APP}-${INPUT}-time-means.gnp"
done

#generating all graphs
cd "$GNP_DIR" || exit 1
gnuplot ./*.gnp

#converting eps to pdf
find . -name "*.eps" -exec epstopdf {} ";"

#removing eps
rm -rvf ./*.eps
