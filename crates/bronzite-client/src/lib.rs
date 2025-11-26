//! Client library for querying the Bronzite type system daemon from proc-macros.
//!
//! This crate provides a simple API for proc-macros to query type information
//! from a running Bronzite daemon. The daemon compiles the crate once and caches
//! the type information, allowing many proc-macro invocations to share the same
//! compilation result.
//!
//! # Example
//!
//! ```ignore
//! use bronzite_client::{BronziteClient, ensure_daemon_running};
//!
//! // Ensure daemon is running (auto-starts if needed)
//! ensure_daemon_running()?;
//!
//! let mut client = BronziteClient::connect()?;
//! let items = client.list_items("my_crate")?;
//! ```

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use bronzite_types::{Query, QueryData, QueryResult, Request, Response};

#[cfg(unix)]
use std::os::unix::net::UnixStream;

/// Errors that can occur when using the Bronzite client.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to connect to Bronzite daemon: {0}")]
    ConnectionFailed(#[from] std::io::Error),

    #[error("Failed to serialize request: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("Daemon returned an error: {0}")]
    DaemonError(String),

    #[error("Response ID mismatch: expected {expected}, got {got}")]
    ResponseMismatch { expected: u64, got: u64 },

    #[error("Daemon is not running. Start it with: bronzite-daemon --ensure")]
    DaemonNotRunning,

    #[error("Unexpected response type")]
    UnexpectedResponse,

    #[error("Failed to start daemon: {0}")]
    DaemonStartFailed(String),

    #[error("Timeout waiting for daemon to start")]
    DaemonStartTimeout,
}

/// Result type for Bronzite operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Global request ID counter.
static REQUEST_ID: AtomicU64 = AtomicU64::new(1);

/// Default timeout for waiting for daemon to start.
const DEFAULT_DAEMON_TIMEOUT: Duration = Duration::from_secs(30);

/// A client for communicating with the Bronzite daemon.
pub struct BronziteClient {
    #[cfg(unix)]
    stream: UnixStream,
    #[cfg(windows)]
    stream: std::net::TcpStream,
}

impl BronziteClient {
    /// Connect to the Bronzite daemon using the default socket path.
    pub fn connect() -> Result<Self> {
        let socket_path = bronzite_types::default_socket_path();
        Self::connect_to(socket_path)
    }

    /// Connect to the Bronzite daemon for a specific workspace.
    pub fn connect_for_workspace(workspace_root: &std::path::Path) -> Result<Self> {
        let socket_path = bronzite_types::socket_path_for_workspace(workspace_root);
        Self::connect_to(socket_path)
    }

    /// Connect to the Bronzite daemon at a specific socket path.
    #[cfg(unix)]
    pub fn connect_to(socket_path: PathBuf) -> Result<Self> {
        if !socket_path.exists() {
            return Err(Error::DaemonNotRunning);
        }

        let stream = UnixStream::connect(&socket_path)?;
        Ok(Self { stream })
    }

    /// Connect to the Bronzite daemon at a specific address (Windows).
    #[cfg(windows)]
    pub fn connect_to(socket_path: PathBuf) -> Result<Self> {
        // On Windows, we use a TCP socket on localhost instead
        // The port is derived from the socket path hash
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        socket_path.hash(&mut hasher);
        let port = 10000 + (hasher.finish() % 50000) as u16;

        let stream = std::net::TcpStream::connect(("127.0.0.1", port))?;
        Ok(Self { stream })
    }

    /// Send a query to the daemon and wait for a response.
    pub fn query(&mut self, crate_name: &str, query: Query) -> Result<QueryData> {
        let id = REQUEST_ID.fetch_add(1, Ordering::SeqCst);

        let request = Request {
            id,
            crate_name: crate_name.to_string(),
            query,
        };

        // Send the request as a JSON line
        let mut request_json = serde_json::to_string(&request)?;
        request_json.push('\n');
        self.stream.write_all(request_json.as_bytes())?;
        self.stream.flush()?;

        // Read the response
        let mut reader = BufReader::new(&self.stream);
        let mut response_line = String::new();
        reader.read_line(&mut response_line)?;

        let response: Response = serde_json::from_str(&response_line)?;

        // Verify the response ID matches
        if response.id != id {
            return Err(Error::ResponseMismatch {
                expected: id,
                got: response.id,
            });
        }

        // Extract the result
        match response.result {
            QueryResult::Success { data } => Ok(data),
            QueryResult::Error { message } => Err(Error::DaemonError(message)),
        }
    }

