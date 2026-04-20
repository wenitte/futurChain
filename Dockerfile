FROM rust:1.95-slim AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock* ./
# Pre-cache dependencies
RUN mkdir src && echo "fn main(){}" > src/main.rs && echo "" > src/lib.rs
RUN cargo build --release 2>/dev/null || true
RUN rm -rf src
# Build the real source
COPY src ./src
RUN touch src/main.rs src/lib.rs && cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/futurchain /usr/local/bin/futurchain
EXPOSE 8899
ENV RUST_LOG=info
ENTRYPOINT ["futurchain"]
CMD ["--host", "0.0.0.0", "--port", "8899"]
