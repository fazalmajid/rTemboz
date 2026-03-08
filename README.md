# rTemboz

rTemboz is a rewrite of Temboz (https://github.com/fazalmajid/temboz) from
Python to Rust.

It is somewhat functional, but not yet at parity with the original, so I would
advise waiting a few weeks for it to settle as I dogfood it.

## TODO

- [X] Authentication!

## Migrating from Temboz

* compile: `cargo build --release`
* Copy the rss.db file from Temboz to the working directory for rTemboz
* run `rtemboz rebuild`
* run `./import.sh`
