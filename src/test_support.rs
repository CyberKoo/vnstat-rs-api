//! Shared helpers for unit tests: fake executables and sample vnstat data.
//!
//! Only compiled when running tests (`#[cfg(test)]`).

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

static FILE_COUNTER: AtomicUsize = AtomicUsize::new(0);

static FAKE_VNSTAT_SCRIPT: OnceLock<PathBuf> = OnceLock::new();
static GARBAGE_VNSTAT_SCRIPT: OnceLock<PathBuf> = OnceLock::new();
static SLOW_VNSTAT_SCRIPT: OnceLock<PathBuf> = OnceLock::new();

/// Sample vnstat `--json` output with two interfaces.
pub const SAMPLE_VNSTAT_JSON: &str = r#"{
  "jsonversion": "1",
  "vnstatversion": "2.13",
  "interfaces": [
    {
      "alias": "",
      "created": { "date": { "year": 2025, "month": 8, "day": 15 }, "timestamp": 1755223044 },
      "name": "eth0",
      "traffic": {
        "day": [
          { "date": { "year": 2026, "month": 6, "day": 1 }, "id": 579, "rx": 100000000, "timestamp": 1780243200, "tx": 5000000 },
          { "date": { "year": 2026, "month": 6, "day": 2 }, "id": 580, "rx": 138825634, "timestamp": 1780329600, "tx": 7089952 }
        ],
        "fiveminute": [
          { "date": { "year": 2026, "month": 6, "day": 2 }, "id": 580, "rx": 100, "time": { "hour": 10, "minute": 30 }, "timestamp": 1780331400, "tx": 50 }
        ],
        "hour": [
          { "date": { "year": 2026, "month": 6, "day": 2 }, "id": 580, "rx": 5000, "time": { "hour": 10, "minute": 30 }, "timestamp": 1780331400, "tx": 1000 }
        ],
        "month": [
          { "date": { "month": 6, "year": 2026 }, "id": 18, "rx": 138825634, "timestamp": 1780329600, "tx": 7089952 }
        ],
        "top": [
          { "date": { "year": 2026, "month": 6, "day": 2 }, "id": 580, "rx": 138825634, "timestamp": 1780329600, "tx": 7089952 }
        ],
        "total": { "rx": 123456789, "tx": 987654321 },
        "year": [
          { "date": { "year": 2026 }, "id": 2, "rx": 138825634, "timestamp": 1780329600, "tx": 7089952 }
        ]
      },
      "updated": { "date": { "year": 2026, "month": 6, "day": 2 }, "time": { "hour": 10, "minute": 30 }, "timestamp": 1780331400 }
    },
    {
      "alias": "Wireless",
      "created": { "date": { "year": 2025, "month": 8, "day": 15 }, "timestamp": 1755223044 },
      "name": "wlan0",
      "traffic": {
        "day": [],
        "fiveminute": [],
        "hour": [],
        "month": [],
        "top": [],
        "total": { "rx": 1, "tx": 2 },
        "year": []
      },
      "updated": { "date": { "year": 2026, "month": 6, "day": 2 }, "time": { "hour": 10, "minute": 30 }, "timestamp": 1780331400 }
    }
  ]
}"#;

/// Writes `content` to a unique temp file and returns its path.
pub fn write_temp_file(prefix: &str, content: &str) -> PathBuf {
    let n = FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "vnstat-rs-api-{}-{}-{}",
        prefix,
        std::process::id(),
        n
    ));
    std::fs::write(&path, content).expect("write temp file");
    path
}

/// Writes an executable shell script to a unique temp file and returns its path.
pub fn write_script(content: &str) -> PathBuf {
    let path = write_temp_file("script", content);
    // macOS may return ETXTBSY ("Text file busy") when exec'ing a freshly
    // written file; fsync commits the write before any exec happens.
    if let Ok(file) = std::fs::File::open(&path) {
        let _ = file.sync_all();
    }
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
        .expect("chmod test script");
    path
}

/// A fake vnstat that prints `SAMPLE_VNSTAT_JSON` for `--json` and a live
/// JSON line every 50 ms for `-l`.
///
/// The script is created once per process and shared by all tests: writing
/// each fake executable only once avoids macOS `ETXTBSY` ("Text file busy")
/// failures when exec'ing freshly-written files under parallel test load.
pub fn fake_vnstat_script() -> PathBuf {
    FAKE_VNSTAT_SCRIPT
        .get_or_init(|| {
            let mut content = String::from(
                "#!/bin/sh\nif [ \"$1\" = \"--json\" ]; then\n    printf '%s\\n' '",
            );
            content.push_str(SAMPLE_VNSTAT_JSON);
            content.push_str(
                "'\n    exit 0\nfi\nwhile true; do\n    echo '{\"jsonversion\":\"1\",\"vnstatversion\":\"2.13\",\"interface\":\"eth0\",\"sampletime\":2}'\n    sleep 0.05\ndone\n",
            );
            write_script(&content)
        })
        .clone()
}

/// A fake vnstat that outputs garbage instead of JSON.
pub fn garbage_vnstat_script() -> PathBuf {
    GARBAGE_VNSTAT_SCRIPT
        .get_or_init(|| write_script("#!/bin/sh\nprintf 'this is not json\\n'\n"))
        .clone()
}

/// A fake vnstat that sleeps longer than any test timeout.
pub fn slow_vnstat_script() -> PathBuf {
    SLOW_VNSTAT_SCRIPT
        .get_or_init(|| write_script("#!/bin/sh\nsleep 10\nprintf '{}'\n"))
        .clone()
}
