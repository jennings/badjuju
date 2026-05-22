#!/bin/sh
set -e
echo release > "$3"
redo-stamp < "$3"
