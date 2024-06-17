#!/bin/sh

for plot in data/bzip2-compression/*; do
	python plot.py "$plot" "$(basename "$plot")-compression"
	echo "$(basename "$plot")-compression"
done

for plot in data/bzip2-decompression/*; do
	python plot.py "$plot" "$(basename "$plot")-decompression"
	echo "$(basename "$plot")-decompression"
done

for plot in micro-bench image-processing eye-detector; do
	python plot.py data/"$plot" "$plot"
	echo "$plot"
done
