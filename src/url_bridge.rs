//! Opt-in sign-in URL bridge for `devcage mcp`.
//!
//! The cage does not receive the display socket, the `DBus` session bus, or
//! `/run/user`. A read-only `xdg-open` / `open` shim writes one URL per line to
//! a FIFO the cage cannot read. The host checks the URL and, when asked, starts
//! the host browser with that single argument.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

/// Directory created inside the cage for the shim and the FIFO.
pub const GUEST_DIR: &str = "/devcage";
/// Read-only directory that holds the `xdg-open` and `open` shims.
pub const GUEST_SHIM_DIR: &str = "/devcage/bin";
/// Write-only FIFO path inside the cage.
pub const GUEST_FIFO: &str = "/devcage/url-bridge";
/// `BROWSER` value inside a `bwrap` cage.
pub const GUEST_BROWSER: &str = "/devcage/bin/xdg-open";

/// Desktop session variables removed while the bridge is on.
///
/// The stock `xdg-open` script (including the copy bundled with the `open`
/// npm package) tries the desktop bus before `$BROWSER` when these are set.
/// That call hangs without the display socket, and the connector still reports
/// that a browser opened. Clearing them makes the script exec `$BROWSER`.
pub const DESKTOP_SESSION_VARS: &[&str] = &[
    "DISPLAY",
    "WAYLAND_DISPLAY",
    "DBUS_SESSION_BUS_ADDRESS",
    "XDG_CURRENT_DESKTOP",
    "XDG_SESSION_TYPE",
    "XDG_RUNTIME_DIR",
    "DESKTOP_SESSION",
    "GNOME_DESKTOP_SESSION_ID",
    "KDE_FULL_SESSION",
    "KDE_SESSION_VERSION",
];

/// Longest accepted URL. One writer, so the extra newline stays one record.
pub const MAX_URL_LEN: usize = 4096;
/// Minimum time between accepted URLs.
pub const MIN_OPEN_GAP: Duration = Duration::from_secs(3);
/// Accepted URLs in one `devcage mcp` process.
pub const MAX_OPENS_PER_SESSION: u32 = 8;

const POLL_WAIT: Duration = Duration::from_millis(20);
/// `O_NONBLOCK` on Linux (`04000`).
#[cfg(target_os = "linux")]
const O_NONBLOCK: i32 = 0x800;
/// `O_NONBLOCK` on macOS.
#[cfg(target_os = "macos")]
const O_NONBLOCK: i32 = 0x4;
/// Other Unix hosts get a blocking FIFO. The reader still stops after the child exits.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
const O_NONBLOCK: i32 = 0;

const SHIM_NAMES: [&str; 2] = ["xdg-open", "open"];

const SHIM_SCRIPT: &str = "#!/usr/bin/sh
# Forwards one URL to the host. Does not open a browser.
bridge=${DEVCAGE_URL_BRIDGE:-/devcage/url-bridge}
if [ \"$#\" -lt 1 ] || [ -z \"$1\" ]; then
  exit 0
fi
printf '%s\\n' \"$1\" >> \"$bridge\" 2>/dev/null || true
exit 0
";

/// What `--open-urls` asked the host to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenUrls {
    /// Do not install a shim or a channel.
    Off,
    /// Print the link and open it when a graphical session exists.
    Open,
    /// Print the link. Do not start a browser.
    PrintOnly,
}

impl OpenUrls {
    /// Parse the CLI value. `None` means the flag was absent.
    ///
    /// # Errors
    ///
    /// The value is not `open` or `print`.
    pub fn from_cli(raw: Option<&str>) -> anyhow::Result<Self> {
        match raw {
            None => Ok(Self::Off),
            Some("open") => Ok(Self::Open),
            Some("print") => Ok(Self::PrintOnly),
            Some(other) => {
                anyhow::bail!("--open-urls must be `open` or `print` (got `{other}`)")
            },
        }
    }

    /// True when the shim and the FIFO should be installed.
    #[must_use]
    pub const fn is_enabled(self) -> bool {
        !matches!(self, Self::Off)
    }

    /// Label for `--print`.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Open => "open",
            Self::PrintOnly => "print",
        }
    }
}

