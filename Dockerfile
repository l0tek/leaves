FROM rust:1-bookworm AS builder
WORKDIR /app
COPY Cargo.toml ./
COPY src ./src
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release && cp target/release/leaves /app/leaves

FROM debian:bookworm-slim
RUN groupadd --gid 10001 leaves && useradd --uid 10001 --gid leaves --create-home leaves \
    && mkdir /data && chown leaves:leaves /data
COPY --from=builder /app/leaves /usr/local/bin/leaves
USER leaves
ENV DATA_DIR=/data LISTEN_ADDR=0.0.0.0:8080
VOLUME ["/data"]
EXPOSE 8080
CMD ["leaves"]
