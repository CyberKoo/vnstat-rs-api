# vnstat-rs-api

A RESTful Web API wrapper for [vnStat](https://humdi.net/vnstat/) network traffic monitoring.

vnstat-rs-api converts vnStat's CLI output into a clean RESTful JSON API, providing endpoints to query network interfaces, traffic statistics (daily, monthly, yearly, etc.), and real-time updates via Server-Sent Events (SSE).

See [CHANGELOG.md](CHANGELOG.md) for release history.

## Features

- **Complete traffic data** via JSON — daily, hourly, 5-minute, monthly, yearly, and top records
- **Real-time live traffic** via SSE (Server-Sent Events)
- **JSend-compliant responses** — consistent JSON response format
- **Response caching** — 60-second cache on vnStat queries reduces system load
- **Configurable** — TOML-based configuration for server address and vnStat executable path
- **Graceful shutdown** — handles SIGTERM / SIGINT cleanly
- **Health check endpoint** — ready for container orchestration (Kubernetes, Docker)

## Quick Start

### Prerequisites

- [vnStat](https://humdi.net/vnstat/) installed and configured on the host system
- Rust toolchain (for building from source)

### Install

```bash
# From source
git clone https://github.com/CyberKoo/vnstat-rs-api.git
cd vnstat-rs-api
cargo build --release
```

### Configure

Create a `config.toml` file:

```toml
[server]
listen = "127.0.0.1"
port = 3000

[vnstat]
executable = "/usr/bin/vnstat"
```

See [config.example.toml](config.example.toml) for all options (including [CORS](#cors-configuration)).

### Run

```bash
./target/release/vnstat-rs-api -c config.toml
```

## API Endpoints

All API endpoints are served under the `/api/v1` prefix. The root path `/`
returns a simple service banner (see below).

### `GET /`

Service banner served at the root path (outside `/api/v1`).

**Response** (`200 OK`):
```json
{
    "status": "success",
    "code": 0,
    "data": "vnstat-rs-api v1.0.1 is up and running"
}
```

### Service-level

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/v1/health` | Service health (liveness) check |
| GET | `/api/v1/version` | vnStat version string |

### Interface data

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/v1/interfaces` | List of interface names |
| GET | `/api/v1/interfaces/summary` | Compact summary for all interfaces |
| GET | `/api/v1/interfaces/stats` | Aggregate traffic statistics |
| GET | `/api/v1/interfaces/{if_name}` | Traffic data for one interface |
| GET | `/api/v1/interfaces/{if_name}/summary` | Compact summary for one interface |
| GET | `/api/v1/interfaces/{if_name}/updated` | Last-updated timestamp |
| GET | `/api/v1/interfaces/{if_name}/live` | Real-time SSE stream |
| GET | `/api/v1/interfaces/{if_name}/periods/day` | Daily traffic records |
| GET | `/api/v1/interfaces/{if_name}/periods/hour` | Hourly traffic records |
| GET | `/api/v1/interfaces/{if_name}/periods/month` | Monthly traffic records |
| GET | `/api/v1/interfaces/{if_name}/periods/year` | Yearly traffic records |
| GET | `/api/v1/interfaces/{if_name}/periods/fiveminute` | 5-minute traffic records |
| GET | `/api/v1/interfaces/{if_name}/periods/top` | Top day records |
| GET | `/api/v1/interfaces/{if_name}/periods/total` | Cumulative total (rx / tx) |

---

### `GET /api/v1/health`

Liveness probe for container orchestration (Kubernetes, Docker). Returns
`200 OK` as long as the server process is running; it does **not** invoke
vnstat.

**Response** (`200 OK`):
```json
{
    "status": "success",
    "code": 0,
    "data": "ok"
}
```

### `GET /api/v1/version`

Returns the vnStat version string.

**Response** (`200 OK`):
```json
{
    "status": "success",
    "code": 0,
    "data": "2.10"
}
```

---

### `GET /api/v1/interfaces`

Returns a list of all monitored network interfaces.

**Response** (`200 OK`):
```json
{
    "status": "success",
    "code": 0,
    "data": ["eth0", "wlan0"]
}
```

### `GET /api/v1/interfaces/summary`

Compact summary for every monitored interface.

**Response** (`200 OK`):
```json
{
    "status": "success",
    "code": 0,
    "data": [
        {
            "name": "eth0",
            "alias": "eth0",
            "total": { "rx": 123456789, "tx": 987654321 },
            "todayRx": 1048576,
            "todayTx": 524288,
            "updatedTimestamp": 1718613000
        }
    ]
}
```

### `GET /api/v1/interfaces/stats`

Aggregate traffic statistics across all interfaces.

**Response** (`200 OK`):
```json
{
    "status": "success",
    "code": 0,
    "data": {
        "totalInterfaces": 3,
        "totalRx": 1234567890000,
        "totalTx": 987654321000
    }
}
```

---

### `GET /api/v1/interfaces/{if_name}`

Returns traffic statistics for a specific interface.

**Path parameters**: `if_name` — interface name (e.g., `eth0`)

**Query parameters**:

| Parameter | Type | Description |
|-----------|------|-------------|
| `periods` | `string` | Comma-separated list of periods to include (`day`, `hour`, `month`, `year`, `fiveminute`, `top`, `total`). All periods are returned when absent. |
| `limit` | `int` | Maximum records per period. No limit when absent. |

**Example**: `GET /api/v1/interfaces/eth0?periods=day,hour,total&limit=7`

**Response** (`200 OK`):
```json
{
    "status": "success",
    "code": 0,
    "data": {
        "name": "eth0",
        "alias": "eth0",
        "traffic": {
            "total": { "rx": 123456789, "tx": 987654321 },
            "day": [ /* ... up to N records ... */ ],
            "hour": [ /* ... up to N records ... */ ],
            "month": [],
            "year": [],
            "fiveminute": [],
            "top": []
        },
        "created": { "date": { "year": 2024, "month": 1, "day": 1 }, "timestamp": 1704067200 },
        "updated": { "date": { "year": 2024, "month": 6, "day": 17 }, "time": { "hour": 10, "minute": 30 }, "timestamp": 1718613000 }
    }
}
```

**Error** (`404 Not Found`):
```json
{
    "status": "fail",
    "code": 10001,
    "message": "no such interface: eth0"
}
```

### `GET /api/v1/interfaces/{if_name}/summary`

Compact summary for a single interface.

**Response** (`200 OK`):
```json
{
    "status": "success",
    "code": 0,
    "data": {
        "name": "eth0",
        "alias": "eth0",
        "total": { "rx": 123456789, "tx": 987654321 },
        "todayRx": 1048576,
        "todayTx": 524288,
        "updatedTimestamp": 1718613000
    }
}
```

### `GET /api/v1/interfaces/{if_name}/updated`

Returns only the last-updated timestamp for an interface.

**Response** (`200 OK`):
```json
{
    "status": "success",
    "code": 0,
    "data": {
        "date": { "year": 2024, "month": 6, "day": 17 },
        "time": { "hour": 10, "minute": 30 },
        "timestamp": 1718613000
    }
}
```

### `GET /api/v1/interfaces/{if_name}/live`

Real-time traffic stream via Server-Sent Events (SSE).

**Path parameters**: `if_name` — interface name

**Response**: SSE stream with `data` events containing JSON lines from `vnstat -l --json`.

### `GET /api/v1/interfaces/{if_name}/periods/{period}`

Returns only a single time period's records for an interface, without the full interface wrapper.

**Path parameters**:

| Parameter | Type | Description |
|-----------|------|-------------|
| `if_name` | `string` | Interface name (e.g., `eth0`) |
| `period` | `string` | One of `day`, `hour`, `month`, `year`, `fiveminute`, `top`, `total` |

**Examples**:

```
GET /api/v1/interfaces/eth0/periods/day
GET /api/v1/interfaces/eth0/periods/hour
GET /api/v1/interfaces/eth0/periods/month
GET /api/v1/interfaces/eth0/periods/year
GET /api/v1/interfaces/eth0/periods/fiveminute
GET /api/v1/interfaces/eth0/periods/top
GET /api/v1/interfaces/eth0/periods/total
```

**Response** (`200 OK`) for `total`:
```json
{
    "status": "success",
    "code": 0,
    "data": { "rx": 123456789, "tx": 987654321 }
}
```

**Response** (`200 OK`) for `top`:
```json
{
    "status": "success",
    "code": 0,
    "data": [
        { "id": 1, "date": { "year": 2024, "month": 6, "day": 15 }, "rx": 52428800, "tx": 26214400, "timestamp": 1718409600 }
    ]
}
```

## Configuration

Full configuration reference:

```toml
[server]
# IP address to listen on. Default: "0.0.0.0"
listen = "0.0.0.0"

# Port to listen on. Default: 3000
port = 3000

[vnstat]
# Path to the vnStat executable. Default: "/usr/bin/vnstat"
executable = "/usr/bin/vnstat"

# Timeout in seconds for each vnstat subprocess call. Default: 5
# query_timeout_secs = 10

[sse]
# Per-subscriber SSE output buffer capacity (number of messages).
# A slow client loses messages instead of blocking the vnstat live-stream
# pipeline. Must be > 0. Default: 4
# subscriber_buffer = 4
```

### CORS Configuration

```toml
[cors]
# Master switch — enable CORS support. Default: false
enabled = false

# Allowed origins. Empty = any origin (`*`).
# When allow_credentials = true, specific origins are required.
# allowed_origins = ["http://localhost:5173", "https://example.com"]

# Allowed HTTP methods. Empty = any method.
# allowed_methods = ["GET", "POST", "OPTIONS"]

# Allowed request headers. Empty = any header.
# allowed_headers = ["Content-Type", "Authorization", "X-Requested-With"]

# Response headers exposed to the browser.
# expose_headers = ["X-RateLimit-Remaining"]

# Allow credentials (cookies, Authorization header).
# When true, allowed_origins must be a non-empty list.
# allow_credentials = true

# Max age (seconds) for preflight caching.
# max_age = 3600
```

#### CORS field reference

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | `bool` | `false` | Master switch |
| `allowed_origins` | `string[]` | `[]` (any origin) | Specific origins allowed |
| `allowed_methods` | `string[]` | `[]` (any method) | Allowed HTTP methods |
| `allowed_headers` | `string[]` | `[]` (any header) | Allowed request headers |
| `expose_headers` | `string[]` | `[]` (safelisted only) | Response headers exposed to JS |
| `allow_credentials` | `bool` | `false` | Allow cookies/auth headers |
| `max_age` | `uint` | `None` (browser default) | Preflight max age (seconds) |

CORS is **disabled by default**. To enable it, set `enabled = true` and adjust other fields as needed.

## Error Codes

| Code  | Description        |
|-------|--------------------|
| 0     | No error           |
| 10000 | Get data failed    |
| 10001 | No such interface  |
| 10002 | Invalid parameter  |
| 10003 | Resource not found |
| 99999 | Unknown error      |

All responses use the [JSend](https://github.com/omniti-labs/jsend) envelope. The
`status` field is `"success"` on 2xx, `"fail"` for client errors (4xx, e.g.
`404 No such interface`), and `"error"` for server errors (5xx, e.g.
`503` when the vnstat query fails or times out).

## Development

```bash
# Run with debug logging
cargo run -- -d

# Build release
cargo build --release

# Run clippy lints
cargo clippy --all-targets

# Run tests
cargo test

# Coverage (line + branch; branch requires nightly)
cargo llvm-cov --workspace          # line coverage (stable)
cargo +nightly llvm-cov --workspace --branch --summary-only
```

## License

This project is licensed under the terms of the [LICENSE](LICENSE) file.