/// Why a line from the cage was not opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignInReject {
    /// Empty line.
    Empty,
    /// Longer than [`MAX_URL_LEN`].
    TooLong,
    /// Whitespace, control characters, or bytes that are not UTF-8.
    InvalidChars,
    /// Scheme is not `https` or `http`.
    Scheme,
    /// `http` whose host is not loopback.
    HttpHost,
    /// A second URL arrived before [`MIN_OPEN_GAP`].
    RateLimit,
    /// [`MAX_OPENS_PER_SESSION`] already accepted.
    SessionLimit,
}

impl SignInReject {
    /// Short text for the stderr line.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::TooLong => "too long",
            Self::InvalidChars => "invalid characters",
            Self::Scheme => "scheme not allowed",
            Self::HttpHost => "http only allowed for localhost",
            Self::RateLimit => "rate limit",
            Self::SessionLimit => "session limit",
        }
    }
}

/// Host decision for one line from the cage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignInDecision {
    /// URL passed validation and the rate limit.
    Allow {
        /// The URL, unchanged.
        url: String,
        /// True when the host should start a browser.
        open_browser: bool,
    },
    /// URL was dropped. No browser is started.
    Refuse(SignInReject),
}

/// One line after validation, including the connector name for the log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignInEvent {
    /// Basename of the connector command.
    pub connector: String,
    /// Validation result.
    pub decision: SignInDecision,
}

/// Paths the child process should see.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChildBridgePaths {
    /// Directory prepended to `PATH`.
    pub path_prefix: String,
    /// `BROWSER` value.
    pub browser: String,
    /// `DEVCAGE_URL_BRIDGE` value.
    pub fifo: String,
}

/// Gap and session cap for accepted URLs.
#[derive(Debug, Clone, Copy, Default)]
pub struct BridgeLimiter {
    last: Option<Instant>,
    count: u32,
}

impl BridgeLimiter {
    /// Record `now` when the gap and the session cap allow another URL.
    ///
    /// # Errors
    ///
    /// [`SignInReject::RateLimit`] or [`SignInReject::SessionLimit`].
    pub fn try_acquire(&mut self, now: Instant) -> Result<(), SignInReject> {
        if self.count >= MAX_OPENS_PER_SESSION {
            return Err(SignInReject::SessionLimit);
        }
        if let Some(last) = self.last {
            if now.saturating_duration_since(last) < MIN_OPEN_GAP {
                return Err(SignInReject::RateLimit);
            }
        }
        self.count = self.count.saturating_add(1);
        self.last = Some(now);
        Ok(())
    }
}

/// Basename of the connector program, for the stderr line.
#[must_use]
pub fn connector_label(command: &[String]) -> String {
    let Some(prog) = command.first() else {
        return "connector".to_owned();
    };
    Path::new(prog)
        .file_name()
        .and_then(|name| name.to_str())
        .map_or_else(|| prog.clone(), ToOwned::to_owned)
}

/// Accept `https` URLs, and `http` only for `localhost`, `127.0.0.1`, and `::1`.
///
/// # Errors
///
/// The URL is empty, too long, contains whitespace or control characters, or
/// uses a scheme or host this bridge does not open.
pub fn validate_sign_in_url(raw: &str) -> Result<(), SignInReject> {
    if raw.is_empty() {
        return Err(SignInReject::Empty);
    }
    if raw.len() > MAX_URL_LEN {
        return Err(SignInReject::TooLong);
    }
    if raw.chars().any(|ch| ch.is_control() || ch.is_whitespace()) {
        return Err(SignInReject::InvalidChars);
    }
    let Some((scheme, rest)) = split_scheme(raw) else {
        return Err(SignInReject::Scheme);
    };
    if scheme.eq_ignore_ascii_case("https") {
        if authority_host(rest).is_some() {
            Ok(())
        } else {
            Err(SignInReject::Scheme)
        }
    } else if scheme.eq_ignore_ascii_case("http") {
        if authority_host(rest).is_some_and(is_loopback_host) {
            Ok(())
        } else {
            Err(SignInReject::HttpHost)
        }
    } else {
        Err(SignInReject::Scheme)
    }
}

/// Validate, then apply the rate limit. `mode` chooses whether an allow opens a browser.
#[must_use]
pub fn decide_sign_in(
    raw: &str,
    limiter: &mut BridgeLimiter,
    now: Instant,
    mode: OpenUrls,
) -> SignInDecision {
    if let Err(reason) = validate_sign_in_url(raw) {
        return SignInDecision::Refuse(reason);
    }
    if let Err(reason) = limiter.try_acquire(now) {
        return SignInDecision::Refuse(reason);
    }
    SignInDecision::Allow {
        url: raw.to_owned(),
        open_browser: matches!(mode, OpenUrls::Open),
    }
}

