//! JSONRPC2 server
//!
//! This module implements the connections and streams handling logic for receiving
//! JSONRPC2 requests on a Unix Domain Socket.

use crate::{
    jsonrpc::{api, Request, Response},
    DaemonControl,
};

use std::{
    io,
    os::unix::net,
    path,
    sync::{self, atomic},
    thread, time,
};

// Maximum number of concurrent RPC connections we may accept.
const MAX_CONNECTIONS: u32 = 16;

// Read a command from the stream.
//
// In order to both treat commands separately (respond as soon as we read one), and support
// multiple commands in a single read or in multiple parts, we are given the context as writable
// arguments:
//   - `buf` is the buffer used to read from the socket. It will be extended as needed. It must be
//   initialized.
//   - `end`: The index of the end of the data read from the stream. Since `buf` needs to be
//   initialized with dummy values, it can be very different from `buf.len()`. Used to not check
//   for the separator character in the parts of the buffer with dummy values.
//   - `cursor`: The index at which we checked for the separator character (`\n`). Used to not
//   check twice for it on the same buffer chunk.
fn read_command(
    stream: &mut dyn io::Read,
    buf: &mut Vec<u8>,
    end: &mut usize,
    cursor: &mut usize,
) -> Result<Option<Request>, io::Error> {
    assert!(!buf.is_empty());

    loop {
        // First off, check if there are no existing commands in the buffer.
        let pos = buf[*cursor..*end].iter().position(|byt| byt == &b'\n');
        log::trace!(
            "pos: {:?}, buf[cur..end]: {:?}",
            pos,
            String::from_utf8_lossy(&buf[*cursor..*end])
        );
        if let Some(pos) = pos {
            log::trace!(
                "Parsing Request from: {:?}",
                String::from_utf8_lossy(&buf[..*cursor + pos])
            );
            // TODO: don't return an io::Error here, instead try to parse a Request. Failing that,
            // try to parse a serde_json::Value. Then return accordingly a JSONRPC "malformed
            // request" or "invalid JSON" error.
            let req: Request = serde_json::from_slice(&buf[..*cursor + pos])?;
            *buf = buf[pos + 1..].to_vec(); // FIXME: can we avoid reallocating here?
            *cursor = 0;
            *end -= pos + 1;

            return Ok(Some(req));
        }

        // If nothing can be gathered from the buffer, continue reading.
        let new_read = stream.read(&mut buf[*end..])?;
        if new_read == 0 {
            return Ok(None);
        }

        // If we filled the buffer, increase its size and try again.
        *end += new_read;
        let buffer_filled = *end == buf.len();
        if buffer_filled {
            buf.resize(buf.len() * 2, 0);
            continue;
        }
    }
}

// Handle all messages from this connection.
fn connection_handler(
    control: DaemonControl,
    mut stream: net::UnixStream,
    shutdown: sync::Arc<atomic::AtomicBool>,
) -> Result<(), io::Error> {
    let mut buf = vec![0; 2048];
    let mut end = 0;
    let mut cursor = 0;

    while !shutdown.load(atomic::Ordering::Relaxed) {
        let req = match read_command(&mut stream, &mut buf, &mut end, &mut cursor)? {
            Some(req) => req,
            None => {
                // Connection closed.
                return Ok(());
            }
        };

        let req_id = req.id.clone();
        if &req.method == "stop" {
            shutdown.store(true, atomic::Ordering::Relaxed);
            log::info!("Stopping the liana daemon.");
        }

        log::trace!("JSONRPC request: {:?}", serde_json::to_string(&req));
        let response =
            api::handle_request(&control, req).unwrap_or_else(|e| Response::error(req_id, e));
        log::trace!("JSONRPC response: {:?}", serde_json::to_string(&response));
        if let Err(e) = serde_json::to_writer(&stream, &response) {
            log::error!("Error writing response: '{}'", e);
            return Ok(());
        }
    }

    Ok(())
}

// FIXME: have a decent way to share the DaemonControl between connections. Maybe make it Clone?
/// The main event loop. Wait for connections, and treat requests sent through them.
pub fn rpcserver_loop(
    listener: net::UnixListener,
    daemon_control: DaemonControl,
) -> Result<(), io::Error> {
    // Keep it simple. We don't need great performances so just treat each connection in
    // its thread, with a given maximum number of connections.
    let connections_counter = sync::Arc::from(atomic::AtomicU32::new(0));
    let shutdown = sync::Arc::from(atomic::AtomicBool::new(false));

    listener.set_nonblocking(true)?;
    while !shutdown.load(atomic::Ordering::Relaxed) {
        let (connection, _) = match listener.accept() {
            Ok(c) => c,
            Err(_) => {
                thread::sleep(time::Duration::from_millis(100));
                continue;
            }
        };
        log::trace!("New JSONRPC connection");

        while connections_counter.load(atomic::Ordering::Relaxed) >= MAX_CONNECTIONS {
            thread::sleep(time::Duration::from_millis(50));
        }
        connections_counter.fetch_add(1, atomic::Ordering::Relaxed);

        let handler_id = connections_counter.load(atomic::Ordering::Relaxed);
        thread::Builder::new()
            .name(format!("liana-jsonrpc-{}", handler_id))
            .spawn({
                let control = daemon_control.clone();
                let counter = connections_counter.clone();
                let shutdown = shutdown.clone();

                move || {
                    if let Err(e) = connection_handler(control, connection, shutdown) {
                        log::error!("Error while handling connection {}: '{}'", handler_id, e);
                    } else {
                        log::trace!("Connection {} terminated without error.", handler_id);
                    }
                    counter.fetch_sub(1, atomic::Ordering::Relaxed);
                }
            })?;
    }

    Ok(())
}

