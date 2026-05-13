# syntax=docker/dockerfile:1.7

FROM --platform=$BUILDPLATFORM ghcr.io/rust-cross/cargo-zigbuild:latest AS builder
WORKDIR /src
RUN rustup target add aarch64-unknown-linux-gnu
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target,id=dcp-target-aarch64 \
    cargo zigbuild --release --locked \
        --target aarch64-unknown-linux-gnu \
        -p dcp --bin dcp \
 && cp target/aarch64-unknown-linux-gnu/release/dcp /dcp

FROM --platform=linux/arm64 gcr.io/distroless/cc-debian12:nonroot
COPY --from=builder /dcp /usr/local/bin/dcp
ENTRYPOINT ["/usr/local/bin/dcp"]
