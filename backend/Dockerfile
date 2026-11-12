# Build a partir da raiz do repositório (usa shared/ e backend/migrations/):
#   docker compose --profile server build backend
FROM rust:1.99.1-slim-trixie AS build
WORKDIR /src
COPY shared shared
COPY backend backend
RUN cargo build --manifest-path backend/Cargo.toml --locked --release

FROM debian:trixie-slim
COPY --from=build /src/backend/target/release/flounder-backend /usr/local/bin/flounder-backend
USER 65534:65534
ENTRYPOINT ["flounder-backend"]
