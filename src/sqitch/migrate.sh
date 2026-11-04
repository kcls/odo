#!/bin/sh
# Entrypoint for the migration image (src/sqitch/Dockerfile): deploy the
# core sqitch project to DATABASE_URL.
#
# A non-zero exit is the whole point: run as a PreSync hook, it is what
# stops the new release's pods from being rolled out. sqitch deploy's
# default mode reverts everything this run deployed when a change fails,
# so a failure leaves the schema where it started.

set -eu

: "${DATABASE_URL:?DATABASE_URL is not set}"

echo "Schema before:"
sqitch status --target "$DATABASE_URL" || true

exec sqitch deploy --target "$DATABASE_URL" "$@"
