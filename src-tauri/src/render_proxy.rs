//! A tiny loopback proxy that gives the hidden stack render the same network guard as the static scanner.
//!
//! The scanner (`site_probe`) resolves a name, requires every address to be public and connects to the address it
//! checked. WebView2 resolves names itself, so a public name that rebinds to a private address (DNS rebinding) used to
//! slip past the render's request filter. The render window is now started with `--proxy-server` pointing here, so
//! every page, script and image request leaves through this guard instead:
//!
//! - only `CONNECT host:443` is served (https and wss); plain `http` requests and any other port get a 403;
//! - the host is re-checked with the scanner's own rules (`site_probe::guard_url`: no private literals, no `.local`,
//!   `.internal`, `.lan`, no sensitive pages) and **every** address it resolves to must be public;
//! - the tunnel connects to an address that was checked, so there is no second lookup to rebind;
//! - a small connection cap, a connect timeout and a total lifetime bound what a hostile page can do;
//! - the listener binds `127.0.0.1` on a random port and lives only while one render lives.
//!
//! The tunnel carries the page's own TLS end to end: nothing is decrypted or logged here (only the host name of a
//! refusal, never a path or a query).

use crate::site_probe;
use std::future::Future;
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// Most simultaneous tunnels (a busy page opens ~6 per host).
const MAX_TUNNELS: usize = 96;
/// A request head larger than this is refused.
const MAX_HEAD: usize = 8 * 1024;
const HEAD_TIMEOUT: Duration = Duration::from_secs(8);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);
/// A tunnel that lives longer than this is cut (the render itself is bounded to about 12 s).
const MAX_TUNNEL_LIFE: Duration = Duration::from_secs(40);

pub(crate) type Resolver =
    Arc<dyn Fn(String, u16) -> Pin<Box<dyn Future<Output = std::io::Result<Vec<IpAddr>>> + Send>> + Send + Sync>;

/// Resolves with the operating system (the production resolver).
pub(crate) fn system_resolver() -> Resolver {
    Arc::new(|host, port| {
        Box::pin(async move {
            let addrs = tokio::net::lookup_host((host, port)).await?;
            Ok(addrs.map(|a| a.ip()).collect())
        })
    })
}

#[derive(Clone)]
pub(crate) struct Policy {
    pub resolver: Resolver,
    /// Tests only: let loopback addresses through so a local echo server can stand in for a public site.
    pub allow_loopback: bool,
    /// Tests only: connect to this port instead of the one the page asked for (the proxy serves 443 only).
    pub connect_port: Option<u16>,
}

impl Policy {
    pub fn production() -> Self {
        Policy { resolver: system_resolver(), allow_loopback: false, connect_port: None }
    }
}

/// What the proxy decided for one request head (pure, so each rule has a unit test).
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Decision {
    /// Tunnel to this host and port.
    Tunnel(String, u16),
    /// Refuse with this short reason (never echoed to the page beyond the status line).
    Refuse(&'static str),
}

/// Parse the first line of a proxy request and decide whether it may be tunnelled (before any DNS lookup).
pub(crate) fn decide(head: &str) -> Decision {
    let line = head.lines().next().unwrap_or("").trim();
    let mut parts = line.split_whitespace();
    let (Some(method), Some(target), Some(version)) = (parts.next(), parts.next(), parts.next()) else {
        return Decision::Refuse("malformed request");
    };
    if !version.starts_with("HTTP/1.") {
        return Decision::Refuse("malformed request");
    }
    if !method.eq_ignore_ascii_case("CONNECT") {
        return Decision::Refuse("only https is allowed");
    }
    let (host, port) = match split_host_port(target) {
        Some(v) => v,
        None => return Decision::Refuse("malformed target"),
    };
    if port != 443 {
        return Decision::Refuse("only port 443 is allowed");
    }
    // The scanner's own URL guard: private literals, local names, sensitive pages.
    let probe = if host.contains(':') { format!("https://[{host}]/") } else { format!("https://{host}/") };
    if site_probe::guard_url(&probe).is_err() {
        return Decision::Refuse("address not allowed");
    }
    Decision::Tunnel(host, port)
}

fn split_host_port(target: &str) -> Option<(String, u16)> {
    if target.contains('/') || target.contains('@') || target.contains(' ') {
        return None;
    }
    let (host, port) = if let Some(rest) = target.strip_prefix('[') {
        let end = rest.find(']')?;
        let host = &rest[..end];
        let port = rest[end + 1..].strip_prefix(':')?;
        (host, port)
    } else {
        target.rsplit_once(':')?
    };
    if host.is_empty() || host.len() > 253 {
        return None;
    }
    Some((host.to_ascii_lowercase(), port.parse().ok()?))
}

/// Pick the addresses a tunnel may connect to: all must be public (one private answer refuses the whole name).
pub(crate) fn checked_addresses(addrs: Vec<IpAddr>, allow_loopback: bool) -> Option<Vec<IpAddr>> {
    if addrs.is_empty() {
        return None;
    }
    if addrs.iter().all(|ip| site_probe::ip_is_public(*ip) || (allow_loopback && ip.is_loopback())) {
        Some(addrs)
    } else {
        None
    }
}

pub(crate) struct RenderProxy {
    pub addr: SocketAddr,
    stop: Arc<AtomicBool>,
    handle: tokio::task::JoinHandle<()>,
    refused: Arc<AtomicUsize>,
    tunnelled: Arc<AtomicUsize>,
}

impl RenderProxy {
    /// `http://127.0.0.1:<port>` for `--proxy-server`.
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }
    pub fn refused(&self) -> usize {
        self.refused.load(Ordering::Relaxed)
    }
    pub fn tunnelled(&self) -> usize {
        self.tunnelled.load(Ordering::Relaxed)
    }
    pub fn stop(self) {
        self.stop.store(true, Ordering::SeqCst);
        self.handle.abort();
    }
}

