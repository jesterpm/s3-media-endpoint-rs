# Based on https://alexbrand.dev/post/how-to-package-rust-applications-into-minimal-docker-containers/
FROM rust:1.46.0 AS build

MAINTAINER Jesse Morgan <jesse@jesterpm.net>

WORKDIR /usr/src

# Build the dependencies first
# This should help repeated builds go faster.
RUN USER=root cargo new s3-media-endpoint-rs
WORKDIR /usr/src/s3-media-endpoint-rs
COPY Cargo.toml Cargo.lock ./
#RUN cargo install --path .

# Copy the source and build the application.
COPY src ./src
RUN cargo install --path .

# Now build the deployment image.
FROM debian:buster-slim
# RUN apt-get update && apt-get install -y extra-runtime-dependencies && rm -rf /var/lib/apt/lists/*
RUN apt-get update && apt-get install -y libssl1.1 ca-certificates
COPY --from=build /usr/local/cargo/bin/s3-media-endpoint-rs .
USER 999
CMD ["./s3-media-endpoint-rs"]
