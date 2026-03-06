# rTemboz

rTemboz is a rewrite of Temboz (https://github.com/fazalmajid/temboz) from
Python to Rust.

## Migrating from Temboz

* compile: `cargo build --release`
* Copy the rss.db file from Temboz to the working directory for rTemboz
* run `rtemboz rebuild`
* run `./import.sh`
