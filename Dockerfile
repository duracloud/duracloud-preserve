# syntax=docker/dockerfile:1.7

FROM --platform=linux/arm64 gcr.io/distroless/cc-debian12:nonroot
# dcp dynamically links the prebuilt libduckdb, staged by scripts/stage-duckdb-lib.mjs
COPY lib/libduckdb.so /usr/local/lib/
ENV LD_LIBRARY_PATH=/usr/local/lib
COPY target/aarch64-unknown-linux-gnu/release/dcp /usr/local/bin/dcp
ENTRYPOINT ["/usr/local/bin/dcp"]