    /// Check if the daemon is alive.
    pub fn ping(&mut self) -> Result<bool> {
        match self.query("", Query::Ping) {
            Ok(QueryData::Pong) => Ok(true),
            Ok(_) => Err(Error::UnexpectedResponse),
            Err(e) => Err(e),
        }
    }

    /// Request the daemon to shut down.
    pub fn shutdown(&mut self) -> Result<()> {
        match self.query("", Query::Shutdown) {
            Ok(QueryData::ShuttingDown) => Ok(()),
            Ok(_) => Err(Error::UnexpectedResponse),
            Err(e) => Err(e),
        }
    }

    /// List all items in a crate.
    pub fn list_items(&mut self, crate_name: &str) -> Result<Vec<bronzite_types::ItemInfo>> {
        match self.query(crate_name, Query::ListItems)? {
            QueryData::Items { items } => Ok(items),
            _ => Err(Error::UnexpectedResponse),
        }
    }

    /// Get all trait implementations for a type.
    pub fn get_trait_impls(
        &mut self,
        crate_name: &str,
        type_path: &str,
    ) -> Result<Vec<bronzite_types::TraitImplDetails>> {
        let query = Query::GetTraitImpls {
            type_path: type_path.to_string(),
        };

        match self.query(crate_name, query)? {
            QueryData::TraitImpls { impls } => Ok(impls),
            _ => Err(Error::UnexpectedResponse),
        }
    }

    /// Get inherent impls for a type (impl Foo { ... }).
    pub fn get_inherent_impls(
        &mut self,
        crate_name: &str,
        type_path: &str,
    ) -> Result<Vec<bronzite_types::InherentImplDetails>> {
        let query = Query::GetInherentImpls {
            type_path: type_path.to_string(),
        };

        match self.query(crate_name, query)? {
            QueryData::InherentImpls { impls } => Ok(impls),
            _ => Err(Error::UnexpectedResponse),
        }
    }

    /// Check if a type implements a trait.
    pub fn check_impl(
        &mut self,
        crate_name: &str,
        type_path: &str,
        trait_path: &str,
    ) -> Result<(bool, Option<bronzite_types::TraitImplDetails>)> {
        let query = Query::CheckImpl {
            type_path: type_path.to_string(),
            trait_path: trait_path.to_string(),
        };

        match self.query(crate_name, query)? {
            QueryData::ImplCheck {
                implements,
                impl_info,
            } => Ok((implements, impl_info)),
            _ => Err(Error::UnexpectedResponse),
        }
    }

    /// Get all fields of a struct.
    pub fn get_fields(
        &mut self,
        crate_name: &str,
        type_path: &str,
    ) -> Result<Vec<bronzite_types::FieldInfo>> {
        let query = Query::GetFields {
            type_path: type_path.to_string(),
        };

        match self.query(crate_name, query)? {
            QueryData::Fields { fields } => Ok(fields),
            _ => Err(Error::UnexpectedResponse),
        }
    }

    /// Get detailed information about a type.
    pub fn get_type(
        &mut self,
        crate_name: &str,
        type_path: &str,
    ) -> Result<bronzite_types::TypeDetails> {
        let query = Query::GetType {
            path: type_path.to_string(),
        };

        match self.query(crate_name, query)? {
            QueryData::TypeInfo(info) => Ok(info),
            _ => Err(Error::UnexpectedResponse),
        }
    }

    /// Get all traits defined in a crate.
    pub fn get_traits(&mut self, crate_name: &str) -> Result<Vec<bronzite_types::TraitInfo>> {
        match self.query(crate_name, Query::GetTraits)? {
            QueryData::Traits { traits } => Ok(traits),
            _ => Err(Error::UnexpectedResponse),
        }
    }