/// Stderr line for an accepted URL. Printed even when the host browser starts.
#[must_use]
pub fn sign_in_log_line(connector: &str, url: &str) -> String {
    format!("devcage: sign-in link for {connector}: {url}")
}

/// Stderr line for a rejected URL. The raw text is omitted.
#[must_use]
pub fn refusal_log_line(reason: SignInReject) -> String {
    format!("devcage: refused sign-in link ({})", reason.as_str())
}

/// Put `entry` first on `PATH`.
#[must_use]
pub fn prepend_path_entry(entry: &str, existing: &str) -> String {
    if existing.is_empty() {
        entry.to_owned()
    } else {
        format!("{entry}:{existing}")
    }
}

/// Live FIFO reader. Drop joins the reader before the write end closes.
#[derive(Debug)]
pub struct UrlBridge {
    stop: Arc<AtomicBool>,
    reader: Option<JoinHandle<()>>,
    shim_dir: PathBuf,
    fifo: PathBuf,
    /// Held so the FIFO stays open while the reader polls.
    _write: File,
}

impl Drop for UrlBridge {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.reader.take() {
            let _ = handle.join();
        }
    }
}

impl UrlBridge {
    /// Host directory that contains the shims.
    #[must_use]
    pub fn shim_dir(&self) -> &Path {
        &self.shim_dir
    }

    /// Host FIFO path.
    #[must_use]
    pub fn fifo(&self) -> &Path {
        &self.fifo
    }

    /// Paths to export into the child.
    ///
    /// `guest` is true for `bwrap`, where the shim and FIFO are mounted at
    /// [`GUEST_SHIM_DIR`] and [`GUEST_FIFO`]. Seatbelt and env-only see the host paths.
    #[must_use]
    pub fn child_paths(&self, guest: bool) -> ChildBridgePaths {
        if guest {
            ChildBridgePaths {
                path_prefix: GUEST_SHIM_DIR.to_owned(),
                browser: GUEST_BROWSER.to_owned(),
                fifo: GUEST_FIFO.to_owned(),
            }
        } else {
            ChildBridgePaths {
                path_prefix: self.shim_dir.display().to_string(),
                browser: self.shim_dir.join("xdg-open").display().to_string(),
                fifo: self.fifo.display().to_string(),
            }
        }
    }
}

/// Install the shim, open the FIFO, and read URLs until the bridge is dropped.
///
/// The handler runs on the reader thread. It must not block for long.
///
/// # Errors
///
/// `mkfifo` is missing, or the shim directory or FIFO cannot be created.
pub fn start_url_bridge(
    session_dir: &Path,
    connector: &str,
    mode: OpenUrls,
    handler: Box<dyn Fn(SignInEvent) + Send>,
) -> anyhow::Result<UrlBridge> {
    let root = session_dir.join("url-bridge");
    let shim_dir = root.join("bin");
    let fifo = root.join("url.fifo");
    fs::create_dir_all(&shim_dir)?;
    install_shims(&shim_dir)?;
    make_fifo(&fifo)?;
    let read = open_fifo(&fifo, true)?;
    let write = open_fifo(&fifo, false)?;
    fs::set_permissions(&fifo, fs::Permissions::from_mode(0o200))?;

    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = Arc::clone(&stop);
    let connector = connector.to_owned();
    let reader = thread::Builder::new()
        .name("devcage-url-bridge".to_owned())
        .spawn(move || read_loop(read, &stop_thread, &connector, mode, &handler))
        .map_err(|err| anyhow::anyhow!("failed to start the url bridge reader: {err}"))?;

    Ok(UrlBridge {
        stop,
        reader: Some(reader),
        shim_dir,
        fifo,
        _write: write,
    })
}

/// Same as [`start_url_bridge`], logging to stderr and opening the host browser when allowed.
///
/// # Errors
///
/// The FIFO or the shim directory cannot be created.
pub fn start_sign_in_bridge(
    session_dir: &Path,
    connector: &str,
    mode: OpenUrls,
) -> anyhow::Result<UrlBridge> {
    start_url_bridge(session_dir, connector, mode, Box::new(dispatch_sign_in))
}

