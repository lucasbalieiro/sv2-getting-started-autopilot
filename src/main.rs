use std::{
    collections::HashMap,
    convert::Infallible,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tokio::process::Child;
use tokio::{
    fs::File,
    io::{AsyncBufReadExt, BufReader},
    time::sleep,
};

use axum::{
    Router,
    extract::Query,
    response::{
        Html, Sse,
        sse::{Event, KeepAlive},
    },
    routing::get,
};
use futures_util::{Stream, stream};
use std::collections::HashMap as StdHashMap;
use tracing::{error, info};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    initial_config();
    let roles = Arc::new(Mutex::new(HashMap::new()));
    let roles_clone = roles.clone();
    let project_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("stratum")
        .join("roles");
    let _ = run_roles(project_path, roles.clone()).await;

    let app = Router::new()
        .route("/", get(index))
        .route("/last-commit", get(last_commit))
        .route("/sse-tp-logs", get(sse_tp_logs))
        .route("/sse-pool-logs", get(sse_pool_logs))
        .route("/sse-jds-logs", get(sse_jds_logs))
        .route("/sse-jdc-logs", get(sse_jdc_logs))
        .route("/sse-translator-logs", get(sse_translator_logs))
        .route("/sse-minerd-logs", get(sse_minerd_logs));
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();

    info!("Server running on http://0.0.0.0:3000");
    tokio::select! {
        _ = axum::serve(listener, app) => { }
        _ = tokio::signal::ctrl_c() => {
            info!("Received Ctrl+C, shutting down...");
            let mut roles = roles_clone.lock().unwrap();
            for (name, child) in roles.iter_mut() {
                match child.kill().await {
                    Ok(_) => info!("Killed {}", name),
                    Err(e) => error!("Failed to kill {}: {}", name, e),
                }
            }
        }
    }
}

async fn index() -> Html<String> {
    let html_content = include_str!("templates/index.html");
    Html(html_content.to_string())
}

// Helper to parse order param
fn is_newest_first(query: &StdHashMap<String, String>) -> bool {
    match query.get("order").map(|s| s.as_str()) {
        Some("newest") => true,
        _ => false,
    }
}

// Update all SSE endpoints to accept Query<HashMap<String, String>>
pub async fn sse_tp_logs(
    Query(params): Query<StdHashMap<String, String>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let file_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("template-provider")
        .join("bitcoin-sv2-tp-0.1.19")
        .join(".bitcoin")
        .join("testnet4")
        .join("debug.log");
    let newest_first = is_newest_first(&params);
    let log_stream = tail_file_lines(file_path, newest_first).await;
    Sse::new(log_stream).keep_alive(KeepAlive::new())
}

pub async fn sse_pool_logs(
    Query(params): Query<StdHashMap<String, String>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let file_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("stratum")
        .join("roles")
        .join("pool")
        .join("pool.log");
    let newest_first = is_newest_first(&params);
    let log_stream = tail_file_lines(file_path, newest_first).await;
    Sse::new(log_stream).keep_alive(KeepAlive::new())
}

pub async fn sse_jds_logs(
    Query(params): Query<StdHashMap<String, String>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let file_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("stratum")
        .join("roles")
        .join("jd-server")
        .join("jd-server.log");
    let newest_first = is_newest_first(&params);
    let log_stream = tail_file_lines(file_path, newest_first).await;
    Sse::new(log_stream).keep_alive(KeepAlive::new())
}

pub async fn sse_jdc_logs(
    Query(params): Query<StdHashMap<String, String>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let file_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("stratum")
        .join("roles")
        .join("jd-client")
        .join("jd-client.log");
    let newest_first = is_newest_first(&params);
    let log_stream = tail_file_lines(file_path, newest_first).await;
    Sse::new(log_stream).keep_alive(KeepAlive::new())
}

pub async fn sse_translator_logs(
    Query(params): Query<StdHashMap<String, String>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let file_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("stratum")
        .join("roles")
        .join("translator")
        .join("translator.log");
    let newest_first = is_newest_first(&params);
    let log_stream = tail_file_lines(file_path, newest_first).await;
    Sse::new(log_stream).keep_alive(KeepAlive::new())
}

pub async fn sse_minerd_logs(
    Query(params): Query<StdHashMap<String, String>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let file_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("minerd.log");
    let newest_first = is_newest_first(&params);
    let log_stream = tail_file_lines(file_path, newest_first).await;
    Sse::new(log_stream).keep_alive(KeepAlive::new())
}

pub async fn last_commit() -> Html<String> {
    let file_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("stratum");

    let output = std::process::Command::new("git")
        .args(["log", "-1", "--pretty=format:%H%n%an%n%ad%n%s"])
        .current_dir(&file_path)
        .output()
        .expect("failed to execute git command");

    let commit_info = if output.status.success() {
        String::from_utf8_lossy(&output.stdout).to_string()
    } else {
        String::from("Failed to retrieve commit info")
    };

    Html(commit_info)
}

