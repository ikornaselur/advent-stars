# BUILDER
FROM rust:1.83 as builder
WORKDIR /usr/src
RUN apt-get update -y && apt-get install musl-tools -y
RUN rustup target add x86_64-unknown-linux-musl

RUN USER=root cargo new api
WORKDIR /usr/src/api

RUN echo '[workspace]\nresolver = "2"\nmembers = ["api", "svg"]' > Cargo.toml
COPY svg ./svg
COPY api ./api
RUN cargo build --release --target x86_64-unknown-linux-musl --bin api

# RUNNER
FROM scratch

COPY --from=builder /usr/src/api/target/x86_64-unknown-linux-musl/release/api .
USER 1000
ENV PORT=3000
ENV HOST=0.0.0.0
EXPOSE 3000
CMD ["./api"]