fn dispatch_sign_in(event: SignInEvent) {
    match event.decision {
        SignInDecision::Refuse(reason) => {
            eprintln!("{}", refusal_log_line(reason));
        },
        SignInDecision::Allow { url, open_browser } => {
            eprintln!("{}", sign_in_log_line(&event.connector, &url));
            if should_spawn(
                open_browser,
                graphical_session(),
                browser_program().is_some(),
            ) {
                spawn_host_open(&url);
            }
        },
    }
}

const fn should_spawn(open_browser: bool, graphical: bool, have_program: bool) -> bool {
    open_browser && graphical && have_program
}

const fn browser_program() -> Option<&'static str> {
    if cfg!(target_os = "macos") {
        Some("open")
    } else if cfg!(target_os = "linux") {
        Some("xdg-open")
    } else {
        None
    }
}

fn graphical_session() -> bool {
    if cfg!(target_os = "macos") {
        return crate::util::which("open");
    }
    std::env::var_os("DISPLAY").is_some() || std::env::var_os("WAYLAND_DISPLAY").is_some()
}

/// Start the host opener with `url` as one argument. Never uses a shell.
fn spawn_host_open(url: &str) {
    let Some(program) = browser_program() else {
        return;
    };
    if !crate::util::which(program) {
        return;
    }
    let program = program.to_owned();
    let url = url.to_owned();
    let _ = thread::Builder::new()
        .name("devcage-open-url".to_owned())
        .spawn(move || {
            let _ = Command::new(program)
                .arg(url)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        });
}

fn install_shims(dir: &Path) -> anyhow::Result<()> {
    for name in SHIM_NAMES {
        let path = dir.join(name);
        fs::write(&path, SHIM_SCRIPT)?;
        let mut perms = fs::metadata(&path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&path, perms)?;
    }
    Ok(())
}

fn make_fifo(path: &Path) -> anyhow::Result<()> {
    let status = Command::new("mkfifo")
        .arg(path)
        .status()
        .map_err(|err| anyhow::anyhow!("mkfifo is required for --open-urls ({err})"))?;
    if !status.success() {
        anyhow::bail!("mkfifo {} failed", path.display());
    }
    Ok(())
}

fn open_fifo(path: &Path, read: bool) -> anyhow::Result<File> {
    let mut options = OpenOptions::new();
    if read {
        let _ = options.read(true);
    } else {
        let _ = options.write(true);
    }
    options.custom_flags(O_NONBLOCK);
    options
        .open(path)
        .map_err(|err| anyhow::anyhow!("open {}: {err}", path.display()))
}

fn read_loop(
    mut read: File,
    stop: &AtomicBool,
    connector: &str,
    mode: OpenUrls,
    handler: &dyn Fn(SignInEvent),
) {
    let mut pump = Pump {
        pending: Vec::new(),
        discard: false,
        limiter: BridgeLimiter::default(),
        connector,
        mode,
        handler,
    };
    let mut buf = [0_u8; 1024];
    while !stop.load(Ordering::Relaxed) {
        match read.read(&mut buf) {
            Ok(0) => thread::sleep(POLL_WAIT),
            Ok(n) => {
                if let Some(chunk) = buf.get(..n) {
                    pump.push(chunk);
                }
            },
            Err(err) if is_wait(&err) => thread::sleep(POLL_WAIT),
            Err(_) => break,
        }
    }
    // The child can exit in the gap after a write and before the next poll.
    // Read what is already buffered before dropping the write end.
    drain_ready(&mut read, &mut buf, &mut pump);
}

fn drain_ready(read: &mut File, buf: &mut [u8], pump: &mut Pump<'_>) {
    while let Ok(n) = read.read(buf) {
        if n == 0 {
            break;
        }
        if let Some(chunk) = buf.get(..n) {
            pump.push(chunk);
        }
    }
}

struct Pump<'a> {
    pending: Vec<u8>,
    discard: bool,
    limiter: BridgeLimiter,
    connector: &'a str,
    mode: OpenUrls,
    handler: &'a dyn Fn(SignInEvent),
}