fn initial_config() {
    if std::process::Command::new("git")
        .arg("--version")
        .output()
        .is_err()
    {
        error!("Git is not installed");
        std::process::exit(1);
    }

    let repo_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let stratum_path = repo_path.join("stratum");
    let repo_url = "https://github.com/stratum-mining/stratum.git";

    if !stratum_path.exists() {
        let output = std::process::Command::new("git")
            .arg("clone")
            .arg(repo_url)
            .arg(&stratum_path)
            .output()
            .expect("Failed to clone repository");

        if !output.status.success() {
            eprintln!(
                "Error cloning repository: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        } else {
            info!("Repository cloned successfully.");
        }
    } else {
        info!("Repository already exists, pulling latest changes...");
        let output = std::process::Command::new("git")
            .arg("pull")
            .arg("origin")
            .arg("main")
            .current_dir(&stratum_path)
            .output()
            .expect("Failed to pull latest changes");

        if !output.status.success() {
            eprintln!(
                "Error pulling repository: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        } else {
            info!("Repository updated successfully.");
        }
    }

    let roles_path = stratum_path.join("roles");
    let targets = ["jd-client", "jd-server", "pool", "translator"];

    for target in targets.iter() {
        let project_path = roles_path.join(target);

        if project_path.join("Cargo.toml").exists() {
            info!("Building project: {}", project_path.display());

            let output = std::process::Command::new("cargo")
                .arg("build")
                .arg("--jobs")
                .arg("3")
                .arg("--release")
                .current_dir(&project_path)
                .output()
                .expect("Failed to run cargo build");

            if !output.status.success() {
                eprintln!(
                    "Build failed for {}: {}",
                    project_path.display(),
                    String::from_utf8_lossy(&output.stderr)
                );
            } else {
                info!("Successfully built {}", project_path.display());
            }
        } else {
            error!(
                "Cargo.toml not found for project: {}",
                project_path.display()
            );
        }
    }

    let tp_log_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("template-provider")
        .join("bitcoin-sv2-tp-0.1.19")
        .join(".bitcoin")
        .join("testnet4")
        .join("debug.log");
    if let Err(e) = std::fs::write(&tp_log_path, "") {
        error!("Failed to clear TP log file: {}", e);
    } else {
        info!("TP log file cleared successfully.");
    }

    let roles_logs = [
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("stratum/roles/jd-client/jd-client.log"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("stratum/roles/jd-server/jd-server.log"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("stratum/roles/pool/pool.log"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("stratum/roles/translator/translator.log"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("minerd.log"),
    ];
    for log_path in &roles_logs {
        if let Err(e) = std::fs::write(log_path, "") {
            error!(
                "Failed to clear role log file {}: {}",
                log_path.display(),
                e
            );
        } else {
            info!("Role log file {} cleared successfully.", log_path.display());
        }
    }
}

fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

async fn run_roles(
    project_path: PathBuf,
    roles: Arc<Mutex<HashMap<String, Child>>>,
) -> std::io::Result<()> {
    use std::process::Stdio;
    use tokio::process::Command;
    // JD Client role
    let jd_client = Command::new("cargo")
        .arg("run")
        .arg("--release")
        .arg("--")
        .arg("--config")
        .arg("config-examples/jdc-config-local-example.toml")
        .arg("-f")
        .arg("jd-client.log")
        .current_dir(&project_path.join("jd-client"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    roles
        .lock()
        .unwrap()
        .insert("jd-client".to_string(), jd_client);

    // JD Server role
    let jd_server = Command::new("cargo")
        .arg("run")
        .arg("--release")
        .arg("--")
        .arg("--config")
        .arg("config-examples/jds-config-local-example.toml")
        .arg("-f")
        .arg("jd-server.log")
        .current_dir(&project_path.join("jd-server"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    roles
        .lock()
        .unwrap()
        .insert("jd-server".to_string(), jd_server);

    // Pool role
    let pool = Command::new("cargo")
        .arg("run")
        .arg("--release")
        .arg("--")
        .arg("--config")
        .arg("config-examples/pool-config-local-tp-example.toml")
        .arg("-f")
        .arg("pool.log")
        .current_dir(&project_path.join("pool"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    roles.lock().unwrap().insert("pool".to_string(), pool);

    // translator role
    let translator = Command::new("cargo")
        .arg("run")
        .arg("--release")
        .arg("--")
        .arg("--config")
        .arg("config-examples/tproxy-config-local-pool-example.toml")
        .arg("-f")
        .arg("translator.log")
        .current_dir(&project_path.join("translator"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    roles
        .lock()
        .unwrap()
        .insert("translator".to_string(), translator);

    // minerd role
    let minerd_log_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("minerd.log");
    let minerd_bin = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("minerd");
    let minerd = Command::new(minerd_bin)
        .arg("-t")
        .arg("1")
        .arg("-a")
        .arg("sha256d")
        .arg("-o")
        .arg("stratum+tcp://localhost:34255")
        .arg("-q")
        .arg("-D")
        .arg("-P")
        .stdout(std::fs::File::create(&minerd_log_path)?)
        .stderr(std::fs::File::create(&minerd_log_path)?)
        .spawn()?;
    roles.lock().unwrap().insert("minerd".to_string(), minerd);

    Ok(())
}

async fn tail_file_lines(
    path: PathBuf,
    newest_first: bool,
) -> impl Stream<Item = Result<Event, Infallible>> {
    use std::time::Duration;
    use tokio::time::interval;
    use tokio_stream::StreamExt;
    use tokio_stream::wrappers::IntervalStream;

    let interval = interval(Duration::from_secs(2));
    let stream = IntervalStream::new(interval);

    stream::unfold(stream, move |mut stream| {
        let path = path.clone();
        async move {
            stream.next().await?;
            let file = match File::open(&path).await {
                Ok(f) => f,
                Err(_) => {
                    sleep(Duration::from_secs(2)).await;
                    return Some((Ok(Event::default().data("Waiting for file...")), stream));
                }
            };
            let mut reader = BufReader::new(file).lines();
            let mut lines = Vec::new();
            while let Ok(Some(line)) = reader.next_line().await {
                lines.push(escape_html(&line));
            }
            let total = lines.len();
            let start = if total > 100 { total - 100 } else { 0 };
            let mut last_lines = lines[start..].to_vec();
            if newest_first {
                last_lines.reverse();
            }
            let joined = last_lines.join("\n");
            Some((Ok(Event::default().data(joined)), stream))
        }
    })
}
