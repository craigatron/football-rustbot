####################################################################################################
## Builder
####################################################################################################
FROM rust:1.54 AS builder

WORKDIR /footballrobot

COPY Cargo.toml ./
COPY Cargo.lock ./
COPY src/ ./src/

RUN cargo build --release

####################################################################################################
## Final image
####################################################################################################
FROM debian:buster-slim

RUN apt update && apt install -y openssl ca-certificates

WORKDIR /footballrobot
# Copy our build
COPY --from=builder /footballrobot/target/release/football-rustbot /footballrobot
COPY config.json /footballrobot/
RUN mkdir /footballrobot/data

CMD ["/footballrobot/football-rustbot"]