impl Pump<'_> {
    fn push(&mut self, chunk: &[u8]) {
        for &byte in chunk {
            self.push_byte(byte);
        }
    }

    fn push_byte(&mut self, byte: u8) {
        if self.discard {
            if byte == b'\n' {
                self.discard = false;
            }
            return;
        }
        if byte == b'\n' {
            if self.pending.last() == Some(&b'\r') {
                self.pending.pop();
            }
            self.emit_pending();
            return;
        }
        self.pending.push(byte);
        if self.pending.len() > MAX_URL_LEN {
            self.pending.clear();
            self.discard = true;
            self.emit(SignInDecision::Refuse(SignInReject::TooLong));
        }
    }

    fn emit_pending(&mut self) {
        let line = std::mem::take(&mut self.pending);
        let decision = match std::str::from_utf8(&line) {
            Ok(text) => decide_sign_in(text, &mut self.limiter, Instant::now(), self.mode),
            Err(_) => SignInDecision::Refuse(SignInReject::InvalidChars),
        };
        self.emit(decision);
    }

    fn emit(&self, decision: SignInDecision) {
        (self.handler)(SignInEvent {
            connector: self.connector.to_owned(),
            decision,
        });
    }
}

fn is_wait(err: &io::Error) -> bool {
    err.kind() == io::ErrorKind::Interrupted || matches!(err.raw_os_error(), Some(11 | 35))
}

fn split_scheme(url: &str) -> Option<(&str, &str)> {
    let colon = url.find(':')?;
    let scheme = url.get(..colon)?;
    let rest = url.get(colon.saturating_add(1)..)?;
    if scheme.is_empty() || !scheme.bytes().all(|byte| byte.is_ascii_alphabetic()) {
        return None;
    }
    Some((scheme, rest))
}

fn authority_host(rest: &str) -> Option<&str> {
    let after = rest.strip_prefix("//")?;
    let authority = after.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty() {
        return None;
    }
    let hostport = authority
        .rsplit_once('@')
        .map_or(authority, |(_, hostport)| hostport);
    if hostport.is_empty() {
        return None;
    }
    if let Some(v6) = hostport.strip_prefix('[') {
        let (addr, after_bracket) = v6.split_once(']')?;
        if addr.is_empty() || !(after_bracket.is_empty() || after_bracket.starts_with(':')) {
            return None;
        }
        return Some(addr);
    }
    let host = hostport.split_once(':').map_or(hostport, |(host, _)| host);
    if host.is_empty() {
        None
    } else {
        Some(host)
    }
}

fn is_loopback_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1" || host == "::1"
}

#[cfg(test)]
mod tests {
    use super::{
        authority_host, connector_label, decide_sign_in, is_loopback_host, prepend_path_entry,
        refusal_log_line, should_spawn, sign_in_log_line, split_scheme, validate_sign_in_url,
        BridgeLimiter, OpenUrls, SignInDecision, SignInReject, GUEST_FIFO, MAX_OPENS_PER_SESSION,
        MAX_URL_LEN, MIN_OPEN_GAP, SHIM_SCRIPT,
    };
    use std::time::{Duration, Instant};

    #[test]
    fn validator_accepts_https_and_loopback_http() {
        for url in [
            "https://auth.atlassian.com/authorize?x=1",
            "https://example.com",
            "HTTPS://example.com/a",
            "http://localhost/cb",
            "http://localhost:39570/cb",
            "http://LOCALHOST/cb",
            "http://127.0.0.1/cb",
            "http://127.0.0.1:39570/cb",
            "http://user:pass@127.0.0.1/cb",
            "http://[::1]/cb",
            "http://[::1]:8080/cb",
        ] {
            assert_eq!(validate_sign_in_url(url), Ok(()), "{url}");
        }
    }

    #[test]
    fn validator_rejects_schemes_hosts_and_length() {
        let cases = [
            ("", SignInReject::Empty),
            ("   ", SignInReject::InvalidChars),
            ("https://example.com/a b", SignInReject::InvalidChars),
            ("https://example.com/\n", SignInReject::InvalidChars),
            ("notaurl", SignInReject::Scheme),
            ("file:///etc/passwd", SignInReject::Scheme),
            ("javascript:alert(1)", SignInReject::Scheme),
            ("data:text/html,hi", SignInReject::Scheme),
            ("ftp://example.com/a", SignInReject::Scheme),
            ("https://", SignInReject::Scheme),
            ("https:///x", SignInReject::Scheme),
            ("http://example.com/cb", SignInReject::HttpHost),
            ("http://127.0.0.1.evil.com/cb", SignInReject::HttpHost),
            ("http://127.0.0.1.evil.com:80/", SignInReject::HttpHost),
            ("http://localhost.evil.com/cb", SignInReject::HttpHost),
            ("http://127.0.0.2/", SignInReject::HttpHost),
        ];
        for (url, reason) in cases {
            assert_eq!(validate_sign_in_url(url), Err(reason), "{url}");
        }

        let mut at_cap = "https://example.com/".to_owned();
        while at_cap.len() < MAX_URL_LEN {
            at_cap.push('a');
        }
        assert_eq!(validate_sign_in_url(&at_cap), Ok(()));
        at_cap.push('b');
        assert_eq!(validate_sign_in_url(&at_cap), Err(SignInReject::TooLong));
    }

