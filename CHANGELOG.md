# Changelog

All notable changes to this project are documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.1.0] - 2026-08-02

### ⚠️ Breaking changes

- Unknown interfaces now return `404 Not Found` instead of `400` (all
  `/interfaces/{if_name}` endpoints). Status is `"fail"`, code `10001`.
- Server-side failures (vnstat subprocess failure, timeout, JSON parse error)
  now return `503 Service Unavailable` instead of `400`. Status is `"error"`,
  code `10000`.
- `GET /api/v1/health` is now a liveness probe: it always returns `200 OK`
  and no longer invokes vnstat. Use `/api/v1/version` or `/api/v1/interfaces`
  to verify vnstat availability.

### Added

- Root path `GET /` returns a service banner, e.g.
  `"vnstat-rs-api v1.1.0 is up and running"`.
- New error code `10002` (Invalid parameter) for malformed query strings.
- Malformed query parameters now return a JSend-formatted `400` response
  instead of axum's default plain-text body.
- New error code `10003` (Resource not found); unmatched paths return a
  JSend-formatted `404` and unsupported methods a JSend-formatted `405`
  (previously axum's default plain-text responses).
- New `[sse]` configuration section: `subscriber_buffer` controls the
  per-subscriber SSE output buffer size (default 4).

### Changed

- All JSON responses are uniformly wrapped in the JSend envelope; no raw
  `json!` bodies remain.
- Error responses include the underlying error detail in `message` and are
  logged at `error` (server-side) / `warn` (client-side) level.
- The health error response message wording changed along with the
  unified error handling.
- SSE live streams no longer apply backpressure: each subscriber gets its
  own bounded channel, and messages are dropped for slow consumers instead
  of blocking the shared vnstat subprocess pipeline.
- CORS with `allow_credentials = true` no longer panics at startup when
  `allowed_methods` / `allowed_headers` are empty (they now mirror the
  request instead of using a wildcard, which the CORS spec forbids).

### Tests

- Added a unit test suite (94 tests) covering config, models, error
  handling, task registry lifecycle, service layer (via fake vnstat
  executables), and all API handlers.
- Line coverage ≈ 98%, branch coverage ≈ 83%.

## [1.0.1] - 2026-07-23

### ⚠️ Breaking changes

- `GET /` now returns the list of interfaces instead of the full vnstat
  dataset.
- `GET /interfaces/{if_name}/traffic` was removed; the same data is now
  served directly by `GET /interfaces/{if_name}`.
- `GET /interfaces/{if_name}` no longer redirects to `/traffic`.

### Added

- `GET /health` liveness endpoint.
- Interface summary endpoints: `GET /interfaces/summary` and
  `GET /interfaces/{if_name}/summary`.
- Aggregate statistics endpoint: `GET /interfaces/stats`.
- Last-update timestamp endpoint: `GET /interfaces/{if_name}/updated`.
- Per-period data endpoints: `GET /interfaces/{if_name}/periods/day|hour|month|year|top|total`.
- `limit` query parameter to cap the number of records returned per period.
- Configurable CORS support (`[cors]` section: origins, methods, headers,
  credentials, max age).
- Configurable vnstat query timeout (`query_timeout_secs` in `[vnstat]`, default 5s).
- SSE event IDs now include a timestamp, improving client-side reconnection
  tracking.
- Committed `Cargo.lock` and a project-level `README.md`.

### Changed

- Task lifecycle management consolidated into a single `task_registry` module;
  handler actions are now atomic so concurrent subscribers cannot observe
  intermediate task states.
- Route layout reorganized: service-level endpoints live at the top level,
  interface routes are nested under `/interfaces`.
- The entire codebase was documented (module- and item-level doc comments).

## [1.0.0] - 2025-09-11

### Added

- Initial release: a RESTful API wrapper around the `vnstat` CLI.
- Endpoints:
  - `GET /` — full vnstat dataset
  - `GET /version` — vnstat version
  - `GET /interfaces` — list of interfaces
  - `GET /interfaces/{if_name}` — redirects to `/traffic`
  - `GET /interfaces/{if_name}/traffic` — per-interface traffic data
  - `GET /interfaces/{if_name}/live` — SSE live stream
- JSend-compliant JSON response envelope (`status`, `code`, `message`, `data`).
- TOML configuration file: server listen address/port and vnstat executable path.
- CLI options: `--config <FILE>` and `--debug`.
- Background task manager that runs vnstat subprocesses and broadcasts live
  output to SSE subscribers.
