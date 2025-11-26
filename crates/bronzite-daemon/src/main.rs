//! Bronzite daemon that caches rustc compilation and serves type queries.
//!
//! This daemon compiles target crates on-demand using the bronzite-query plugin
//! with `--extract` mode, caches the extracted type information, and serves
//! queries from proc-macros over a Unix socket.
//!
//! # Daemon Auto-Start
//!
//! The daemon supports an `--ensure` mode for use by proc-macros:
//! - If a daemon is already running, it exits immediately with success
//! - If no daemon is running, it spawns one in the background and waits for it to be ready

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};

use bronzite_types::{
    CrateTypeInfo, InherentImplDetails, Query, QueryData, QueryResult, Request, Response,
    TraitImplDetails, TraitInfo, TypeSummary,
};
use clap::Parser;

/// CLI arguments for the Bronzite daemon
#[derive(Parser, Debug)]
#[command(name = "bronzite-daemon")]
#[command(about = "A daemon that caches rustc compilation for type queries")]
struct Args {
    /// Path to the workspace/crate to analyze
    #[arg(short, long)]
    manifest_path: Option<PathBuf>,

    /// Socket path for IPC
    #[arg(short, long)]
    socket: Option<PathBuf>,

    /// Run in foreground (don't daemonize)
    #[arg(long, default_value = "true")]
    foreground: bool,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,

    /// Ensure a daemon is running (start one if needed, exit if already running)
    /// This is intended for use by proc-macros to auto-start the daemon.
    #[arg(long)]
    ensure: bool,

    /// Timeout in seconds when using --ensure to wait for daemon to be ready
    #[arg(long, default_value = "30")]
    ensure_timeout: u64,
}

/// Message sent to the cache manager thread
enum CacheMessage {
    Query {
        crate_name: String,
        query: Query,
        response_tx: Sender<QueryResult>,
    },
    InvalidateCache {
        crate_name: String,
    },
    Shutdown,
}

/// Cache manager that holds extracted type information
struct CacheManager {
    /// Cached type information per crate
    cache: HashMap<String, CrateTypeInfo>,
    /// Path to the cargo-bronzite-query binary
    query_binary: PathBuf,
    /// Working directory for compilation
    workspace_dir: Option<PathBuf>,
    /// Verbose logging
    verbose: bool,
}

impl CacheManager {
    fn new(workspace_dir: Option<PathBuf>, verbose: bool) -> Self {
        // Find the bronzite-query binary
        let query_binary = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .map(|p| p.join("cargo-bronzite-query"))
            .unwrap_or_else(|| PathBuf::from("cargo-bronzite-query"));

        Self {
            cache: HashMap::new(),
            query_binary,
            workspace_dir,
            verbose,
        }
    }

    fn get_or_compile(&mut self, crate_name: &str) -> Result<&CrateTypeInfo, String> {
        if !self.cache.contains_key(crate_name) {
            let info = self.compile_and_extract(crate_name)?;
            self.cache.insert(crate_name.to_string(), info);
        }
        Ok(self.cache.get(crate_name).unwrap())
    }

