#!/bin/sh
# A project-pinned tool whose data file lives beside it under the pin's `root`.
cat "$(dirname "$0")/../share/data.txt" > pinned.txt
head -c 1048576 /dev/zero | tr '\000' '\377' > blob.bin