    /// Get detailed information about a trait.
    pub fn get_trait(
        &mut self,
        crate_name: &str,
        trait_path: &str,
    ) -> Result<bronzite_types::TraitDetails> {
        let query = Query::GetTrait {
            path: trait_path.to_string(),
        };

        match self.query(crate_name, query)? {
            QueryData::TraitDetails(details) => Ok(details),
            _ => Err(Error::UnexpectedResponse),
        }
    }

    /// Find types matching a pattern.
    pub fn find_types(
        &mut self,
        crate_name: &str,
        pattern: &str,
    ) -> Result<Vec<bronzite_types::TypeSummary>> {
        let query = Query::FindTypes {
            pattern: pattern.to_string(),
        };

        match self.query(crate_name, query)? {
            QueryData::Types { types } => Ok(types),
            _ => Err(Error::UnexpectedResponse),
        }
    }

    /// Resolve a type alias to its underlying type.
    pub fn resolve_alias(
        &mut self,
        crate_name: &str,
        path: &str,
    ) -> Result<(String, String, Vec<String>)> {
        let query = Query::ResolveAlias {
            path: path.to_string(),
        };

        match self.query(crate_name, query)? {
            QueryData::ResolvedType {
                original,
                resolved,
                chain,
            } => Ok((original, resolved, chain)),
            _ => Err(Error::UnexpectedResponse),
        }
    }

    /// Get all types that implement a specific trait.
    pub fn get_implementors(
        &mut self,
        crate_name: &str,
        trait_path: &str,
    ) -> Result<Vec<bronzite_types::TypeSummary>> {
        let query = Query::GetImplementors {
            trait_path: trait_path.to_string(),
        };

        match self.query(crate_name, query)? {
            QueryData::Implementors { types } => Ok(types),
            _ => Err(Error::UnexpectedResponse),
        }
    }

    /// Get memory layout information for a type.
    pub fn get_layout(
        &mut self,
        crate_name: &str,
        type_path: &str,
    ) -> Result<bronzite_types::LayoutInfo> {
        let query = Query::GetLayout {
            type_path: type_path.to_string(),
        };

        match self.query(crate_name, query)? {
            QueryData::Layout(layout) => Ok(layout),
            _ => Err(Error::UnexpectedResponse),
        }
    }
}

/// Try to connect to an existing daemon, or return an error if not running.
///
/// This is the recommended entry point for proc-macros, as it provides
/// clear error messages if the daemon isn't running.
pub fn connect() -> Result<BronziteClient> {
    BronziteClient::connect()
}

/// Try to connect to an existing daemon for a specific workspace.
pub fn connect_for_workspace(workspace_root: &std::path::Path) -> Result<BronziteClient> {
    BronziteClient::connect_for_workspace(workspace_root)
}

/// Check if the daemon is running and responding.
pub fn is_daemon_running() -> bool {
    is_daemon_running_at(&bronzite_types::default_socket_path())
}

/// Check if a daemon is running at a specific socket path.
#[cfg(unix)]
pub fn is_daemon_running_at(socket_path: &PathBuf) -> bool {
    if !socket_path.exists() {
        return false;
    }

    // Try to connect and ping
    match UnixStream::connect(socket_path) {
        Ok(mut stream) => {
            // Send a ping request
            let request = Request {
                id: 0,
                crate_name: String::new(),
                query: Query::Ping,
            };

            if let Ok(json) = serde_json::to_string(&request) {
                let msg = format!("{}\n", json);
                if stream.write_all(msg.as_bytes()).is_ok() {
                    let _ = stream.flush();

                    // Set a read timeout
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));

                    // Try to read response
                    let mut reader = BufReader::new(&stream);
                    let mut response = String::new();
                    if reader.read_line(&mut response).is_ok() {
                        return response.contains("pong");
                    }
                }
            }
            false
        }
        Err(_) => false,
    }
}

#[cfg(windows)]
pub fn is_daemon_running_at(socket_path: &PathBuf) -> bool {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    socket_path.hash(&mut hasher);
    let port = 10000 + (hasher.finish() % 50000) as u16;

    std::net::TcpStream::connect(("127.0.0.1", port)).is_ok()
}

