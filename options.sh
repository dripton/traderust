#!/bin/bash

# Vary the --jump, --min-btn, and --min-route-btn options over reasonable
# ranges and produce a tree of output files so we can see what looks best.

# This takes a long time to run and produces a lot of output.  If you have less
# time or a slower computer you should probably reduce the input size to just a
# sector or a few sectors, or reduce the range of the input parameters.

set -x

cargo build -r

for JUMP in {1..6}
do
    for MIN_BTN in 0 0.5 1 1.5 2 2.5 3 3.5 4 4.5 5 5.5 6 6.5 7 7.5 8
    do
        for MIN_ROUTE_BTN in 6 6.5 7 7.5 8 8.5 9 9.5 10
        do
            echo "JUMP $JUMP"
            echo "MIN_BTN $MIN_BTN"
            echo "MIN_ROUTE_BTN $MIN_ROUTE_BTN"
            OUTPUT_DIR=/var/tmp/traderust_output/$JUMP/$MIN_BTN/$MIN_ROUTE_BTN
            mkdir -p /var/tmp/traderust_output/$JUMP/$MIN_BTN/$MIN_ROUTE_BTN/
            time target/release/traderust -vvv -d /var/tmp/traderust/ -o $OUTPUT_DIR -f sector_lists/Im.txt -f sector_lists/CsIm.txt -j $JUMP --min-btn $MIN_BTN --min-route-btn $MIN_ROUTE_BTN 2>&1 | tee $OUTPUT_DIR/out
        done
    done
done

