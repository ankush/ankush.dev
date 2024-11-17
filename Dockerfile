FROM rust:1.82.0 AS chef
RUN cargo install cargo-chef
WORKDIR app

FROM chef AS planner
COPY . .
RUN cargo chef prepare  --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim AS runtime
WORKDIR app
RUN mkdir -p /opt/blog/content
COPY --from=builder /app/target/release/ankush_dev /opt/blog/server
ENTRYPOINT ["/opt/blog/server"]
