#!/bin/sh
if [ "$1" = "fail" ]; then
    printf 'intentional tool failure' >&2
    exit 7
fi
cp input.txt output.bin