// Tries to bind to the socket, if we are told it's already in use try to connect
// to check there is actually someone listening and it's not a leftover from a
// crash.
fn bind(socket_path: &path::Path) -> Result<net::UnixListener, io::Error> {
    match net::UnixListener::bind(socket_path) {
        Ok(l) => Ok(l),
        Err(e) => {
            if e.kind() == io::ErrorKind::AddrInUse {
                return match net::UnixStream::connect(socket_path) {
                    Ok(_) => Err(e),
                    Err(_) => {
                        // Ok, no one's here. Just delete the socket and bind.
                        log::debug!("Removing leftover rpc socket.");
                        std::fs::remove_file(socket_path)?;
                        net::UnixListener::bind(socket_path)
                    }
                };
            }

            Err(e)
        }
    }
}

/// Bind to the UDS at `socket_path`
pub fn rpcserver_setup(socket_path: &path::Path) -> Result<net::UnixListener, io::Error> {
    log::debug!("Binding socket at {}", socket_path.display());
    // Create the socket with RW permissions only for the user
    #[cfg(not(test))]
    let old_umask = unsafe { libc::umask(0o177) };
    #[allow(clippy::all)]
    let listener = bind(socket_path);

    #[cfg(not(test))]
    unsafe {
        libc::umask(old_umask);
    }

    listener
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        jsonrpc::{Params, ReqId},
        testutils::*,
    };

    use std::{env, fs, io::Write, process};

    fn read_one_command(socket_path: &path::Path) -> thread::JoinHandle<Option<Request>> {
        let listener = rpcserver_setup(socket_path).unwrap();
        thread::spawn(move || {
            let (mut conn, _) = listener.accept().unwrap();
            let mut buf = vec![0; 32];
            let mut end = 0;
            let mut cursor = 0;
            read_command(&mut conn, &mut buf, &mut end, &mut cursor).unwrap()
        })
    }

    fn read_all_commands(socket_path: &path::Path) -> thread::JoinHandle<Vec<Request>> {
        let listener = rpcserver_setup(socket_path).unwrap();
        thread::spawn(move || {
            let (mut conn, _) = listener.accept().unwrap();
            let mut buf = vec![0; 32];
            let mut end = 0;
            let mut cursor = 0;
            let mut reqs = Vec::new();

            loop {
                match read_command(&mut conn, &mut buf, &mut end, &mut cursor).unwrap() {
                    Some(req) => {
                        reqs.push(req);
                    }
                    None => return reqs,
                }
            }
        })
    }

    fn write_messages(socket_path: &path::Path, messages: &[&[u8]]) {
        let mut client = net::UnixStream::connect(socket_path).unwrap();
        for mess in messages {
            client.write_all(mess).unwrap();
            // Simulate throttling, this mimics real conditions and actually triggered a crash.
            thread::sleep(time::Duration::from_millis(50));
        }
    }

    #[test]
    fn command_read_single() {
        let socket_path = env::temp_dir().join(format!(
            "lianad-jsonrpc-socket-{}-{:?}",
            process::id(),
            thread::current().id()
        ));

        // A simple dummy request
        let t = read_all_commands(&socket_path);
        let req = br#"{"jsonrpc": "2.0", "id": 0, "method": "test", "params": {"a": "b"}}"#;
        let parsed_req: Request = serde_json::from_slice(req).unwrap();
        write_messages(&socket_path, &[req, b"\n"]);
        let read_req = t.join().unwrap();
        assert_eq!(parsed_req, read_req[0]);

        // Same, but with params as a list and a string id
        let t = read_one_command(&socket_path);
        let req = br#"{"jsonrpc": "2.0", "id": "987-abc", "method": "test", "params": ["a", 10]}"#;
        let parsed_req: Request = serde_json::from_slice(req).unwrap();
        write_messages(&socket_path, &[req, b"\n"]);
        let read_req = t.join().unwrap().unwrap();
        assert_eq!(parsed_req, read_req);

        fs::remove_file(&socket_path).unwrap();
    }

    #[test]
    fn command_read_parts() {
        let socket_path = env::temp_dir().join(format!(
            "lianad-jsonrpc-socket-{}-{:?}",
            process::id(),
            thread::current().id()
        ));

        // A single request written in two parts
        let t = read_one_command(&socket_path);
        let req = br#"{"jsonrpc": "2.0", "id": 0, "method": "test", "params": ["a", 10]}"#;
        let parsed_req: Request = serde_json::from_slice(req).unwrap();
        write_messages(
            &socket_path,
            &[&req[..req.len() / 2], &req[req.len() / 2..], b"\n"],
        );
        let read_req = t.join().unwrap().unwrap();
        assert_eq!(parsed_req, read_req);

        // A single request written in many parts
        let t = read_one_command(&socket_path);
        let req = br#"{"jsonrpc": "2.0", "id": 0, "method": "test", "params": ["a", 10]}"#;
        let parsed_req: Request = serde_json::from_slice(req).unwrap();
        let tmp: Vec<Vec<u8>> = req.iter().map(|c| vec![*c]).collect();
        let mut to_send: Vec<&[u8]> = tmp.iter().map(|v| v.as_slice()).collect();
        to_send.push(b"\n");
        write_messages(&socket_path, &to_send);
        let read_req = t.join().unwrap().unwrap();
        assert_eq!(parsed_req, read_req);

        fs::remove_file(&socket_path).unwrap();
    }

    #[test]
    fn command_read_multiple() {
        let socket_path = env::temp_dir().join(format!(
            "lianad-jsonrpc-socket-{}-{:?}",
            process::id(),
            thread::current().id()
        ));

        // Multiple requests, in parts
        let t = read_all_commands(&socket_path);
        let reqs = [
            &br#"{"jsonrpc": "2.0", "id": 20478, "me"#[..],
            br#"thod": "test", "params": ["a", 10]}"#,
            b"\n",
            br#"{"jsonrpc": "2.0", "id": 20479, "method": "testADZ", "params": {}}"#,
            b"\n",
            br#"{"jsonrpc": "2.0", "id": 20499, "method": "t"#,
            br#"e_edzA", "params": {"ttt": 980}}"#,
            b"\n",
        ];
        let parsed_reqs: Vec<Request> = vec![
            serde_json::from_slice(&[reqs[0], reqs[1]].concat()).unwrap(),
            serde_json::from_slice(reqs[3]).unwrap(),
            serde_json::from_slice(&[reqs[5], reqs[6]].concat()).unwrap(),
        ];
        write_messages(&socket_path, &reqs);
        let read_reqs = t.join().unwrap();
        assert_eq!(parsed_reqs, read_reqs);

        // The same requests, sent at once.
        let t = read_all_commands(&socket_path);
        let req_parts = [
            &br#"{"jsonrpc": "2.0", "id": 20478, "method": "test", "params": ["a", 10]}"#[..],
            b"\n",
            br#"{"jsonrpc": "2.0", "id": 20479, "method": "testADZ", "params": {}}"#,
            b"\n",
            br#"{"jsonrpc": "2.0", "id": 20499, "method": "te_edzA", "params": {"ttt": 980}}"#,
            b"\n",
        ]
        .concat();
        write_messages(&socket_path, &[req_parts.as_slice()]);
        let read_reqs = t.join().unwrap();
        assert_eq!(parsed_reqs, read_reqs);

        fs::remove_file(&socket_path).unwrap();
    }

    #[test]
    fn command_read_linebreak() {
        let socket_path = env::temp_dir().join(format!(
            "lianad-jsonrpc-socket-{}-{:?}",
            process::id(),
            thread::current().id()
        ));

        // Multiple requests, in parts
        let t = read_one_command(&socket_path);
        let mut params = serde_json::map::Map::new();
        params.insert(
            "dummy param".to_string(),
            "dummy value
        with line
        breaks"
                .to_string()
                .into(),
        );
        let req = Request {
            jsonrpc: "2.0".to_string(),
            method: "dummy".to_string(),
            params: Some(Params::Map(params)),
            id: ReqId::Num(0),
        };
        write_messages(&socket_path, &[&serde_json::to_vec(&req).unwrap(), b"\n"]);
        let read_req = t.join().unwrap().unwrap();
        assert_eq!(req, read_req);

        fs::remove_file(&socket_path).unwrap();
    }

    #[test]
    fn server_sanity_check() {
        let ms = DummyLiana::new(DummyBitcoind::new(), DummyDatabase::new());
        let socket_path: path::PathBuf = [
            ms.tmp_dir.as_path(),
            path::Path::new("d"),
            path::Path::new("bitcoin"),
            path::Path::new("lianad_rpc"),
        ]
        .iter()
        .collect();

        let t = thread::spawn(move || ms.rpc_server().unwrap());
        while !socket_path.exists() {
            thread::sleep(time::Duration::from_millis(100));
        }

        let stop_req = Request {
            jsonrpc: "2.0".to_string(),
            method: "stop".to_string(),
            params: None,
            id: ReqId::Num(0),
        };
        write_messages(
            &socket_path,
            &[&serde_json::to_vec(&stop_req).unwrap(), b"\n"],
        );

        t.join().unwrap();
    }
}
