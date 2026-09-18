//! Standalone Gray sidecar. Only prepared PNG paths cross the host boundary.
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const HELP: &str =
    "/background <image-path> [opacity 0..1] [none|top-to-bottom|bottom-to-top] | off | status";
const MAX_FRAME: usize = 256 * 1024;
static STOP: AtomicBool = AtomicBool::new(false);
extern "C" fn stop(_: libc::c_int) {
    STOP.store(true, Ordering::Relaxed);
}

#[derive(Debug)]
struct Closed;
impl std::fmt::Display for Closed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("host closed")
    }
}
impl std::error::Error for Closed {}

fn manifest() -> Value {
    json!({"name":"background", "version":env!("CARGO_PKG_VERSION"), "protocol":"1.1",
        "tools":[], "commands":["/background"], "hooks":[]})
}

fn expanduser(path: &Path) -> Result<PathBuf> {
    let text = path.to_string_lossy();
    if text == "~" || text.starts_with("~/") {
        return Ok(
            PathBuf::from(std::env::var_os("HOME").context("HOME is not set")?)
                .join(text.strip_prefix("~/").unwrap_or("")),
        );
    }
    Ok(path.to_path_buf())
}

fn prepare_png(path: &Path, opacity: f64, gradient: &str) -> Result<Vec<u8>> {
    ensure!(
        opacity.is_finite() && (0.0..=1.0).contains(&opacity),
        "opacity must be a finite number between 0 and 1"
    );
    ensure!(
        matches!(gradient, "none" | "top-to-bottom" | "bottom-to-top"),
        "gradient must be none, top-to-bottom, or bottom-to-top"
    );
    use std::os::unix::fs::OpenOptionsExt;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(expanduser(path)?)?;
    ensure!(
        file.metadata()?.is_file(),
        "background must be a regular file"
    );
    let mut bytes = Vec::new();
    file.take(32 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 32 * 1024 * 1024, "image exceeds 32 MiB");
    let format = image::guess_format(&bytes)?;
    let dimensions =
        image::ImageReader::with_format(std::io::Cursor::new(&bytes), format).into_dimensions()?;
    ensure!(
        u64::from(dimensions.0) * u64::from(dimensions.1) <= 16_000_000,
        "image exceeds 16 million pixels"
    );
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(&bytes), format);
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let source = reader.decode()?;
    let mut image = if source.width() > 1920 || source.height() > 1080 {
        source.thumbnail(1920, 1080).into_rgba8()
    } else {
        source.into_rgba8()
    };
    let height = image.height();
    // Bake once; preserve original transparency and round like the Python prototype.
    for (y, row) in image.rows_mut().enumerate() {
        let fade = if height <= 1 || gradient == "none" {
            1.0
        } else {
            let fraction = y as f64 / f64::from(height - 1);
            if gradient == "top-to-bottom" {
                1.0 - fraction
            } else {
                fraction
            }
        };
        for pixel in row {
            pixel.0[3] = (f64::from(pixel.0[3]) * opacity * fade).round_ties_even() as u8;
        }
    }
    let mut png = std::io::Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png)?;
    ensure!(
        png.get_ref().len() <= 8 * 1024 * 1024,
        "prepared PNG exceeds 8 MiB"
    );
    Ok(png.into_inner())
}

#[derive(Default)]
struct Wire {
    bytes: Vec<u8>,
    pending: VecDeque<Value>,
    next_id: u64,
}
impl Wire {
    fn read(&mut self, deadline: Option<Instant>) -> Result<Value> {
        loop {
            if STOP.load(Ordering::Relaxed) {
                return Err(Closed.into());
            }
            if let Some(end) = deadline {
                ensure!(Instant::now() < end, "host/background timed out");
            }
            if let Some(end) = self.bytes.iter().position(|b| *b == b'\n') {
                ensure!(end <= MAX_FRAME, "host frame exceeds 256 KiB");
                let line: Vec<u8> = self.bytes.drain(..=end).collect();
                let Ok(value) = serde_json::from_slice::<Value>(&line) else {
                    continue;
                };
                if value["method"] == "plugin/shutdown" {
                    return Err(Closed.into());
                }
                if value.is_object() {
                    return Ok(value);
                }
                continue;
            }
            ensure!(self.bytes.len() <= MAX_FRAME, "host frame exceeds 256 KiB");
            let mut descriptor = libc::pollfd {
                fd: libc::STDIN_FILENO,
                events: libc::POLLIN,
                revents: 0,
            };
            // No reader thread: bounded polling also lets termination drop temporary files.
            let ready = unsafe { libc::poll(&mut descriptor, 1, 100) };
            if ready < 0 {
                let error = std::io::Error::last_os_error();
                if error.kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(error.into());
            }
            if ready == 0 {
                continue;
            }
            let mut chunk = [0; 8192];
            let size = std::io::stdin().read(&mut chunk)?;
            if size == 0 {
                return Err(Closed.into());
            }
            self.bytes.extend_from_slice(&chunk[..size]);
        }
    }