    #[test]
    fn limiter_enforces_gap_and_session_cap() {
        let start = Instant::now();
        let mut limiter = BridgeLimiter::default();
        assert!(limiter.try_acquire(start).is_ok());
        assert_eq!(
            limiter.try_acquire(start + Duration::from_secs(2)),
            Err(SignInReject::RateLimit)
        );
        assert!(limiter.try_acquire(start + MIN_OPEN_GAP).is_ok());

        let mut capped = BridgeLimiter::default();
        for step in 0..MAX_OPENS_PER_SESSION {
            let now = start + MIN_OPEN_GAP * step;
            assert!(capped.try_acquire(now).is_ok());
        }
        let after = start + MIN_OPEN_GAP * MAX_OPENS_PER_SESSION;
        assert_eq!(capped.try_acquire(after), Err(SignInReject::SessionLimit));
    }

    #[test]
    fn decide_does_not_open_or_consume_a_slot_for_a_bad_url() {
        let now = Instant::now();
        let mut limiter = BridgeLimiter::default();
        assert_eq!(
            decide_sign_in("file:///etc/passwd", &mut limiter, now, OpenUrls::Open),
            SignInDecision::Refuse(SignInReject::Scheme)
        );
        assert_eq!(
            decide_sign_in(
                "https://example.com/a",
                &mut limiter,
                now,
                OpenUrls::PrintOnly
            ),
            SignInDecision::Allow {
                url: "https://example.com/a".to_owned(),
                open_browser: false,
            }
        );
        assert_eq!(
            decide_sign_in(
                "https://example.com/b",
                &mut limiter,
                now + MIN_OPEN_GAP,
                OpenUrls::Open
            ),
            SignInDecision::Allow {
                url: "https://example.com/b".to_owned(),
                open_browser: true,
            }
        );
    }

    #[test]
    fn log_lines_name_the_connector_and_hide_rejected_text() {
        assert_eq!(
            sign_in_log_line("npx", "https://example.com/a"),
            "devcage: sign-in link for npx: https://example.com/a"
        );
        assert_eq!(
            refusal_log_line(SignInReject::Scheme),
            "devcage: refused sign-in link (scheme not allowed)"
        );
        assert!(!refusal_log_line(SignInReject::Scheme).contains("file:"));
    }

    #[test]
    fn spawn_policy_requires_open_mode_a_session_and_a_program() {
        assert!(should_spawn(true, true, true));
        assert!(!should_spawn(false, true, true));
        assert!(!should_spawn(true, false, true));
        assert!(!should_spawn(true, true, false));
    }