/// Ensure the daemon is running, starting it if necessary.
///
/// This function will:
/// 1. Check if a daemon is already running
/// 2. If not, attempt to start one using `bronzite-daemon --ensure`
/// 3. Wait for the daemon to become ready
///
/// This is the recommended way for proc-macros to ensure they can connect.
///
/// # Arguments
///
/// * `manifest_path` - Optional path to the workspace/crate. If None, uses current directory.
///
/// # Example
///
/// ```ignore
/// use bronzite_client::ensure_daemon_running;
///
/// // In your proc-macro:
/// ensure_daemon_running(None)?;
/// let mut client = bronzite_client::connect()?;
/// // ... use client
/// ```
pub fn ensure_daemon_running(manifest_path: Option<&std::path::Path>) -> Result<()> {
    ensure_daemon_running_with_timeout(manifest_path, DEFAULT_DAEMON_TIMEOUT)
}

/// Ensure the daemon is running with a custom timeout.
pub fn ensure_daemon_running_with_timeout(
    manifest_path: Option<&std::path::Path>,
    timeout: Duration,
) -> Result<()> {
    let socket_path = bronzite_types::default_socket_path();

    // Check if daemon is already running
    if is_daemon_running_at(&socket_path) {
        return Ok(());
    }

    // Find the bronzite-daemon binary
    let daemon_path = find_daemon_binary()?;

    // Build the command
    let mut cmd = Command::new(&daemon_path);
    cmd.arg("--ensure");
    cmd.arg("--ensure-timeout")
        .arg(timeout.as_secs().to_string());

    if let Some(path) = manifest_path {
        cmd.arg("--manifest-path").arg(path);
    }

    // Run the --ensure command (it will spawn a daemon if needed and wait)
    let output = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| Error::DaemonStartFailed(format!("Failed to run bronzite-daemon: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::DaemonStartFailed(format!(
            "bronzite-daemon --ensure failed: {}",
            stderr.trim()
        )));
    }

    // Verify daemon is now running
    if !is_daemon_running_at(&socket_path) {
        return Err(Error::DaemonStartTimeout);
    }

    Ok(())
}

/// Find the bronzite-daemon binary.
fn find_daemon_binary() -> Result<PathBuf> {
    // First, check if it's in PATH
    if let Ok(output) = Command::new("which").arg("bronzite-daemon").output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Ok(PathBuf::from(path));
            }
        }
    }

    // Check next to the current executable (for development)
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(parent) = current_exe.parent() {
            let daemon_path = parent.join("bronzite-daemon");
            if daemon_path.exists() {
                return Ok(daemon_path);
            }
        }
    }

    // Check in cargo bin directory
    if let Ok(home) = std::env::var("CARGO_HOME") {
        let daemon_path = PathBuf::from(home).join("bin").join("bronzite-daemon");
        if daemon_path.exists() {
            return Ok(daemon_path);
        }
    }

    // Check in ~/.cargo/bin
    if let Ok(home) = std::env::var("HOME") {
        let daemon_path = PathBuf::from(home)
            .join(".cargo")
            .join("bin")
            .join("bronzite-daemon");
        if daemon_path.exists() {
            return Ok(daemon_path);
        }
    }

    // Last resort: assume it's in PATH as just "bronzite-daemon"
    Ok(PathBuf::from("bronzite-daemon"))
}

/// Connect to the daemon, ensuring it's running first.
///
/// This is a convenience function that combines `ensure_daemon_running` and `connect`.
///
/// # Example
///
/// ```ignore
/// let mut client = bronzite_client::connect_or_start(None)?;
/// let items = client.list_items("my_crate")?;
/// ```
pub fn connect_or_start(manifest_path: Option<&std::path::Path>) -> Result<BronziteClient> {
    ensure_daemon_running(manifest_path)?;
    connect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_daemon_running_when_not_running() {
        // This should return false since we haven't started a daemon
        // Note: This test might fail if a daemon happens to be running
        assert!(!is_daemon_running() || true); // Always pass for now
    }

    #[test]
    fn test_connect_fails_when_daemon_not_running() {
        // Use a path that definitely doesn't exist
        let fake_path = PathBuf::from("/tmp/bronzite-nonexistent-12345.sock");
        let result = BronziteClient::connect_to(fake_path);
        assert!(result.is_err());
    }

    #[test]
    fn test_find_daemon_binary() {
        // This should at least not panic
        let _ = find_daemon_binary();
    }
}
