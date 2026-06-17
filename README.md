# vnstat-rs-api

A RESTful Web API wrapper for [vnStat](https://humdi.net/vnstat/) network traffic monitoring.

vnstat-rs-api converts vnStat's CLI output into a clean RESTful JSON API, providing endpoints to query network interfaces, traffic statistics (daily, monthly, yearly, etc.), and real-time updates via Server-Sent Events (SSE).

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

All endpoints are served under the `/api/v1` prefix.

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/v1/vnstat/` | Complete data for all interfaces |
| GET | `/api/v1/vnstat/health` | vnStat health check |
| GET | `/api/v1/vnstat/version` | vnStat version string |
| GET | `/api/v1/vnstat/interfaces` | List of interface names |
| GET | `/api/v1/vnstat/{if_name}` | Traffic data for one interface |
| GET | `/api/v1/vnstat/{if_name}/live` | Real-time SSE stream |

### `GET /api/v1/vnstat/`

Returns the complete vnStat data for all interfaces.

**Response** (`200 OK`):
```json
{
    "status": "success",
    "code": 0,
    "data": {
        "interfaces": [
            {
                "name": "eth0",
                "alias": "eth0",
                "traffic": {
                    "total": { "rx": 123456789, "tx": 987654321 },
                    "day": [ /* ... daily records ... */ ],
                    "hour": [ /* ... hourly records ... */ ],
                    "month": [ /* ... monthly records ... */ ],
                    "year": [ /* ... yearly records ... */ ],
                    "fiveminute": [ /* ... 5-minute records ... */ ],
                    "top": [ /* ... top records ... */ ]
                },
                "created": { "date": { "year": 2024, "month": 1, "day": 1 }, "timestamp": 1704067200 },
                "updated": { "date": { "year": 2024, "month": 6, "day": 17 }, "time": { "hour": 10, "minute": 30 }, "timestamp": 1718613000 }
            }
        ],
        "jsonversion": "2.0",
        "vnstatversion": "2.10"
    }
}
```

### `GET /api/v1/vnstat/version`

Returns the vnStat version string.

**Response** (`200 OK`):
```json
{
    "status": "success",
    "code": 0,
    "data": "2.10"
}
```

### `GET /api/v1/vnstat/interfaces`

Returns a list of all monitored network interfaces.

**Response** (`200 OK`):
```json
{
    "status": "success",
    "code": 0,
    "data": ["eth0", "wlan0"]
}
```

### `GET /api/v1/vnstat/{if_name}`

Returns traffic statistics for a specific interface.

**Parameters**: `if_name` — interface name (e.g., `eth0`)

**Response** (`200 OK`):
```json
{
    "status": "success",
    "code": 0,
    "data": { /* ... Interface object, same structure as above ... */ }
}
```

**Error** (`400 Bad Request`):
```json
{
    "status": "fail",
    "code": 10001,
    "message": "No such interface"
}
```

### `GET /api/v1/vnstat/{if_name}/live`

Real-time traffic stream via Server-Sent Events (SSE).

**Parameters**: `if_name` — interface name

**Response**: SSE stream with `data` events containing JSON lines from `vnstat -l --json`.

### `GET /api/v1/vnstat/health`

vnStat health check endpoint.

**Response** (`200 OK`):
```json
{
    "status": "success",
    "code": 0,
    "data": "ok"
}
```

**Response** (`503 Service Unavailable`):
```json
{
    "status": "error",
    "code": 10000,
    "message": "vnstat health check failed: ..."
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
| 99999 | Unknown error      |

## Development

```bash
# Run with debug logging
cargo run -- -d

# Build release
cargo build --release

# Run clippy lints
cargo clippy --all-targets
```

## License

This project is licensed under the terms of the [LICENSE](LICENSE) file.
