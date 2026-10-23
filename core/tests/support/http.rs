//! A tiny local HTTP server for tests: serves bytes, honours `Range: bytes=N-`, and can drop a connection
//! midway, ignore ranges, or redirect.

use std::{
    collections::{HashMap, HashSet},
    io::{self, Read},
    sync::{Arc, Mutex},
    thread,
};

#[derive(Default)]
struct ServerState {
    files: HashMap<String, Vec<u8>>,
    cut_after: HashMap<String, u64>,
    no_range: HashSet<String>,
    redirects: HashMap<String, String>,
    log: Vec<(String, Option<String>)>,
}

pub struct TestServer {
    pub base: String,
    state: Arc<Mutex<ServerState>>,
    server: Arc<tiny_http::Server>,
}

impl TestServer {
    pub fn start() -> TestServer {
        let server = Arc::new(tiny_http::Server::http("127.0.0.1:0").unwrap());
        let port = server.server_addr().to_ip().unwrap().port();
        let state = Arc::new(Mutex::new(ServerState::default()));
        let (srv, st) = (server.clone(), state.clone());
        thread::spawn(move || {
            for req in srv.incoming_requests() {
                handle(req, &st);
            }
        });
        TestServer {
            base: format!("http://127.0.0.1:{port}/"),
            state,
            server,
        }
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    pub fn put(&self, path: &str, data: Vec<u8>) {
        self.state.lock().unwrap().files.insert(path.into(), data);
    }

    pub fn remove(&self, path: &str) {
        self.state.lock().unwrap().files.remove(path);
    }

    /// The next GET of `path` drops the connection after `bytes` bytes of body.
    pub fn cut_next(&self, path: &str, bytes: u64) {
        self.state
            .lock()
            .unwrap()
            .cut_after
            .insert(path.into(), bytes);
    }

    /// Answers every GET of `path` with the whole file (200), whatever range was asked for.
    pub fn ignore_range(&self, path: &str) {
        self.state.lock().unwrap().no_range.insert(path.into());
    }

    pub fn redirect(&self, path: &str, location: &str) {
        self.state
            .lock()
            .unwrap()
            .redirects
            .insert(path.into(), location.into());
    }

    /// Every request so far: (path, Range header).
    pub fn requests(&self) -> Vec<(String, Option<String>)> {
        self.state.lock().unwrap().log.clone()
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.server.unblock();
    }
}

fn handle(req: tiny_http::Request, state: &Mutex<ServerState>) {
    let path = req.url().trim_start_matches('/').to_string();
    let range = req
        .headers()
        .iter()
        .find(|h| h.field.equiv("Range"))
        .map(|h| h.value.as_str().to_string());
    let (body, cut, no_range, redirect) = {
        let mut st = state.lock().unwrap();
        st.log.push((path.clone(), range.clone()));
        (
            st.files.get(&path).cloned(),
            st.cut_after.remove(&path),
            st.no_range.contains(&path),
            st.redirects.get(&path).cloned(),
        )
    };
    if let Some(location) = redirect {
        let header = tiny_http::Header::from_bytes("Location", location).unwrap();
        let _ = req.respond(tiny_http::Response::empty(302).with_header(header));
        return;
    }
    let Some(body) = body else {
        let _ = req.respond(tiny_http::Response::empty(404));
        return;
    };
    let start = range
        .as_deref()
        .filter(|_| !no_range)
        .and_then(|r| r.strip_prefix("bytes="))
        .and_then(|r| r.strip_suffix('-'))
        .and_then(|n| n.parse::<usize>().ok())
        .filter(|&n| n < body.len());
    let (status, slice, headers) = match start {
        Some(s) => {
            let cr = format!("bytes {}-{}/{}", s, body.len() - 1, body.len());
            (
                206,
                body[s..].to_vec(),
                vec![tiny_http::Header::from_bytes("Content-Range", cr).unwrap()],
            )
        }
        None => (200, body, Vec::new()),
    };
    let len = slice.len();
    let reader: Box<dyn Read + Send> = match cut {
        Some(n) => Box::new(CutReader {
            data: io::Cursor::new(slice),
            left: n,
        }),
        None => Box::new(io::Cursor::new(slice)),
    };
    let _ = req.respond(tiny_http::Response::new(
        tiny_http::StatusCode(status),
        headers,
        reader,
        Some(len),
        None,
    ));
}

/// Fails after `left` bytes, which makes tiny_http drop the connection mid-body.
struct CutReader {
    data: io::Cursor<Vec<u8>>,
    left: u64,
}

impl Read for CutReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.left == 0 {
            return Err(io::Error::new(io::ErrorKind::ConnectionAborted, "cut"));
        }
        let n = (buf.len() as u64).min(self.left) as usize;
        let r = self.data.read(&mut buf[..n])?;
        self.left -= r as u64;
        Ok(r)
    }
}