/// Bind a loopback listener on a random port and serve until `stop`.
pub(crate) async fn start(policy: Policy) -> std::io::Result<RenderProxy> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
    let addr = listener.local_addr()?;
    let stop = Arc::new(AtomicBool::new(false));
    let live = Arc::new(AtomicUsize::new(0));
    let refused = Arc::new(AtomicUsize::new(0));
    let tunnelled = Arc::new(AtomicUsize::new(0));
    let (stop2, refused2, tunnelled2) = (stop.clone(), refused.clone(), tunnelled.clone());
    let handle = tokio::spawn(async move {
        loop {
            if stop2.load(Ordering::SeqCst) {
                break;
            }
            let Ok((sock, peer)) = listener.accept().await else { break };
            // Only this machine may use the proxy.
            if !peer.ip().is_loopback() {
                continue;
            }
            if live.load(Ordering::SeqCst) >= MAX_TUNNELS {
                refuse(sock, "busy").await;
                refused2.fetch_add(1, Ordering::Relaxed);
                continue;
            }
            live.fetch_add(1, Ordering::SeqCst);
            let (policy, live, refused, tunnelled) = (policy.clone(), live.clone(), refused2.clone(), tunnelled2.clone());
            tokio::spawn(async move {
                serve(sock, policy, &refused, &tunnelled).await;
                live.fetch_sub(1, Ordering::SeqCst);
            });
        }
    });
    Ok(RenderProxy { addr, stop, handle, refused, tunnelled })
}

async fn refuse(mut sock: TcpStream, why: &str) {
    let _ = sock
        .write_all(format!("HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\nX-Nexus-Refused: {why}\r\n\r\n").as_bytes())
        .await;
    let _ = sock.shutdown().await;
}

async fn read_head(sock: &mut TcpStream) -> Option<(String, Vec<u8>)> {
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    let started = tokio::time::Instant::now();
    loop {
        if let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            let rest = buf[end + 4..].to_vec();
            return Some((String::from_utf8_lossy(&buf[..end]).into_owned(), rest));
        }
        if buf.len() > MAX_HEAD {
            return None;
        }
        let left = HEAD_TIMEOUT.checked_sub(started.elapsed())?;
        let n = tokio::time::timeout(left, sock.read(&mut chunk)).await.ok()?.ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
}

