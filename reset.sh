#!/bin/sh
set -e

rm temboz.db || true
sqlite3 temboz.db < migrations/001_initial.sql

#./import.sh
sqlite3 temboz.db
