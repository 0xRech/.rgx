# syntax=docker/dockerfile:1.7

FROM rust:1.88-bookworm AS builder

RUN apt-get update \
    && apt-get install --yes --no-install-recommends musl-tools \
    && rm -rf /var/lib/apt/lists/* \
    && rustup target add x86_64-unknown-linux-musl

WORKDIR /src

COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --release --locked --target x86_64-unknown-linux-musl

FROM scratch

LABEL org.opencontainers.image.title="RGX" \
      org.opencontainers.image.description="Reference implementation of the .rgx archive format" \
      org.opencontainers.image.source="https://github.com/0xRech/.rgx" \
      org.opencontainers.image.licenses="MIT"

ENV HOME=/root
WORKDIR /data

COPY --from=builder /src/target/x86_64-unknown-linux-musl/release/rgx /usr/local/bin/rgx

ENTRYPOINT ["/usr/local/bin/rgx"]
CMD ["--help"]
