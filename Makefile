#SSL_HOME=	/usr/local/ssl
#SSL_HOME=	$$HOME/local/ssl
PREFIX=		/home/majid/local
#PREFIX=		/usr/local
SSL_HOME=	$(shell openssl version -a | grep OPENSSLDIR | cut -d " " -f 2|tr -d '"')
ENV=		env CARGO_BACKTRACE=1 OPENSSL_DIR=$(SSL_HOME) \
		HYPERSCAN_DIR=$(PREFIX) \
		HYPERSCAN_INCLUDE_PATH=$(PREFIX)/include \
		HYPERSCAN_LIB_PATH=$(PREFIX)/lib \
		BINDGEN_EXTRA_CLANG_ARGS=-I$(PREFIX)/include \
		LIBCLANG_PATH=$(PREFIX)/lib \
		RUSTFLAGS="-C link-arg=-Wl,-rpath,$(PREFIX)/lib"

CARGO=		$(ENV) cargo

all: run

.sqlx: boot.db $(HOME)/.cargo/bin/sqlx
	clear
	$(ENV) DATABASE_URL=sqlite://boot.db cargo sqlx prepare

$(HOME)/.cargo/bin/sqlx:
	env OPENSSL_DIR=$(SSL_HOME) cargo install sqlx-cli

boot.db:
	cat migrations/*.sql|sqlite3 boot.db

assets: static/datatables.css static/simple-datatables.js target/debug/rtemboz

static/datatables.css:
	wget -c -O $@ https://cdn.jsdelivr.net/npm/simple-datatables@latest/dist/style.css

static/simple-datatables.js:
	wget -c -O $@ https://cdn.jsdelivr.net/npm/simple-datatables@latest

target/debug/rtemboz:
	$(CARGO) build

release target/release/rtemboz:
	$(CARGO) build --release

build run check clippy fmt: .sqlx assets
	$(CARGO) $@

rebuild: target/debug/rtemboz
	target/debug/rtemboz rebuild

clippy-fix: .sqlx assets
	$(CARGO) clippy --fix


test: .venv/bin/pytest
	$(CARGO) build
	$(ENV) .venv/bin/pytest tests/test.py -v

.venv/bin/pytest:
	uv venv
	uv pip install pytest requests

docker:
	docker build . -t fazalmajid/rtemboz:latest
	docker build . -t fazalmajid/rtemboz:alpine

docker-ubuntu:
	docker build -f Dockerfile.ubuntu . -t fazalmajid/rtemboz:ubuntu

push-docker docker-push: docker docker-ubuntu
	docker push fazalmajid/rtemboz:latest
	docker push fazalmajid/rtemboz:alpine
	docker push fazalmajid/rtemboz:ubuntu

upgrade:
	$(CARGO) install cargo-edit
	$(CARGO) upgrade

clean:
	-find . -name \*~ -exec rm {} \;
	-rm -rf .sqlx
	-rm -rf .venv
	-rm -f boot.db
	-rm -f static/datatables.css
	-rm -f static/simple-datatables.js
	-rm -rf target

realclean: clean
	-cargo clean
