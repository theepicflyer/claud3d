//! Just enough HTTP/1.1 to POST JSON to the local review server (`cadlib/review/server.py`).

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use serde_json::Value;

pub fn post_json(server: &str, path: &str, body: &str) -> Result<Value, String> {
    let rest = server.strip_prefix("http://").ok_or("the server URL must start with http://")?;
    let host = rest.split('/').next().unwrap_or(rest);
    let mut s = TcpStream::connect(host).map_err(|e| format!("can't reach the review server at {server}: {e}"))?;
    s.set_read_timeout(Some(Duration::from_secs(30))).map_err(|e| e.to_string())?;
    let req = format!(
        "POST {path} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    s.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
    let mut raw = Vec::new();
    s.read_to_end(&mut raw).map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&raw);
    let (head, payload) = text.split_once("\r\n\r\n").ok_or("malformed response")?;
    let code: u16 = head.split_whitespace().nth(1).and_then(|c| c.parse().ok()).ok_or("malformed status line")?;
    let v: Value = serde_json::from_str(payload).unwrap_or(Value::Null);
    if code >= 400 {
        return Err(v["error"].as_str().map_or_else(|| format!("HTTP {code}"), str::to_owned));
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn posts_a_body_and_reads_the_json_reply() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let srv = std::thread::spawn(move || {
            let (mut c, _) = l.accept().unwrap();
            let mut req = String::new();
            let mut buf = [0u8; 4096];
            while !req.ends_with('}') {
                let n = c.read(&mut buf).unwrap();
                req.push_str(&String::from_utf8_lossy(&buf[..n]));
            }
            let body = "{\"ok\": true, \"closing\": true}";
            write!(c, "HTTP/1.0 200 OK\r\nContent-Length: {}\r\n\r\n{body}", body.len()).unwrap();
            req
        });
        let v = post_json(&format!("http://127.0.0.1:{port}"), "/api/send", "{\"new\": []}").unwrap();
        assert_eq!(v["closing"], Value::Bool(true));
        let req = srv.join().unwrap();
        assert!(req.starts_with("POST /api/send HTTP/1.1"));
        assert!(req.ends_with("{\"new\": []}"));
    }
}
