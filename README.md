# rTemboz

rTemboz is a rewrite of Temboz (https://github.com/fazalmajid/temboz) from
Python to Rust.

It is somewhat functional, but not yet at parity with the original, so I would
advise waiting a few weeks for it to settle as I dogfood it.

## Building

Building is a bit fraught at the moment. In addition to Rust, the process
needs the Vectorscan library installed. On Alpine Linux, the hyperscan-tokio
crate won't build as-is because bindgen needs to be built with libclang linked
statically.

Until I can fix this, I recommend using the Docker builds, using either `make
docker` for an Alpine-based image, or `make docker-ubuntu` if you prefer an
Ubuntu-based image. Alternatively fetch one of the
`fazalmajid/rtemboz:latest` (alias of `fazalmajid/rtemboz:alpine`) or `fazalmajid/rtemboz:ubuntu` images.

## Running

Create a directory that will hold the database, e.g. `/home/majid/temboz` and
then run:

```
docker run -v /home/majid/temboz:/data -p 9998:9998 \
    --user `id -u`:`id -g` \
    --restart=unless-stopped -d --name rtemboz fazalmajid/rtemboz:latest
docker exec -it rtemboz /usr/local/bin/rtemboz change-password majid
```

(change majid to your preferred login and /home/majid/temboz to some existing
directory where the rTemboz database will be kept)

## Migrating from Temboz

* Copy the `rss.db` file from Temboz to the working directory for rTemboz
* Copy the `import.sh` script from this directory to the working directory for
  rTemboz
* run `./import.sh`

Because of differences in feed handling, you may discover a great many new
items that the Python Temboz hadn't recorded. Use the "Deduplicate" or the
"Catch-up" options in the feed info page to fix this. It should only happen
once after the migration.

## TODO

- [x] Authentication!
- [x] Tool to set the password and initial settings
- [x] Actually save the rules to the DB
- [ ] Stemmer endpoint for the "Add Rules" dialog
- [ ] Link to list all articles filtered by a rule in the Filters Actions
      column
- [ ] Delete a rule in the Filters Actions column
- [ ] Feed autodiscovery
- [ ] Feed duplicate title checking
- [ ] Duplicate URL filtering
- [ ] Settings page, including overload threshold
- [ ] Ad-blocking
- [ ] OPML import/export
- [ ] More test cases
- [ ] Better build process
