use std::io::{BufRead, BufReader, Write};
use std::process::Command;
use std::sync::mpsc::Receiver;

// Replies come back on a dedicated reader thread: `recv_timeout` gives the
// cross-platform read deadline that libc::poll only provides on unix.
struct Replies(Receiver<serde_json::Value>);

impl Replies {
    fn next(&mut self) -> serde_json::Value {
        self.0
            .recv_timeout(std::time::Duration::from_secs(25))
            .expect("plugin reply timed out")
    }
}

fn spawn() -> (std::process::Child, Replies, std::process::ChildStdin) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_background"))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("spawn plugin");
    let stdout = child.stdout.take().expect("stdout");
    let stdin = child.stdin.take().expect("stdin");
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) | Err(_) => return,
                Ok(_) => {
                    let value = serde_json::from_str(&line).expect("valid reply JSON");
                    if tx.send(value).is_err() {
                        return;
                    }
                }
            }
        }
    });
    (child, Replies(rx), stdin)
}

fn send(stdin: &mut std::process::ChildStdin, value: &serde_json::Value) {
    stdin.write_all(value.to_string().as_bytes()).unwrap();
    stdin.write_all(b"\n").unwrap();
    stdin.flush().unwrap();
}

fn write_image(dir: &std::path::Path, height: u32) -> std::path::PathBuf {
    let path = dir.join(format!("image-{height}.png"));
    image::RgbaImage::from_pixel(2, height, image::Rgba([10, 20, 30, 128]))
        .save_with_format(&path, image::ImageFormat::Png)
        .unwrap();
    path
}

#[test]
fn manifest_registers_background_command() {
    let (mut child, mut replies, mut stdin) = spawn();
    send(
        &mut stdin,
        &serde_json::json!({"id": 1, "method": "plugin/manifest"}),
    );
    let manifest = replies.next()["result"].clone();
    assert_eq!(manifest["name"], "background");
    assert_eq!(manifest["commands"][0], "/background");
    assert_eq!(manifest["protocol"], "1.1");
    assert_eq!(manifest["tools"].as_array().unwrap().len(), 0);
    send(
        &mut stdin,
        &serde_json::json!({"method": "plugin/shutdown"}),
    );
    assert!(child.wait().unwrap().success());
}

#[test]
fn off_sends_null_path_and_reports() {
    let (mut child, mut replies, mut stdin) = spawn();
    send(
        &mut stdin,
        &serde_json::json!({"id": 1, "method": "command/run",
        "params": {"name": "/background", "argv": ["off"]}}),
    );
    let request = replies.next();
    assert_eq!(request["method"], "host/background");
    assert_eq!(request["params"]["path"], serde_json::Value::Null);
    send(
        &mut stdin,
        &serde_json::json!({"id": request["id"], "result": {"ok": true}}),
    );
    assert_eq!(replies.next()["result"]["text"], "Background off.");
    send(
        &mut stdin,
        &serde_json::json!({"method": "plugin/shutdown"}),
    );
    assert!(child.wait().unwrap().success());
}

#[test]
fn set_bakes_opacity_gradient_and_prepares_absolute_png() {
    let dir = tempfile::tempdir().unwrap();
    let source = write_image(dir.path(), 3);
    let (mut child, mut replies, mut stdin) = spawn();
    send(
        &mut stdin,
        &serde_json::json!({"id": 1, "method": "command/run",
        "params": {"name": "/background", "argv": [source, "0.5", "top-to-bottom"]}}),
    );
    let request = replies.next();
    assert_eq!(request["method"], "host/background");
    let prepared = std::path::PathBuf::from(request["params"]["path"].as_str().unwrap());
    assert!(prepared.is_absolute());
    let image = image::ImageReader::open(&prepared)
        .unwrap()
        .decode()
        .unwrap()
        .into_rgba8();
    assert_eq!(image.get_pixel(0, 0).0[3], 64);
    assert_eq!(image.get_pixel(0, 1).0[3], 32);
    assert_eq!(image.get_pixel(0, 2).0[3], 0);
    send(
        &mut stdin,
        &serde_json::json!({"id": request["id"], "result": {"ok": true}}),
    );
    assert_eq!(
        replies.next()["result"]["text"],
        "Background set. /background off removes it."
    );
    assert!(
        !prepared.exists(),
        "temp PNG must be gone after host acknowledgment"
    );
    send(
        &mut stdin,
        &serde_json::json!({"method": "plugin/shutdown"}),
    );
    assert!(child.wait().unwrap().success());
}