    fn compile_and_extract(&self, crate_name: &str) -> Result<CrateTypeInfo, String> {
        if self.verbose {
            eprintln!("[bronzite-daemon] Compiling crate: {}", crate_name);
        }

        let work_dir = self
            .workspace_dir
            .clone()
            .unwrap_or_else(|| std::env::current_dir().unwrap());

        // The specific nightly toolchain that bronzite requires
        const BRONZITE_TOOLCHAIN: &str = "nightly-2025-08-20";

        // Get the rustc sysroot for the bronzite toolchain
        let sysroot = get_rustc_sysroot_for_toolchain(BRONZITE_TOOLCHAIN)?;
        let lib_path = PathBuf::from(&sysroot).join("lib");

        // Set the library path environment variable
        #[cfg(target_os = "macos")]
        let lib_path_var = "DYLD_LIBRARY_PATH";
        #[cfg(target_os = "linux")]
        let lib_path_var = "LD_LIBRARY_PATH";
        #[cfg(target_os = "windows")]
        let lib_path_var = "PATH";

        // Use a separate target directory to avoid polluting user's cache
        // This also avoids conflicts with different toolchain versions
        let bronzite_target_dir = work_dir.join("target").join("bronzite");

        // Run cargo-bronzite-query with --extract flag using the specific toolchain
        // We use `rustup run <toolchain>` to ensure the correct nightly is used
        let output = Command::new("rustup")
            .arg("run")
            .arg(BRONZITE_TOOLCHAIN)
            .arg(&self.query_binary)
            .arg("bronzite-query")
            .arg("--extract")
            .current_dir(&work_dir)
            .env(lib_path_var, &lib_path)
            .env("CARGO_TARGET_DIR", &bronzite_target_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| format!("Failed to run bronzite-query: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Don't fail completely - some crates might fail to compile
            if self.verbose {
                eprintln!("[bronzite-daemon] Compilation had errors: {}", stderr);
            }
        }

        // Parse the output - it may contain multiple JSON objects (one per crate)
        // The output is pretty-printed, so we need to find complete JSON objects
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut found_info: Option<CrateTypeInfo> = None;

        // Extract JSON objects by tracking brace depth
        for json_str in extract_json_objects(&stdout) {
            match serde_json::from_str::<CrateTypeInfo>(&json_str) {
                Ok(info) => {
                    if info.crate_name == crate_name || crate_name.is_empty() {
                        found_info = Some(info);
                        if !crate_name.is_empty() {
                            break;
                        }
                    }
                }
                Err(e) => {
                    if self.verbose {
                        eprintln!("[bronzite-daemon] Failed to parse JSON: {}", e);
                    }
                }
            }
        }

        found_info.ok_or_else(|| format!("Crate '{}' not found in compilation output", crate_name))
    }

    fn invalidate(&mut self, crate_name: &str) {
        self.cache.remove(crate_name);
        if self.verbose {
            eprintln!("[bronzite-daemon] Invalidated cache for: {}", crate_name);
        }
    }

    fn execute_query(&mut self, crate_name: &str, query: Query) -> QueryResult {
        // Handle queries that don't need crate info
        match &query {
            Query::Ping => {
                return QueryResult::Success {
                    data: QueryData::Pong,
                };
            }
            Query::Shutdown => {
                return QueryResult::Success {
                    data: QueryData::ShuttingDown,
                };
            }
            _ => {}
        }

        // Get or compile the crate info
        let info = match self.get_or_compile(crate_name) {
            Ok(info) => info,
            Err(e) => {
                return QueryResult::Error { message: e };
            }
        };

        // Execute the specific query
        match query {
            Query::ListItems => QueryResult::Success {
                data: QueryData::Items {
                    items: info.items.clone(),
                },
            },

            Query::GetType { path } => {
                // Try exact match first, then suffix match
                let type_info = info.types.get(&path).or_else(|| {
                    info.types
                        .values()
                        .find(|t| t.path.ends_with(&format!("::{}", path)))
                });

                if let Some(type_info) = type_info {
                    QueryResult::Success {
                        data: QueryData::TypeInfo(type_info.clone()),
                    }
                } else {
                    QueryResult::Error {
                        message: format!("Type '{}' not found", path),
                    }
                }
            }

            Query::GetTraitImpls { type_path } => {
                // trait_impls is HashMap<String, Vec<TraitImplDetails>> keyed by self_ty
                let mut impls: Vec<TraitImplDetails> = Vec::new();

                // Try exact key match first
                if let Some(type_impls) = info.trait_impls.get(&type_path) {
                    impls.extend(type_impls.clone());
                }

                // Also search by suffix matching on keys
                for (key, type_impls) in &info.trait_impls {
                    if key != &type_path
                        && (key.ends_with(&format!("::{}", type_path))
                            || key.split('<').next() == Some(&type_path))
                    {
                        impls.extend(type_impls.clone());
                    }
                }

                QueryResult::Success {
                    data: QueryData::TraitImpls { impls },
                }
            }

            Query::GetInherentImpls { type_path } => {
                // inherent_impls is HashMap<String, Vec<InherentImplDetails>> keyed by self_ty
                let mut impls: Vec<InherentImplDetails> = Vec::new();

                // Try exact key match first
                if let Some(type_impls) = info.inherent_impls.get(&type_path) {
                    impls.extend(type_impls.clone());
                }

                // Also search by suffix matching on keys
                for (key, type_impls) in &info.inherent_impls {
                    if key != &type_path && key.ends_with(&format!("::{}", type_path)) {
                        impls.extend(type_impls.clone());
                    }
                }

                QueryResult::Success {
                    data: QueryData::InherentImpls { impls },
                }
            }

            Query::GetFields { type_path } => {
                // types is HashMap<String, TypeDetails>
                let type_info = info.types.get(&type_path).or_else(|| {
                    info.types
                        .values()
                        .find(|t| t.path.ends_with(&format!("::{}", type_path)))
                });

                if let Some(type_info) = type_info {
                    QueryResult::Success {
                        data: QueryData::Fields {
                            fields: type_info.fields.clone().unwrap_or_default(),
                        },
                    }
                } else {
                    QueryResult::Error {
                        message: format!("Type '{}' not found", type_path),
                    }
                }
            }

            Query::GetLayout { type_path } => {
                if let Some(layout) = info.layouts.get(&type_path) {
                    QueryResult::Success {
                        data: QueryData::Layout(layout.clone()),
                    }
                } else {
                    QueryResult::Error {
                        message: format!("Layout for '{}' not found", type_path),
                    }
                }
            }

            Query::GetTraits => {
                // traits is HashMap<String, TraitDetails>
                let traits: Vec<TraitInfo> = info
                    .traits
                    .values()
                    .map(|t| TraitInfo {
                        name: t.name.clone(),
                        path: t.path.clone(),
                        generics: t.generics.clone(),
                        required_methods: t.methods.iter().filter(|m| !m.has_default).count(),
                        provided_methods: t.methods.iter().filter(|m| m.has_default).count(),
                        supertraits: t.supertraits.clone(),
                    })
                    .collect();

                QueryResult::Success {
                    data: QueryData::Traits { traits },
                }
            }

            Query::GetTrait { path } => {
                // traits is HashMap<String, TraitDetails>
                let trait_info = info.traits.get(&path).or_else(|| {
                    info.traits
                        .values()
                        .find(|t| t.path.ends_with(&format!("::{}", path)))
                });

                if let Some(trait_info) = trait_info {
                    QueryResult::Success {
                        data: QueryData::TraitDetails(trait_info.clone()),
                    }
                } else {
                    QueryResult::Error {
                        message: format!("Trait '{}' not found", path),
                    }
                }
            }

            Query::FindTypes { pattern } => {
                // types is HashMap<String, TypeDetails>
                let types: Vec<TypeSummary> = info
                    .types
                    .values()
                    .filter(|t| bronzite_types::path_matches_pattern(&t.path, &pattern))
                    .map(|t| TypeSummary {
                        name: t.name.clone(),
                        path: t.path.clone(),
                        kind: t.kind.clone(),
                        generics: t.generics.clone(),
                    })
                    .collect();

                QueryResult::Success {
                    data: QueryData::Types { types },
                }
            }

            Query::ResolveAlias { path } => {
                // type_aliases is HashMap<String, TypeAliasInfo>
                let alias = info.type_aliases.get(&path).or_else(|| {
                    info.type_aliases
                        .values()
                        .find(|a| a.path.ends_with(&format!("::{}", path)))
                });

                if let Some(alias) = alias {
                    QueryResult::Success {
                        data: QueryData::ResolvedType {
                            original: alias.path.clone(),
                            resolved: alias.resolved_ty.clone(),
                            chain: vec![alias.ty.clone()],
                        },
                    }
                } else {
                    QueryResult::Error {
                        message: format!("Type alias '{}' not found", path),
                    }
                }
            }

            Query::CheckImpl {
                type_path,
                trait_path,
            } => {
                let (implements, impl_info) = check_impl_from_cache(info, &type_path, &trait_path);
                QueryResult::Success {
                    data: QueryData::ImplCheck {
                        implements,
                        impl_info,
                    },
                }
            }

            Query::GetImplementors { trait_path } => {
                // trait_impls is HashMap<String, Vec<TraitImplDetails>>
                let mut types: Vec<TypeSummary> = Vec::new();

                for (self_ty, impls) in &info.trait_impls {
                    for impl_ in impls {
                        let matches = impl_.trait_path == trait_path
                            || impl_.trait_path.ends_with(&format!("::{}", trait_path));

                        if matches {
                            if let Some(type_info) = info.types.get(self_ty) {
                                types.push(TypeSummary {
                                    name: type_info.name.clone(),
                                    path: type_info.path.clone(),
                                    kind: type_info.kind.clone(),
                                    generics: type_info.generics.clone(),
                                });
                            }
                        }
                    }
                }

                QueryResult::Success {
                    data: QueryData::Implementors { types },
                }
            }

            Query::Ping | Query::Shutdown => unreachable!(),
        }
    }
}

fn check_impl_from_cache(
    info: &CrateTypeInfo,
    type_path: &str,
    trait_path: &str,
) -> (bool, Option<TraitImplDetails>) {
    // trait_impls is HashMap<String, Vec<TraitImplDetails>> keyed by self_ty
    for (key, impls) in &info.trait_impls {
        let type_matches = key == type_path || key.ends_with(&format!("::{}", type_path));

        if type_matches {
            for impl_ in impls {
                let trait_matches = impl_.trait_path == trait_path
                    || impl_.trait_path.ends_with(&format!("::{}", trait_path));

                if trait_matches {
                    return (true, Some(impl_.clone()));
                }
            }
        }
    }
    (false, None)
}

/// Extract complete JSON objects from a string that may contain multiple objects.
fn extract_json_objects(input: &str) -> Vec<String> {
    let mut objects = Vec::new();
    let mut depth = 0;
    let mut start = None;
    let mut in_string = false;
    let mut escape_next = false;

    for (i, ch) in input.char_indices() {
        if escape_next {
            escape_next = false;
            continue;
        }

        match ch {
            '\\' if in_string => {
                escape_next = true;
            }
            '"' => {
                in_string = !in_string;
            }
            '{' if !in_string => {
                if depth == 0 {
                    start = Some(i);
                }
                depth += 1;
            }
            '}' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    if let Some(s) = start {
                        objects.push(input[s..=i].to_string());
                        start = None;
                    }
                }
            }
            _ => {}
        }
    }

    objects
}

