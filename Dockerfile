FROM rust:latest as builder
WORKDIR build
ADD . .
RUN cargo build --release

FROM alpine:latest
WORKDIR app
COPY --from=builder /build/target/release/speedtest-exporter /app
CMD ["/app/speedtest-exporter"]