    #[test]
    fn shim_script_only_writes_the_url() {
        assert!(SHIM_SCRIPT.contains("printf"));
        assert!(SHIM_SCRIPT.contains(GUEST_FIFO));
        assert!(SHIM_SCRIPT.contains("exit 0"));
        assert!(!SHIM_SCRIPT.contains("eval"));
        assert!(!SHIM_SCRIPT.contains("xdg-open"));
        assert_eq!(connector_label(&["/usr/bin/npx".into()]), "npx");
        assert_eq!(connector_label(&[]), "connector");
        assert_eq!(
            prepend_path_entry("/devcage/bin", "/usr/bin"),
            "/devcage/bin:/usr/bin"
        );
        assert_eq!(prepend_path_entry("/devcage/bin", ""), "/devcage/bin");
        assert_eq!(
            split_scheme("https://example.com").map(|pair| pair.0),
            Some("https")
        );
        assert!(is_loopback_host("localhost"));
        assert_eq!(authority_host("//127.0.0.1:9/cb"), Some("127.0.0.1"));
        assert_eq!(OpenUrls::from_cli(None).expect("off"), OpenUrls::Off);
        assert_eq!(
            OpenUrls::from_cli(Some("open")).expect("open"),
            OpenUrls::Open
        );
        assert_eq!(
            OpenUrls::from_cli(Some("print")).expect("print"),
            OpenUrls::PrintOnly
        );
        assert!(OpenUrls::from_cli(Some("browser")).is_err());
        assert!(OpenUrls::Open.is_enabled());
        assert!(!OpenUrls::Off.is_enabled());
        assert_eq!(OpenUrls::PrintOnly.label(), "print");
        assert!(super::DESKTOP_SESSION_VARS.contains(&"DISPLAY"));
        assert!(super::DESKTOP_SESSION_VARS.contains(&"WAYLAND_DISPLAY"));
        assert!(super::DESKTOP_SESSION_VARS.contains(&"DBUS_SESSION_BUS_ADDRESS"));
        assert!(super::DESKTOP_SESSION_VARS.contains(&"XDG_CURRENT_DESKTOP"));
        assert!(!super::DESKTOP_SESSION_VARS.contains(&"HOME"));
        assert!(!super::DESKTOP_SESSION_VARS.contains(&"PATH"));
    }