/// Get the rustc sysroot path for a specific toolchain.
fn get_rustc_sysroot_for_toolchain(toolchain: &str) -> Result<String, String> {
    let output = Command::new("rustup")
        .arg("run")
        .arg(toolchain)
        .arg("rustc")
        .arg("--print")
        .arg("sysroot")
        .output()
        .map_err(|e| format!("Failed to get rustc sysroot for {}: {}", toolchain, e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "rustc --print sysroot failed for toolchain {}: {}",
            toolchain, stderr
        ));
    }

    String::from_utf8(output.stdout)
        .map(|s| s.trim().to_string())
        .map_err(|e| format!("Invalid UTF-8 in sysroot path: {}", e))
}

/// Try to connect to an existing daemon.
/// Returns Ok(stream) if connected, Err if no daemon is running.
#[cfg(unix)]
fn try_connect_to_daemon(socket_path: &PathBuf) -> Result<UnixStream, std::io::Error> {
    UnixStream::connect(socket_path)
}

/// Check if a daemon is already running and responding.
fn is_daemon_running(socket_path: &PathBuf) -> bool {
    if !socket_path.exists() {
        return false;
    }

    // Try to connect and ping
    match try_connect_to_daemon(socket_path) {
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
                    stream.flush().ok();
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

/// Spawn the daemon as a background process.
fn spawn_daemon_background(args: &Args) -> Result<(), String> {
    let exe =
        std::env::current_exe().map_err(|e| format!("Failed to get current executable: {}", e))?;

    let mut cmd = Command::new(&exe);

    // Pass through relevant arguments, but NOT --ensure
    if let Some(ref manifest_path) = args.manifest_path {
        cmd.arg("--manifest-path").arg(manifest_path);
    }
    if let Some(ref socket) = args.socket {
        cmd.arg("--socket").arg(socket);
    }
    if args.verbose {
        cmd.arg("--verbose");
    }

    // Detach the process
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Create a new process group so the daemon doesn't die with the parent
        cmd.process_group(0);
    }

    cmd.spawn()
        .map_err(|e| format!("Failed to spawn daemon: {}", e))?;

    Ok(())
}

/// Wait for the daemon to become ready.
fn wait_for_daemon_ready(socket_path: &PathBuf, timeout: Duration) -> Result<(), String> {
    let start = Instant::now();
    let poll_interval = Duration::from_millis(50);

    while start.elapsed() < timeout {
        if is_daemon_running(socket_path) {
            return Ok(());
        }
        std::thread::sleep(poll_interval);
    }

    Err(format!(
        "Timed out waiting for daemon to start ({}s)",
        timeout.as_secs()
    ))
}

/// Handle --ensure mode: ensure a daemon is running.
fn ensure_daemon_running(args: &Args) -> Result<(), String> {
    let socket_path = args
        .socket
        .clone()
        .unwrap_or_else(bronzite_types::default_socket_path);

    // Check if daemon is already running
    if is_daemon_running(&socket_path) {
        if args.verbose {
            eprintln!(
                "[bronzite-daemon] Daemon already running at {:?}",
                socket_path
            );
        }
        return Ok(());
    }

    if args.verbose {
        eprintln!("[bronzite-daemon] No daemon running, spawning one...");
    }

    // Spawn the daemon
    spawn_daemon_background(args)?;

    // Wait for it to be ready
    let timeout = Duration::from_secs(args.ensure_timeout);
    wait_for_daemon_ready(&socket_path, timeout)?;

    if args.verbose {
        eprintln!("[bronzite-daemon] Daemon is now running");
    }

    Ok(())
}

fn main() {
    env_logger::init();
    let args = Args::parse();

    // Handle --ensure mode
    if args.ensure {
        match ensure_daemon_running(&args) {
            Ok(()) => {
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("[bronzite-daemon] Error: {}", e);
                std::process::exit(1);
            }
        }
    }

    let socket_path = args
        .socket
        .clone()
        .unwrap_or_else(bronzite_types::default_socket_path);

    // Clean up existing socket
    if socket_path.exists() {
        if let Err(e) = std::fs::remove_file(&socket_path) {
            eprintln!("Warning: Failed to remove existing socket: {}", e);
        }
    }

    // Create the Unix socket listener
    let listener = match UnixListener::bind(&socket_path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Failed to bind to socket {:?}: {}", socket_path, e);
            std::process::exit(1);
        }
    };

    if args.verbose {
        eprintln!("[bronzite-daemon] Listening on {:?}", socket_path);
    }

    // Channel for communicating with the cache manager thread
    let (cache_tx, cache_rx): (Sender<CacheMessage>, Receiver<CacheMessage>) = mpsc::channel();

    // Shared flag for shutdown
    let running = Arc::new(Mutex::new(true));
    let running_clone = running.clone();

    // Handle Ctrl+C gracefully
    let cache_tx_shutdown = cache_tx.clone();
    std::thread::spawn(move || {
        // Simple signal handling - poll the running flag
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
            if !*running_clone.lock().unwrap() {
                break;
            }
        }
    });

    // Spawn the cache manager thread
    let verbose = args.verbose;
    let workspace_dir = args.manifest_path.clone().and_then(|p| {
        if p.is_file() {
            p.parent().map(|p| p.to_path_buf())
        } else {
            Some(p)
        }
    });

    let cache_handle = thread::spawn(move || {
        run_cache_manager(cache_rx, workspace_dir, verbose);
    });

    // Set socket to non-blocking for graceful shutdown
    listener
        .set_nonblocking(true)
        .expect("Cannot set non-blocking");

    // Accept connections
    loop {
        if !*running.lock().unwrap() {
            break;
        }

        match listener.accept() {
            Ok((stream, _)) => {
                let tx = cache_tx.clone();
                let verbose = args.verbose;
                let running_for_client = running.clone();
                thread::spawn(move || {
                    handle_client(stream, tx, verbose, running_for_client);
                });
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // No connection available, sleep briefly
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            Err(e) => {
                if args.verbose {
                    eprintln!("[bronzite-daemon] Accept error: {}", e);
                }
            }
        }
    }

    // Cleanup
    let _ = cache_tx_shutdown.send(CacheMessage::Shutdown);
    let _ = cache_handle.join();
    let _ = std::fs::remove_file(&socket_path);

    if args.verbose {
        eprintln!("[bronzite-daemon] Shut down");
    }
}

