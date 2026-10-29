# forge-api: API server and worker in one image (`forge-api serve`, `forge-api worker`).
# Build from the repository root: docker build -f docker/forge-api.Dockerfile .

FROM rust:1-bookworm AS build
WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p forge-api \
 && cp target/release/forge-api /forge-api \
 && mkdir -p /data/images /data/packs

# No shell, no package manager, not root.
FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=build /forge-api /usr/local/bin/forge-api
# Volumes mounted here start with this owner, so the non-root process can write images and packs.
COPY --from=build --chown=nonroot:nonroot /data /data
USER nonroot:nonroot
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/forge-api"]
CMD ["serve"]
