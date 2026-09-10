//! A loopback HTTP server that speaks the browser `<video>` byte-range contract.
//!
//! Browsers drive playback and seeking with `Range` requests, so the server has
//! to answer `HEAD`, partial `GET` (`206` + `Content-Range`), and unsatisfiable
//! ranges (`416`) correctly. It binds `127.0.0.1` on an ephemeral port and gates
//! every request behind a per-launch random token, so nothing outside this
//! machine — and nothing on it that lacks the token — can read the media.

use std::io::{self, SeekFrom};
use std::net::{Ipv4Addr, SocketAddr};
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::extract::{Path as UrlPath, State};
use axum::http::{HeaderMap, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::any;
use bytes::Bytes;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

/// The largest slice of the source read into memory at once while streaming.
///
/// Response bodies are streamed rather than buffered, so a request for the whole
/// file costs at most one chunk of memory at a time.
pub const CHUNK_SIZE: usize = 64 * 1024;

/// Only H.264/AAC MP4 is in scope, so the served type is not negotiated.
const CONTENT_TYPE: &str = "video/mp4";

/// A read-only, random-access byte source behind the media server.
///
/// The Host serves its own disk ([`LocalFile`]); a Guest will serve the same
/// contract from a remote range fetch. Keeping the server behind this seam means
/// the HTTP contract is identical on both sides and testable without a network.
#[async_trait]
pub trait MediaSource: Send + Sync + 'static {
    /// The total number of bytes in the source.
    fn len(&self) -> u64;

    /// Whether the source holds no bytes.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Read up to `buf.len()` bytes starting at `offset`.
    ///
    /// Returns the number of bytes read, or `0` once `offset` is at or past the
    /// end. An implementation must never write more than `buf.len()` bytes.
    async fn read_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<usize>;
}

/// A [`MediaSource`] backed by a file on the local disk.
pub struct LocalFile {
    file: tokio::sync::Mutex<tokio::fs::File>,
    len: u64,
}

impl LocalFile {
    /// Open a file and capture its length up front.
    pub async fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let file = tokio::fs::File::open(path).await?;
        let len = file.metadata().await?.len();
        Ok(Self {
            file: tokio::sync::Mutex::new(file),
            len,
        })
    }
}

#[async_trait]
impl MediaSource for LocalFile {
    fn len(&self) -> u64 {
        self.len
    }

    async fn read_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<usize> {
        let mut file = self.file.lock().await;
        file.seek(SeekFrom::Start(offset)).await?;
        file.read(buf).await
    }
}

/// Serves one [`MediaSource`] to the webview over loopback HTTP.
pub struct MediaServer {
    addr: SocketAddr,
    token: String,
    task: JoinHandle<()>,
}

#[derive(Clone)]
struct AppState {
    token: Arc<str>,
    source: Arc<dyn MediaSource>,
}

impl MediaServer {
    /// Bind a fresh server on an ephemeral loopback port and start serving.
    pub async fn bind(source: Arc<dyn MediaSource>) -> io::Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
        let addr = listener.local_addr()?;
        let token = random_token();
        let state = AppState {
            token: token.as_str().into(),
            source,
        };

        let router = Router::new()
            .route("/media/{token}", any(handle))
            .with_state(state);

        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });

        Ok(Self { addr, token, task })
    }

    /// Serve a file on the local disk, the Host's own media path.
    pub async fn serve_file(path: impl AsRef<Path>) -> io::Result<Self> {
        let source = Arc::new(LocalFile::open(path).await?);
        Self::bind(source).await
    }

    /// The `<video src>` URL, including the access token.
    pub fn url(&self) -> String {
        format!("http://{}/media/{}", self.addr, self.token)
    }

    /// The loopback address the server is bound to.
    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    /// The per-launch access token embedded in [`url`](Self::url).
    pub fn token(&self) -> &str {
        &self.token
    }
}

impl Drop for MediaServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// What a `Range` header asks for, resolved against a known source length.
#[derive(Debug, PartialEq, Eq)]
enum ParsedRange {
    /// Not something we implement (or malformed); serve the whole source.
    Full,
    /// A satisfiable byte range, `[start, end)`.
    Satisfiable { start: u64, end: u64 },
    /// Well formed, but no bytes of the source satisfy it.
    Unsatisfiable,
}

