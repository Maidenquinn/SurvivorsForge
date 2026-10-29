# forge-web: the website, a client of forge-api.
# Build from the repository root: docker build -f docker/forge-web.Dockerfile .

FROM rust:1-bookworm AS build
WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p forge-web \
 && cp target/release/forge-web /forge-web

FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=build /forge-web /usr/local/bin/forge-web
USER nonroot:nonroot
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/forge-web"]
