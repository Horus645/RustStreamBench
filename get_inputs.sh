#!/bin/sh

set -e

APPS="
bzip2
eye-detector
image-processing
"
# note micro bench does not have inputs

if [ ! -d inputs ]; then
	mkdir -v inputs
fi

for APP in $APPS; do
	if [ -d inputs/"$APP" ]; then
		echo "Inputs for $APP already exist. Skipping..."
		continue
	fi
	mkdir -vp inputs/"$APP"
	cd inputs/"$APP"

	echo "Getting inputs for $APP..."

	case "$APP" in
		bzip2)
			wget https://gmap.pucrs.br/public_data/RustStreamBench/bzip2/inputs.tar.gz
			tar -xvf inputs.tar.gz
			mv -v inputs/* ./
			rm -rfv inputs.tar.gz inputs
			;;
		eye-detector)
			wget https://gmap.pucrs.br/public_data/RustStreamBench/eye-detector/inputs.tar.gz
			tar -xvf inputs.tar.gz
			rm -rfv inputs.tar.gz
			;;
		image-processing)
			wget https://gmap.pucrs.br/public_data/RustStreamBench/image-processing/inputs.tar.gz
			tar -xvf inputs.tar.gz
			mv inputs/* ./
			rm -rfv inputs.tar.gz inputs

			mkdir -v mixed
			for size in "big" "small"; do
				if [ ! -d "$size" ]; then
					mkdir -v "$size"
				fi
				for i in $(seq 1 1000); do
					cp -v "$size".jpg "$size"/"$i".jpg
				done
				for i in $(seq 1 500); do
					cp -v "$size".jpg mixed/"$size"-"$i".jpg
				done
				rm -v "$size".jpg
			done

			;;
		*)
			echo "ERROR: input for application $APP has not been implemented"
			exit 1
			;;
	esac

	cd ../..
done