/// Parse a single-range `Range` header against a source of `len` bytes.
///
/// Only `bytes=` single ranges are honoured — multipart/byteranges buys nothing
/// for a `<video>` element. Anything malformed or unsupported is ignored and the
/// caller serves the whole source, as RFC 9110 permits. A well-formed range that
/// starts past the end is [`ParsedRange::Unsatisfiable`].
fn parse_range(value: &str, len: u64) -> ParsedRange {
    let value = value.trim();
    let Some(rest) = value
        .get(..6)
        .filter(|prefix| prefix.eq_ignore_ascii_case("bytes="))
        .map(|_| &value[6..])
    else {
        return ParsedRange::Full;
    };

    if rest.contains(',') {
        return ParsedRange::Full;
    }
    let Some((first, last)) = rest.split_once('-') else {
        return ParsedRange::Full;
    };
    let (first, last) = (first.trim(), last.trim());

    if first.is_empty() {
        // Suffix range: the final `last` bytes of the source.
        let Ok(suffix) = last.parse::<u64>() else {
            return ParsedRange::Full;
        };
        if suffix == 0 || len == 0 {
            return ParsedRange::Unsatisfiable;
        }
        return ParsedRange::Satisfiable {
            start: len.saturating_sub(suffix),
            end: len,
        };
    }

    let Ok(start) = first.parse::<u64>() else {
        return ParsedRange::Full;
    };
    if start >= len {
        return ParsedRange::Unsatisfiable;
    }
    let end = if last.is_empty() {
        len
    } else {
        let Ok(end_inclusive) = last.parse::<u64>() else {
            return ParsedRange::Full;
        };
        if end_inclusive < start {
            return ParsedRange::Full;
        }
        end_inclusive.saturating_add(1).min(len)
    };
    ParsedRange::Satisfiable { start, end }
}

/// Route every method through here and answer it, or reject it.
async fn handle(
    State(state): State<AppState>,
    UrlPath(token): UrlPath<String>,
    method: Method,
    headers: HeaderMap,
) -> Response {
    if token != state.token.as_ref() {
        return StatusCode::NOT_FOUND.into_response();
    }
    if method != Method::GET && method != Method::HEAD {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }

    let len = state.source.len();
    let head_only = method == Method::HEAD;
    let range = headers
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok());

    // A HEAD answers with the headers a GET would produce, but no body.
    let body_for = |start: u64, end: u64| {
        if head_only {
            Body::empty()
        } else {
            streaming_body(state.source.clone(), start, end)
        }
    };

    match range.map(|value| parse_range(value, len)) {
        None | Some(ParsedRange::Full) => {
            media_response(StatusCode::OK, len, None, body_for(0, len))
        }
        Some(ParsedRange::Satisfiable { start, end }) => media_response(
            StatusCode::PARTIAL_CONTENT,
            end - start,
            Some(format!("bytes {}-{}/{}", start, end - 1, len)),
            body_for(start, end),
        ),
        Some(ParsedRange::Unsatisfiable) => media_response(
            StatusCode::RANGE_NOT_SATISFIABLE,
            0,
            Some(format!("bytes */{len}")),
            Body::empty(),
        ),
    }
}

/// Build a response with the headers every media reply shares.
///
/// `content_length` is passed explicitly because the response body is streamed
/// and so carries no size of its own.
fn media_response(
    status: StatusCode,
    content_length: u64,
    content_range: Option<String>,
    body: Body,
) -> Response {
    let mut builder = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, CONTENT_TYPE)
        .header(header::ACCEPT_RANGES, "bytes");
    if let Some(content_range) = content_range {
        builder = builder.header(header::CONTENT_RANGE, content_range);
    }
    builder
        .header(header::CONTENT_LENGTH, content_length)
        .body(body)
        .expect("media response is always valid")
}

/// Stream `[start, end)` from the source in [`CHUNK_SIZE`] pieces.
fn streaming_body(source: Arc<dyn MediaSource>, start: u64, end: u64) -> Body {
    let stream = async_stream::stream! {
        let mut offset = start;
        while offset < end {
            let chunk_len = (end - offset).min(CHUNK_SIZE as u64) as usize;
            let mut buf = vec![0u8; chunk_len];
            match source.read_at(offset, &mut buf).await {
                Ok(0) => break,
                Ok(read) => {
                    buf.truncate(read);
                    yield Ok::<Bytes, io::Error>(Bytes::from(buf));
                    offset += read as u64;
                }
                Err(err) => {
                    yield Err(err);
                    break;
                }
            }
        }
    };
    Body::from_stream(stream)
}

