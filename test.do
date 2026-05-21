#!/bin/sh
set -e
exec >&2
redo-ifchange server/test