    #[cfg(unix)]
    #[test]
    fn fifo_is_write_only_and_delivers_one_url_per_line() {
        use super::{start_url_bridge, GUEST_BROWSER, GUEST_FIFO, GUEST_SHIM_DIR};
        use std::fs::{self, File, OpenOptions};
        use std::io::Write;
        use std::sync::mpsc;

        let (tx, rx) = mpsc::channel();
        let dir = bridge_temp("fifo");
        let bridge = start_url_bridge(
            &dir,
            "npx",
            OpenUrls::Open,
            Box::new(move |event| {
                let _ = tx.send(event);
            }),
        )
        .expect("bridge");

        assert!(
            File::open(bridge.fifo()).is_err(),
            "fifo must not be readable"
        );
        let guest = bridge.child_paths(true);
        assert_eq!(guest.path_prefix, GUEST_SHIM_DIR);
        assert_eq!(guest.browser, GUEST_BROWSER);
        assert_eq!(guest.fifo, GUEST_FIFO);
        let host = bridge.child_paths(false);
        assert!(host.browser.ends_with("xdg-open"));
        assert_eq!(host.fifo, bridge.fifo().display().to_string());

        let script = fs::read_to_string(bridge.shim_dir().join("xdg-open")).expect("shim");
        assert_eq!(
            script,
            fs::read_to_string(bridge.shim_dir().join("open")).expect("open shim")
        );
        assert!(script.contains("printf"));
        assert!(!script.contains("eval"));

        let mut writer = OpenOptions::new()
            .write(true)
            .open(bridge.fifo())
            .expect("write fifo");
        writeln!(writer, "https://example.com/a").expect("https");
        writer.flush().expect("flush");
        let allowed = recv_event(&rx);
        assert_eq!(allowed.connector, "npx");
        assert_eq!(
            allowed.decision,
            SignInDecision::Allow {
                url: "https://example.com/a".to_owned(),
                open_browser: true,
            }
        );

        writeln!(writer, "file:///etc/passwd").expect("file");
        writer.flush().expect("flush");
        let refused = recv_event(&rx);
        assert_eq!(
            refused.decision,
            SignInDecision::Refuse(SignInReject::Scheme)
        );

        writeln!(writer, "javascript:alert(1)").expect("js");
        writer.flush().expect("flush");
        let scripted = recv_event(&rx);
        assert_eq!(
            scripted.decision,
            SignInDecision::Refuse(SignInReject::Scheme)
        );

        drop(writer);
        drop(bridge);
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn shutdown_drains_a_url_written_while_the_reader_sleeps() {
        use super::start_url_bridge;
        use std::fs;
        use std::io::Write;
        use std::sync::mpsc;
        use std::time::Duration;

        let (tx, rx) = mpsc::channel();
        let dir = bridge_temp("drain");
        let bridge = start_url_bridge(
            &dir,
            "npx",
            OpenUrls::PrintOnly,
            Box::new(move |event| {
                let _ = tx.send(event);
            }),
        )
        .expect("bridge");
        std::thread::sleep(Duration::from_millis(40));
        let mut writer = fs::OpenOptions::new()
            .write(true)
            .open(bridge.fifo())
            .expect("write");
        writeln!(writer, "https://example.com/fast").expect("url");
        writer.flush().expect("flush");
        drop(writer);
        drop(bridge);
        let event = rx.try_recv().expect("drained before shutdown returned");
        assert_eq!(
            event.decision,
            SignInDecision::Allow {
                url: "https://example.com/fast".to_owned(),
                open_browser: false,
            }
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    fn recv_event(rx: &std::sync::mpsc::Receiver<super::SignInEvent>) -> super::SignInEvent {
        rx.recv_timeout(Duration::from_secs(2)).expect("url")
    }

    #[cfg(unix)]
    fn bridge_temp(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "devcage-url-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("mkdir");
        dir
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_cage_forwards_https_and_hides_display_sockets() {
        use super::{start_url_bridge, GUEST_BROWSER};
        use std::fs;
        use std::sync::mpsc;
        use std::time::Duration;

        if !crate::util::which("bwrap") {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let dir = bridge_temp("bwrap");
        let home = dir.join("home");
        fs::create_dir_all(&home).expect("home");
        let bridge = start_url_bridge(
            &dir,
            "npx",
            OpenUrls::PrintOnly,
            Box::new(move |event| {
                let _ = tx.send(event);
            }),
        )
        .expect("bridge");

        let forwarded = run_bwrap_plan(
            &home,
            &bridge,
            vec![
                GUEST_BROWSER.to_owned(),
                "https://example.com/from-cage".to_owned(),
            ],
        );
        if skip_namespace(&forwarded) {
            drop(bridge);
            let _ = fs::remove_dir_all(&dir);
            return;
        }
        assert!(
            forwarded.status.success(),
            "shim failed: {}",
            String::from_utf8_lossy(&forwarded.stderr)
        );
        let event = rx.recv_timeout(Duration::from_secs(2)).expect("forwarded");
        assert_eq!(
            event.decision,
            SignInDecision::Allow {
                url: "https://example.com/from-cage".to_owned(),
                open_browser: false,
            }
        );

        let hidden = run_bwrap_plan(
            &home,
            &bridge,
            vec![
                "/bin/sh".to_owned(),
                "-c".to_owned(),
                "if ls /tmp/.X11-unix >/dev/null 2>&1 || ls /run/user >/dev/null 2>&1 || ls \"$HOME/.aws\" >/dev/null 2>&1; then exit 7; else exit 0; fi".to_owned(),
            ],
        );
        assert!(
            hidden.status.success(),
            "display or secrets were visible: {}",
            String::from_utf8_lossy(&hidden.stderr)
        );

        let bad = run_bwrap_plan(
            &home,
            &bridge,
            vec![GUEST_BROWSER.to_owned(), "data:text/html,hi".to_owned()],
        );
        assert!(
            bad.status.success(),
            "{}",
            String::from_utf8_lossy(&bad.stderr)
        );
        let refused = rx.recv_timeout(Duration::from_secs(2)).expect("refused");
        assert_eq!(
            refused.decision,
            SignInDecision::Refuse(SignInReject::Scheme)
        );

        drop(bridge);
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(target_os = "linux")]
    fn run_bwrap_plan(
        home: &std::path::Path,
        bridge: &super::UrlBridge,
        command: Vec<String>,
    ) -> std::process::Output {
        use crate::bwrap_plan::{mcp_bwrap_argv, McpBwrapPlan, McpUrlBridge};
        use std::process::Command;

        let argv = mcp_bwrap_argv(&McpBwrapPlan {
            ephemeral_home: home.to_path_buf(),
            workspace: None,
            ro_mounts: Vec::new(),
            url_bridge: Some(McpUrlBridge {
                shim_dir: bridge.shim_dir().to_path_buf(),
                fifo: bridge.fifo().to_path_buf(),
            }),
            net_none: false,
            command,
        })
        .expect("argv");
        let (prog, rest) = argv.split_first().expect("bwrap");
        Command::new(prog).args(rest).output().expect("spawn bwrap")
    }

    #[cfg(target_os = "linux")]
    fn skip_namespace(output: &std::process::Output) -> bool {
        if output.status.success() {
            return false;
        }
        let err = String::from_utf8_lossy(&output.stderr);
        let blocked = err.contains("namespace") || err.contains("uid map");
        if blocked {
            eprintln!("skip: {err}");
        }
        blocked
    }
}