    fn send(&self, value: Value) -> Result<()> {
        let mut out = std::io::stdout().lock();
        serde_json::to_writer(&mut out, &value)?;
        out.write_all(b"\n")?;
        out.flush()?;
        Ok(())
    }

    fn background(&mut self, path: Option<&Path>) -> Result<()> {
        self.next_id += 1;
        let id = format!("background-{}", self.next_id);
        self.send(json!({"id":id,"method":"host/background","params":{"path":path}}))?;
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let reply = self.read(Some(deadline))?;
            if reply["id"] == id {
                ensure!(
                    reply["result"]["ok"] == true,
                    "{}",
                    reply["result"]["error"]
                        .as_str()
                        .unwrap_or("host rejected background")
                );
                return Ok(());
            }
            if reply.get("id").is_some() && reply.get("method").is_some() {
                ensure!(self.pending.len() < 64, "too many pending host requests");
                self.pending.push_back(reply);
            }
        }
    }
}

fn command(params: &Value, wire: &mut Wire, last: &mut Option<PathBuf>) -> Result<String> {
    ensure!(params["name"] == "/background", "unknown command");
    let empty = Vec::new();
    let argv = match params.get("argv") {
        None => &empty,
        Some(value) => value
            .as_array()
            .context("argv must be an array of strings")?,
    };
    let argv = argv
        .iter()
        .map(|v| v.as_str().context("argv must be an array of strings"))
        .collect::<Result<Vec<_>>>()?;
    match argv.as_slice() {
        [] => Ok(HELP.into()),
        ["status"] => Ok(format!(
            "Background last requested by this plugin: {}",
            last.as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "off".into())
        )),
        ["off"] => {
            wire.background(None)?;
            *last = None;
            Ok("Background off.".into())
        }
        _ => {
            ensure!(argv.len() <= 3, "{HELP}");
            let opacity = argv
                .get(1)
                .unwrap_or(&"0.2")
                .parse::<f64>()
                .context("opacity must be a finite number between 0 and 1")?;
            let path = expanduser(Path::new(argv[0]))?;
            let png = prepare_png(&path, opacity, argv.get(2).unwrap_or(&"none"))?;
            let temp = tempfile::Builder::new()
                .prefix("gray-background-")
                .tempdir()?;
            let prepared = temp.path().join("background.png");
            std::fs::write(&prepared, png)?;
            wire.background(Some(&prepared))?;
            *last = Some(path.canonicalize().unwrap_or(path));
            Ok("Background set. /background off removes it.".into())
        }
    }
}

fn serve() -> Result<()> {
    let mut wire = Wire::default();
    let mut last = None;
    loop {
        let request = match wire.pending.pop_front() {
            Some(v) => v,
            None => wire.read(None)?,
        };
        let Some(id) = request.get("id") else {
            continue;
        };
        let Some(method) = request["method"].as_str() else {
            continue;
        };
        let result = match method {
            "plugin/manifest" => manifest(),
            "command/run" => match command(&request["params"], &mut wire, &mut last) {
                Ok(text) => json!({"text":text}),
                Err(error) if error.is::<Closed>() => return Err(error),
                Err(error) => json!({"text":format!("Error: {error:#}")}),
            },
            _ => json!({"error":"unknown method"}),
        };
        wire.send(json!({"id":id,"result":result}))?;
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args == ["--help"] {
        println!(
            "Gray Background {}\n{HELP}\nRun without arguments as a Gray sidecar.",
            env!("CARGO_PKG_VERSION")
        );
        return Ok(());
    }
    if args == ["--version"] {
        println!("background {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if !args.is_empty() {
        bail!("unknown arguments; use --help");
    }
    for sig in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
        unsafe {
            libc::signal(sig, stop as *const () as libc::sighandler_t);
        }
    }
    match serve() {
        Err(e) if e.is::<Closed>() => Ok(()),
        result => result,
    }
}

#[cfg(test)]
#[path = "image_tests.rs"]
mod image_tests;
