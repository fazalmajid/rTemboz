#SSL_HOME=	/usr/local/ssl
SSL_HOME=	$$HOME/local/ssl
ENV=		env CARGO_BACKTRACE=1 OPENSSL_DIR=$(SSL_HOME)
CARGO=		$(ENV) cargo

all: run

.sqlx: boot.db
	clear
	env OPENSSL_DIR=$(SSL_HOME) DATABASE_URL=sqlite://boot.db cargo sqlx prepare

boot.db:
	sqlite3 boot.db < migrations/001_initial.sql

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

clippy-fix: .sqlx assets
	$(CARGO) clippy --fix


test: .venv/bin/pytest
	$(CARGO) build
	.venv/bin/pytest tests/test.py -v

.venv/bin/pytest:
	uv venv
	uv pip install pytest requests

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
