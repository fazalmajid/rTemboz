# --- Build stage ---
FROM alpine:3.23 AS builder

RUN apk add --no-cache \
    rust \
    cargo \
    clang20-static \
    clang20-dev \
    llvm20-dev \
    llvm20-static \
    vectorscan-dev \
    openssl-dev \
    musl-dev \
    pkgconf \
    sqlite \
    make \
    ncurses-dev \
    wget

WORKDIR /build

# Cache dependencies before copying source
COPY Cargo.toml Cargo.lock ./
COPY vendor/ vendor/

# Download datatables assets required by build.rs (static file embedding)
COPY static/ static/
RUN wget -q -O static/datatables.css \
        https://cdn.jsdelivr.net/npm/simple-datatables@latest/dist/style.css && \
    wget -q -O static/simple-datatables.js \
        https://cdn.jsdelivr.net/npm/simple-datatables@latest

# Create the SQLite database from migrations so sqlx can check queries at compile time
COPY migrations/ migrations/
RUN cat migrations/*.sql | sqlite3 boot.db

COPY build.rs ./
COPY src/ src/
COPY templates/ templates/

ENV LIBCLANG_PATH=/usr/lib/llvm20/lib
ENV LLVM_CONFIG_PATH=/usr/lib/llvm20/bin/llvm-config
ENV DATABASE_URL=sqlite://boot.db
ENV OPENSSL_DIR=/usr
ENV HYPERSCAN_DIR=/usr
ENV HYPERSCAN_INCLUDE_PATH=/usr/include
ENV HYPERSCAN_LIB_PATH=/usr/lib
ENV BINDGEN_EXTRA_CLANG_ARGS=-I/usr/include
ENV CARGO_BACKTRACE=1

RUN cargo build --release

# --- Runtime stage ---
FROM alpine:3.23

RUN apk add --no-cache \
    vectorscan \
    openssl \
    libstdc++ \
    ca-certificates

COPY --from=builder /build/target/release/rtemboz /usr/local/bin/rtemboz

ENTRYPOINT ["/usr/local/bin/rtemboz"]