/// A fresh unguessable token for one server launch.
fn random_token() -> String {
    hex::encode(rand::random::<[u8; 32]>())
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use tempfile::NamedTempFile;

    use super::*;

    /// Bytes that differ at most offsets, so a wrong range is visible.
    fn sample(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    /// A server over a temporary file, keeping the file alive for the test.
    async fn local_server(len: usize) -> (MediaServer, Vec<u8>, NamedTempFile) {
        let mut file = NamedTempFile::new().unwrap();
        let bytes = sample(len);
        file.write_all(&bytes).unwrap();
        file.flush().unwrap();
        let server = MediaServer::serve_file(file.path()).await.unwrap();
        (server, bytes, file)
    }

    fn client() -> reqwest::Client {
        reqwest::Client::new()
    }

    #[test]
    fn single_ranges_are_resolved_against_the_source_length() {
        assert_eq!(
            parse_range("bytes=0-99", 1000),
            ParsedRange::Satisfiable { start: 0, end: 100 }
        );
        assert_eq!(
            parse_range("bytes=100-", 1000),
            ParsedRange::Satisfiable {
                start: 100,
                end: 1000
            }
        );
        assert_eq!(
            parse_range("bytes=990-5000", 1000),
            ParsedRange::Satisfiable {
                start: 990,
                end: 1000
            }
        );
    }

    #[test]
    fn suffix_ranges_count_back_from_the_end() {
        assert_eq!(
            parse_range("bytes=-50", 1000),
            ParsedRange::Satisfiable {
                start: 950,
                end: 1000
            }
        );
        assert_eq!(
            parse_range("bytes=-5000", 1000),
            ParsedRange::Satisfiable {
                start: 0,
                end: 1000
            }
        );
    }

    #[test]
    fn unsatisfiable_ranges_are_reported_as_such() {
        assert_eq!(parse_range("bytes=1000-", 1000), ParsedRange::Unsatisfiable);
        assert_eq!(parse_range("bytes=-0", 1000), ParsedRange::Unsatisfiable);
        assert_eq!(parse_range("bytes=0-", 0), ParsedRange::Unsatisfiable);
    }

    #[test]
    fn malformed_or_unsupported_ranges_are_ignored() {
        assert_eq!(parse_range("bytes=5-2", 1000), ParsedRange::Full);
        assert_eq!(parse_range("bytes=0-1,3-4", 1000), ParsedRange::Full);
        assert_eq!(parse_range("items=0-1", 1000), ParsedRange::Full);
        assert_eq!(parse_range("bytes=abc", 1000), ParsedRange::Full);
        assert_eq!(parse_range("bytes=-", 1000), ParsedRange::Full);
    }

    #[tokio::test]
    async fn binds_loopback_on_an_ephemeral_port_with_a_tokenised_url() {
        let (server, _bytes, _file) = local_server(1024).await;

        let addr = server.local_addr();
        assert!(addr.ip().is_loopback(), "not a loopback address: {addr}");
        assert_ne!(addr.port(), 0, "port was not resolved");

        let url = server.url();
        assert!(url.starts_with("http://127.0.0.1:"), "{url}");
        assert!(url.ends_with(server.token()), "{url}");
        assert!(!server.token().is_empty());
    }

    #[tokio::test]
    async fn requests_without_the_token_are_rejected() {
        let (server, _bytes, _file) = local_server(1024).await;
        let client = client();

        let wrong = server.url().replace(server.token(), "not-the-token");
        let response = client.get(wrong).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let bare = format!("http://{}/media", server.local_addr());
        let response = client.get(bare).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn head_reports_the_length_and_accepts_ranges() {
        let (server, bytes, _file) = local_server(200 * 1024).await;

        let response = client().head(server.url()).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[reqwest::header::ACCEPT_RANGES], "bytes");
        assert_eq!(
            response.headers()[reqwest::header::CONTENT_LENGTH],
            bytes.len().to_string()
        );
        assert!(response.bytes().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_full_get_returns_the_whole_file() {
        let (server, bytes, _file) = local_server(200 * 1024).await;

        let response = client().get(server.url()).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[reqwest::header::ACCEPT_RANGES], "bytes");
        assert_eq!(
            response.headers()[reqwest::header::CONTENT_LENGTH],
            bytes.len().to_string()
        );
        assert_eq!(response.bytes().await.unwrap().as_ref(), &bytes[..]);
    }

    #[tokio::test]
    async fn a_range_request_returns_206_with_content_range_and_body() {
        let (server, bytes, _file) = local_server(200 * 1024).await;

        let response = client()
            .get(server.url())
            .header(reqwest::header::RANGE, "bytes=10-19")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(
            response.headers()[reqwest::header::CONTENT_RANGE],
            format!("bytes 10-19/{}", bytes.len())
        );
        assert_eq!(response.headers()[reqwest::header::CONTENT_LENGTH], "10");
        assert_eq!(response.bytes().await.unwrap().as_ref(), &bytes[10..20]);
    }

    #[tokio::test]
    async fn an_open_ended_range_runs_to_the_end() {
        let (server, bytes, _file) = local_server(4096).await;
        let start = 4000;

        let response = client()
            .get(server.url())
            .header(reqwest::header::RANGE, format!("bytes={start}-"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(
            response.headers()[reqwest::header::CONTENT_RANGE],
            format!("bytes {start}-{}/{}", bytes.len() - 1, bytes.len())
        );
        assert_eq!(response.bytes().await.unwrap().as_ref(), &bytes[start..]);
    }

    #[tokio::test]
    async fn a_suffix_range_returns_the_tail() {
        let (server, bytes, _file) = local_server(4096).await;

        let response = client()
            .get(server.url())
            .header(reqwest::header::RANGE, "bytes=-50")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(
            response.bytes().await.unwrap().as_ref(),
            &bytes[bytes.len() - 50..]
        );
    }

    #[tokio::test]
    async fn an_unsatisfiable_range_returns_416() {
        let (server, bytes, _file) = local_server(4096).await;

        let response = client()
            .get(server.url())
            .header(reqwest::header::RANGE, format!("bytes={}-", bytes.len()))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::RANGE_NOT_SATISFIABLE);
        assert_eq!(response.headers()[reqwest::header::ACCEPT_RANGES], "bytes");
        assert_eq!(
            response.headers()[reqwest::header::CONTENT_RANGE],
            format!("bytes */{}", bytes.len())
        );
        assert_eq!(response.headers()[reqwest::header::CONTENT_LENGTH], "0");
    }

    #[tokio::test]
    async fn playback_and_seeking_read_the_correct_ranges() {
        let (server, bytes, _file) = local_server(4 * CHUNK_SIZE).await;
        let client = client();

        // Playback: two sequential chunks.
        for start in [0, CHUNK_SIZE] {
            let end = start + CHUNK_SIZE;
            let response = client
                .get(server.url())
                .header(reqwest::header::RANGE, format!("bytes={start}-{}", end - 1))
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
            assert_eq!(response.bytes().await.unwrap().as_ref(), &bytes[start..end]);
        }

        // Seeking: jump near the end and read out of order.
        let start = bytes.len() - 1234;
        let response = client
            .get(server.url())
            .header(reqwest::header::RANGE, format!("bytes={start}-"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(response.bytes().await.unwrap().as_ref(), &bytes[start..]);
    }

    #[tokio::test]
    async fn bodies_are_read_from_the_source_in_bounded_chunks() {
        #[derive(Default)]
        struct CountingSource {
            data: Vec<u8>,
            largest_read: AtomicUsize,
        }

        #[async_trait]
        impl MediaSource for CountingSource {
            fn len(&self) -> u64 {
                self.data.len() as u64
            }

            async fn read_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<usize> {
                self.largest_read.fetch_max(buf.len(), Ordering::SeqCst);
                let start = (offset as usize).min(self.data.len());
                let end = (start + buf.len()).min(self.data.len());
                let read = end - start;
                buf[..read].copy_from_slice(&self.data[start..end]);
                Ok(read)
            }
        }

        let source = Arc::new(CountingSource {
            data: sample(3 * CHUNK_SIZE + 7),
            largest_read: AtomicUsize::new(0),
        });
        let server = MediaServer::bind(source.clone()).await.unwrap();

        let response = client().get(server.url()).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.bytes().await.unwrap();
        assert_eq!(body.as_ref(), &source.data[..]);
        assert!(
            source.largest_read.load(Ordering::SeqCst) <= CHUNK_SIZE,
            "server read more than one chunk at a time"
        );
    }
}
