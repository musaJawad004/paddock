//! Docker Compose: `compose.yaml`, `compose.yml`, `docker-compose.yml`,
//! `docker-compose.yaml`.
//!
//! Each service becomes a process running `docker compose up <service>` in
//! the foreground, so its logs stream like any other process and stop sends
//! SIGTERM to compose, which stops the container. Published ports are read as
//! hints.