#[test]
fn host_errors_surface_and_invalid_requests_stay_local() {
    let dir = tempfile::tempdir().unwrap();
    let source = write_image(dir.path(), 1);
    let (mut child, mut replies, mut stdin) = spawn();
    send(
        &mut stdin,
        &serde_json::json!({"id": 1, "method": "command/run",
        "params": {"name": "/background", "argv": [source, "0.5"]}}),
    );
    let request = replies.next();
    send(
        &mut stdin,
        &serde_json::json!({"id": request["id"], "result": {"error": "boom"}}),
    );
    assert_eq!(replies.next()["result"]["text"], "Error: boom");
    for argv in [
        serde_json::json!(["/definitely-missing.png"]),
        serde_json::json!(["/missing.png", "nan"]),
        serde_json::json!([3]),
    ] {
        send(
            &mut stdin,
            &serde_json::json!({"id": 9, "method": "command/run",
            "params": {"name": "/background", "argv": argv}}),
        );
        let text = replies.next()["result"]["text"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(text.starts_with("Error:"), "{text}");
    }
    send(
        &mut stdin,
        &serde_json::json!({"method": "plugin/shutdown"}),
    );
    assert!(child.wait().unwrap().success());
}

#[test]
fn manifest_requests_queue_behind_in_flight_background() {
    let dir = tempfile::tempdir().unwrap();
    let source = write_image(dir.path(), 1);
    let (mut child, mut replies, mut stdin) = spawn();
    send(
        &mut stdin,
        &serde_json::json!({"id": 1, "method": "command/run",
        "argv": {}, "params": {"name": "/background", "argv": [source, "0.2"]}}),
    );
    let request = replies.next();
    send(
        &mut stdin,
        &serde_json::json!({"id": 2, "method": "plugin/manifest"}),
    );
    send(
        &mut stdin,
        &serde_json::json!({"id": request["id"], "result": {"ok": true}}),
    );
    assert_eq!(
        replies.next()["result"]["text"],
        "Background set. /background off removes it."
    );
    assert_eq!(replies.next()["id"], 2);
    send(
        &mut stdin,
        &serde_json::json!({"method": "plugin/shutdown"}),
    );
    assert!(child.wait().unwrap().success());
}

#[test]
fn shutdown_eof_and_sigterm_remove_pending_temp_image() {
    // Windows has no SIGTERM: TerminateProcess gives no cleanup chance, so
    // only the orderly exits are tested there.
    #[cfg(unix)]
    let modes = ["shutdown", "eof", "sigterm"];
    #[cfg(not(unix))]
    let modes = ["shutdown", "eof"];
    for mode in modes {
        let dir = tempfile::tempdir().unwrap();
        let source = write_image(dir.path(), 1);
        let (mut child, mut replies, mut stdin) = spawn();
        send(
            &mut stdin,
            &serde_json::json!({"id":1,"method":"command/run",
            "params":{"name":"/background","argv":[source]}}),
        );
        let request = replies.next();
        let prepared = std::path::PathBuf::from(request["params"]["path"].as_str().unwrap());
        assert!(prepared.exists());
        match mode {
            "shutdown" => send(&mut stdin, &serde_json::json!({"method":"plugin/shutdown"})),
            #[cfg(unix)]
            "sigterm" => unsafe {
                libc::kill(child.id() as i32, libc::SIGTERM);
            },
            _ => {}
        }
        drop(stdin);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success(), "{mode}: {status}");
                break;
            }
            if std::time::Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("{mode} hung");
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(!prepared.exists(), "{mode}: temporary PNG leaked");
    }
}

#[test]
fn silent_host_times_out_and_plugin_recovers() {
    let (mut child, mut replies, mut stdin) = spawn();
    send(
        &mut stdin,
        &serde_json::json!({"id":1,"method":"command/run",
        "params":{"name":"/background","argv":["off"]}}),
    );
    assert_eq!(replies.next()["method"], "host/background");
    let start = std::time::Instant::now();
    assert!(
        replies.next()["result"]["text"]
            .as_str()
            .unwrap()
            .contains("timed out")
    );
    assert!(start.elapsed().as_secs() < 25);
    send(
        &mut stdin,
        &serde_json::json!({"id":2,"method":"plugin/manifest"}),
    );
    assert_eq!(replies.next()["result"]["name"], "background");
    drop(stdin);
    assert!(child.wait().unwrap().success());
}
