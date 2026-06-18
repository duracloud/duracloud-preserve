# syntax=docker/dockerfile:1.7

FROM --platform=linux/arm64 gcr.io/distroless/cc-debian12:nonroot
COPY target/aarch64-unknown-linux-gnu/release/dcp /usr/local/bin/dcp
ENTRYPOINT ["/usr/local/bin/dcp"]
