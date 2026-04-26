# Stage 1: build the server binary using nix
FROM nixos/nix:latest AS builder

# Enable flakes
RUN echo "experimental-features = nix-command flakes" >> /etc/nix/nix.conf

WORKDIR /src
COPY . .

# Build just the server binary
RUN nix build .#default --no-link
# Copy the result out of the nix store
RUN cp -rL $(nix path-info .#default) /build

# Stage 2: minimal runtime image
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates sqlite3 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/bin/lineup_server /usr/local/bin/lineup_server
COPY crates/server/public /app/public

RUN mkdir -p /data /data/demos

ENV HOST=0.0.0.0
ENV PORT=8080
ENV MASTER_DB=/data/master.db
ENV DATA_DIR=/data
ENV PUBLIC_DIR=/app/public

EXPOSE 8080

CMD ["lineup_server"]
