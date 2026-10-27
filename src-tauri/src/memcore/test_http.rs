//! Shared HTTP mock server for memcore network tests.

#[cfg(test)]
use std::io::{Read, Write};
#[cfg(test)]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(test)]
use std::sync::{Arc, Mutex};
#[cfg(test)]
use std::time::Duration;

#[cfg(test)]
pub(crate) fn start_mock_server<F>(handler: F) -> (String, Arc<Mutex<Vec<String>>>, Arc<AtomicBool>)
where
    F: Fn(&str) -> (u16, Vec<(&'static str, String)>, String) + Send + Sync + 'static,
{
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let addr = listener.local_addr().unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let requests = Arc::new(Mutex::new(Vec::new()));
    let handler = Arc::new(handler);

    let stop_clone = stop.clone();
    let requests_clone = requests.clone();
    std::thread::spawn(move || {
        while !stop_clone.load(Ordering::Relaxed) {
            let Ok((mut sock, _)) = listener.accept() else {
                std::thread::sleep(Duration::from_millis(5));
                continue;
            };
            let _ = sock.set_nonblocking(false);
            let _ = sock.set_read_timeout(Some(Duration::from_secs(2)));
            let mut buf = [0u8; 8192];
            let n = sock.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            requests_clone.lock().unwrap().push(req.clone());

            let (status, headers, body) = handler(&req);
            let mut header_lines = String::new();
            for (k, v) in headers {
                header_lines.push_str(&format!("{k}: {v}\r\n"));
            }
            let resp = format!(
                "HTTP/1.1 {status} OK\r\n{header_lines}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = sock.write_all(resp.as_bytes());
        }
    });

    (format!("http://{addr}"), requests, stop)
}