fn run_cache_manager(rx: Receiver<CacheMessage>, workspace_dir: Option<PathBuf>, verbose: bool) {
    let mut manager = CacheManager::new(workspace_dir, verbose);

    loop {
        match rx.recv() {
            Ok(CacheMessage::Query {
                crate_name,
                query,
                response_tx,
            }) => {
                let result = manager.execute_query(&crate_name, query);
                let _ = response_tx.send(result);
            }
            Ok(CacheMessage::InvalidateCache { crate_name }) => {
                manager.invalidate(&crate_name);
            }
            Ok(CacheMessage::Shutdown) => {
                if verbose {
                    eprintln!("[bronzite-daemon] Cache manager shutting down");
                }
                break;
            }
            Err(_) => {
                // Channel closed, exit
                break;
            }
        }
    }
}

fn handle_client(
    mut stream: UnixStream,
    cache_tx: Sender<CacheMessage>,
    verbose: bool,
    running: Arc<Mutex<bool>>,
) {
    // Set stream to blocking for reading
    stream
        .set_nonblocking(false)
        .expect("Cannot set blocking mode");

    let reader = BufReader::new(stream.try_clone().expect("Failed to clone stream"));

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                if verbose {
                    eprintln!("[bronzite-daemon] Read error: {}", e);
                }
                break;
            }
        };

        if line.is_empty() {
            continue;
        }

        // Parse the request
        let request: Request = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let response = Response {
                    id: 0,
                    result: QueryResult::Error {
                        message: format!("Failed to parse request: {}", e),
                    },
                };
                let _ = writeln!(stream, "{}", serde_json::to_string(&response).unwrap());
                continue;
            }
        };

        if verbose {
            eprintln!(
                "[bronzite-daemon] Request {}: {:?}",
                request.id, request.query
            );
        }

        // Check for shutdown request
        let is_shutdown = matches!(request.query, Query::Shutdown);

        // Send query to cache manager
        let (response_tx, response_rx) = mpsc::channel();
        let msg = CacheMessage::Query {
            crate_name: request.crate_name.clone(),
            query: request.query,
            response_tx,
        };

        if cache_tx.send(msg).is_err() {
            let response = Response {
                id: request.id,
                result: QueryResult::Error {
                    message: "Cache manager unavailable".to_string(),
                },
            };
            let _ = writeln!(stream, "{}", serde_json::to_string(&response).unwrap());
            break;
        }

        // Wait for response
        let result = match response_rx.recv() {
            Ok(r) => r,
            Err(_) => QueryResult::Error {
                message: "No response from cache manager".to_string(),
            },
        };

        let response = Response {
            id: request.id,
            result,
        };

        if let Err(e) = writeln!(stream, "{}", serde_json::to_string(&response).unwrap()) {
            if verbose {
                eprintln!("[bronzite-daemon] Write error: {}", e);
            }
            break;
        }

        // Handle shutdown
        if is_shutdown {
            *running.lock().unwrap() = false;
            break;
        }
    }

    if verbose {
        eprintln!("[bronzite-daemon] Client disconnected");
    }
}
