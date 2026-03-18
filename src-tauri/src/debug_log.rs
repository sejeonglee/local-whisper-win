use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

static LOG_PATH: OnceLock<Option<PathBuf>> = OnceLock::new();

pub fn append(message: impl AsRef<str>) {
    let Some(path) = log_path() else {
        return;
    };

    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let _ = writeln!(file, "[{now}] {}", message.as_ref());
    }
}

fn log_path() -> Option<&'static PathBuf> {
    LOG_PATH
        .get_or_init(|| {
            env::var_os("WHISPER_WINDOWS_DEBUG_LOG")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        })
        .as_ref()
}