async fn serve(mut sock: TcpStream, policy: Policy, refused: &AtomicUsize, tunnelled: &AtomicUsize) {
    let Some((head, early)) = read_head(&mut sock).await else {
        refused.fetch_add(1, Ordering::Relaxed);
        refuse(sock, "bad request").await;
        return;
    };
    let (host, port) = match decide(&head) {
        Decision::Tunnel(h, p) => (h, p),
        Decision::Refuse(why) => {
            refused.fetch_add(1, Ordering::Relaxed);
            refuse(sock, why).await;
            return;
        }
    };
    // A literal address is checked directly; a name is resolved here and every answer must be public.
    let resolved = if let Ok(ip) = host.parse::<IpAddr>() {
        Ok(vec![ip])
    } else {
        (policy.resolver)(host.clone(), port).await
    };
    let Some(addrs) = resolved.ok().and_then(|a| checked_addresses(a, policy.allow_loopback)) else {
        tracing::info!("render proxy: refused {host} (does not resolve to a public address)");
        refused.fetch_add(1, Ordering::Relaxed);
        refuse(sock, "address not allowed").await;
        return;
    };
    let mut upstream = None;
    for ip in addrs.iter().take(4) {
        if let Ok(Ok(s)) = tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(SocketAddr::new(*ip, policy.connect_port.unwrap_or(port)))).await {
            upstream = Some(s);
            break;
        }
    }
    let Some(mut upstream) = upstream else {
        refused.fetch_add(1, Ordering::Relaxed);
        let _ = sock.write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
        return;
    };
    if sock.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n").await.is_err() {
        return;
    }
    if !early.is_empty() && upstream.write_all(&early).await.is_err() {
        return;
    }
    tunnelled.fetch_add(1, Ordering::Relaxed);
    let _ = tokio::time::timeout(MAX_TUNNEL_LIFE, tokio::io::copy_bidirectional(&mut sock, &mut upstream)).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn fake_resolver(map: &[(&str, &str)]) -> Resolver {
        let map: HashMap<String, IpAddr> = map.iter().map(|(h, ip)| (h.to_string(), ip.parse().unwrap())).collect();
        Arc::new(move |host, _| {
            let ip = map.get(&host).copied();
            Box::pin(async move { ip.map(|i| vec![i]).ok_or_else(|| std::io::Error::other("no such host")) })
        })
    }

    #[test]
    fn only_connect_to_443_is_served() {
        assert_eq!(decide("CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443"), Decision::Tunnel("example.com".into(), 443));
        assert_eq!(decide("connect Example.COM:443 HTTP/1.1"), Decision::Tunnel("example.com".into(), 443));
        assert!(matches!(decide("GET http://example.com/ HTTP/1.1"), Decision::Refuse(_)), "plain http is refused");
        for method in ["GET", "POST", "OPTIONS", "TRACE"] {
            let line = format!("{method} example.com:443 HTTP/1.1");
            assert!(matches!(decide(&line), Decision::Refuse(_)), "{method} is not a tunnel even with a host:port target");
        }
        assert!(matches!(decide("CONNECT example.com:80 HTTP/1.1"), Decision::Refuse(_)), "port 80 is refused");
        assert!(matches!(decide("CONNECT example.com:8443 HTTP/1.1"), Decision::Refuse(_)), "other ports are refused");
        assert!(matches!(decide("CONNECT example.com HTTP/1.1"), Decision::Refuse(_)), "no port");
        assert!(matches!(decide("CONNECT user@example.com:443 HTTP/1.1"), Decision::Refuse(_)), "credentials in the target");
        assert!(matches!(decide("CONNECT example.com/x:443 HTTP/1.1"), Decision::Refuse(_)), "a path in the target");
        assert!(matches!(decide(""), Decision::Refuse(_)));
        assert!(matches!(decide("CONNECT example.com:443"), Decision::Refuse(_)), "no version");
        assert!(matches!(decide("CONNECT example.com:443 SPDY/3"), Decision::Refuse(_)));
    }

    #[test]
    fn private_names_and_literals_are_refused_before_any_lookup() {
        for t in [
            "127.0.0.1:443", "10.1.2.3:443", "192.168.0.10:443", "169.254.169.254:443", "[::1]:443", "[fe80::1]:443",
            "[::ffff:10.0.0.1]:443", "localhost:443", "printer.local:443", "db.internal:443", "nas.lan:443", "x.localhost:443",
            "100.64.0.1:443",
        ] {
            assert!(matches!(decide(&format!("CONNECT {t} HTTP/1.1")), Decision::Refuse(_)), "{t} must be refused");
        }
        assert_eq!(decide("CONNECT 93.184.216.34:443 HTTP/1.1"), Decision::Tunnel("93.184.216.34".into(), 443));
        assert_eq!(decide("CONNECT [2606:2800:220:1::1]:443 HTTP/1.1"), Decision::Tunnel("2606:2800:220:1::1".into(), 443));
    }

    #[test]
    fn one_private_answer_refuses_the_whole_name() {
        let public: IpAddr = "93.184.216.34".parse().unwrap();
        let private: IpAddr = "10.0.0.5".parse().unwrap();
        assert!(checked_addresses(vec![public], false).is_some());
        assert!(checked_addresses(vec![public, private], false).is_none(), "a mixed answer is a rebinding trick");
        assert!(checked_addresses(vec![], false).is_none());
        assert!(checked_addresses(vec!["127.0.0.1".parse().unwrap()], false).is_none());
        assert!(checked_addresses(vec!["127.0.0.1".parse().unwrap()], true).is_some(), "test-only switch");
    }

    async fn ask(proxy: &RenderProxy, req: &str) -> String {
        let mut s = TcpStream::connect(proxy.addr).await.expect("connect to the proxy");
        s.write_all(req.as_bytes()).await.expect("send");
        let mut out = vec![0u8; 512];
        let n = tokio::time::timeout(Duration::from_secs(5), s.read(&mut out)).await.expect("reply in time").unwrap_or(0);
        String::from_utf8_lossy(&out[..n]).into_owned()
    }

    async fn echo_server() -> u16 {
        let echo = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind echo");
        let port = echo.local_addr().expect("addr").port();
        tokio::spawn(async move {
            while let Ok((mut s, _)) = echo.accept().await {
                tokio::spawn(async move {
                    let mut b = [0u8; 64];
                    while let Ok(n) = s.read(&mut b).await {
                        if n == 0 || s.write_all(&b[..n]).await.is_err() {
                            break;
                        }
                    }
                });
            }
        });
        port
    }

    #[tokio::test]
    async fn a_name_that_rebinds_to_a_private_address_is_refused() {
        let echo_port = echo_server().await;
        // production rules: a name resolving to loopback / a private range / metadata is refused, even with a live server behind it
        let policy = Policy {
            resolver: fake_resolver(&[("rebind.test", "127.0.0.1"), ("lan.test", "10.0.0.5"), ("meta.test", "169.254.169.254")]),
            allow_loopback: false,
            connect_port: Some(echo_port),
        };
        let proxy = start(policy).await.expect("start");
        for host in ["rebind.test", "lan.test", "meta.test", "unknown.test"] {
            let reply = ask(&proxy, &format!("CONNECT {host}:443 HTTP/1.1\r\nHost: {host}:443\r\n\r\n")).await;
            assert!(reply.starts_with("HTTP/1.1 403"), "{host} must be refused, got {reply:?}");
        }
        assert_eq!(proxy.tunnelled(), 0, "nothing was tunnelled");
        assert_eq!(proxy.refused(), 4);
        let reply = ask(&proxy, "GET http://example.com/ HTTP/1.1\r\nHost: example.com\r\n\r\n").await;
        assert!(reply.starts_with("HTTP/1.1 403"), "plain http: {reply:?}");
        let reply = ask(&proxy, "CONNECT 127.0.0.1:443 HTTP/1.1\r\n\r\n").await;
        assert!(reply.starts_with("HTTP/1.1 403"), "a loopback literal: {reply:?}");
        proxy.stop();
    }

    #[tokio::test]
    async fn a_public_answer_is_tunnelled_end_to_end() {
        let echo_port = echo_server().await;
        // test-only switches let the local echo server play a public site that resolves to an allowed address
        let policy = Policy { resolver: fake_resolver(&[("ok.test", "127.0.0.1")]), allow_loopback: true, connect_port: Some(echo_port) };
        let proxy = start(policy).await.expect("start");
        let mut c = TcpStream::connect(proxy.addr).await.expect("connect");
        c.write_all(b"CONNECT ok.test:443 HTTP/1.1\r\nHost: ok.test:443\r\n\r\n").await.expect("send");
        let mut head = [0u8; 64];
        let n = tokio::time::timeout(Duration::from_secs(5), c.read(&mut head)).await.expect("in time").expect("read");
        assert!(String::from_utf8_lossy(&head[..n]).starts_with("HTTP/1.1 200"));
        c.write_all(b"hello").await.expect("send body");
        let mut echoed = [0u8; 5];
        tokio::time::timeout(Duration::from_secs(5), c.read_exact(&mut echoed)).await.expect("in time").expect("echo");
        assert_eq!(&echoed, b"hello", "bytes pass both ways untouched");
        assert_eq!(proxy.tunnelled(), 1);
        assert_eq!(proxy.refused(), 0);
        proxy.stop();
    }

    #[test]
    fn the_proxy_source_has_no_unwrap_outside_tests() {
        let src = include_str!("render_proxy.rs");
        let prod = &src[..src.find("#[cfg(test)]").unwrap()];
        assert!(!prod.contains(".unwrap()"), "no unwrap in the proxy's production code");
    }
}
