/// Node state machine: heartbeat management, role transitions, and HTTP API.
///
/// Each node exposes two interfaces:
/// * **HTTP** (port `http_port`) — proxied by Nginx; accepts client `GET /read`
///   and `POST /write` requests and redirects to the leader when necessary.
///   NOVO: Também aceita `/entrar_fila` e `/checkout` para venda de ingressos.
/// * **TCP cluster** (port `cluster_port`) — used for inter-node messages
///   (heartbeat, election, replication).
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use axum::{
    Router,
    extract::{Query, State, Json},
    http::{StatusCode, Method},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    time,
};
use tower_http::cors::{Any, CorsLayer};
use uuid::Uuid;

use crate::{
    election,
    message::Message,
    sync::{self, Store},
};

/// How often the leader sends heartbeats.
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(1);
/// How long without a heartbeat before a follower starts an election.
pub const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(3);

/// Possible roles a node can hold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Role {
    Follower,
    Candidate,
    Leader,
}

/// Shared mutable state for a node.
pub struct NodeState {
    pub id: u64,
    pub term: u64,
    pub role: Role,
    pub leader_id: Option<u64>,
    /// Address of the leader's **HTTP** port, for client redirects.
    pub leader_http_addr: Option<String>,
    pub last_heartbeat: Instant,
    /// `(id, cluster_addr)` for every peer.
    pub peers: Vec<(u64, String)>,
    /// HTTP addresses of all peers, keyed by node ID.
    pub peer_http_addrs: HashMap<u64, String>,
    pub store: Store,
    /// Monotonically increasing replication sequence counter.
    pub seq: u64,
    pub election_in_progress: bool,
    
    // ─── NOVO: Conexões para o sistema de ingressos ───
    pub db_pool: PgPool,
    pub redis_client: redis::Client,
}

impl NodeState {
    pub fn new(
        id: u64,
        peers: Vec<(u64, String)>,
        peer_http_addrs: HashMap<u64, String>,
        db_pool: PgPool,
        redis_client: redis::Client,
    ) -> Self {
        Self {
            id,
            term: 0,
            role: Role::Follower,
            leader_id: None,
            leader_http_addr: None,
            last_heartbeat: Instant::now(),
            peers,
            peer_http_addrs,
            store: sync::new_store(),
            seq: 0,
            election_in_progress: false,
            db_pool,
            redis_client,
        }
    }
}

pub type SharedState = Arc<Mutex<NodeState>>;

// ─── Heartbeat tasks ─────────────────────────────────────────────────────────

/// Spawned by the leader: periodically sends `Heartbeat` to all peers.
pub async fn heartbeat_sender(state: SharedState) {
    loop {
        time::sleep(HEARTBEAT_INTERVAL).await;

        let (peers, term, id, is_leader) = {
            let s = state.lock().unwrap();
            (s.peers.clone(), s.term, s.id, s.role == Role::Leader)
        };

        if !is_leader {
            break; // Stop the task when this node is no longer leader.
        }

        let msg = Message::Heartbeat { leader_id: id, term }.to_line();
        for (_, addr) in &peers {
            if let Ok(mut stream) = TcpStream::connect(addr).await {
                let _ = stream.write_all(msg.as_bytes()).await;
            }
        }
    }
}

/// Spawned by every node: watches for heartbeat timeout and triggers election.
pub async fn heartbeat_monitor(state: SharedState) {
    loop {
        time::sleep(Duration::from_millis(500)).await;

        let (elapsed, is_leader, in_progress) = {
            let s = state.lock().unwrap();
            (
                s.last_heartbeat.elapsed(),
                s.role == Role::Leader,
                s.election_in_progress,
            )
        };

        if is_leader || in_progress {
            continue;
        }

        if elapsed > HEARTBEAT_TIMEOUT {
            let (my_id, peers) = {
                let mut s = state.lock().unwrap();
                s.role = Role::Candidate;
                s.election_in_progress = true;
                (s.id, s.peers.clone())
            };

            tracing_log(&format!(
                "Node {my_id}: heartbeat timeout — starting election"
            ));

            let winner = election::start_election(my_id, &peers);

            let mut s = state.lock().unwrap();
            s.election_in_progress = false;
            if winner == my_id {
                s.role = Role::Leader;
                s.leader_id = Some(my_id);
                s.leader_http_addr = None; // self is the leader
                s.term += 1;
                tracing_log(&format!("Node {my_id}: elected as leader (term {})", s.term));
                // Reset heartbeat so the sender loop starts fresh.
                s.last_heartbeat = Instant::now();
                drop(s);
                tokio::spawn(heartbeat_sender(Arc::clone(&state)));
            } else {
                // Another node won (or election is still in progress).
                s.role = Role::Follower;
                s.last_heartbeat = Instant::now(); // avoid tight loop
            }
        }
    }
}

// ─── Cluster TCP server ───────────────────────────────────────────────────────

/// Accept inter-node connections and dispatch messages.
pub async fn cluster_server(state: SharedState, cluster_port: u16) {
    let addr = SocketAddr::from(([0, 0, 0, 0], cluster_port));
    let listener = TcpListener::bind(addr)
        .await
        .expect("failed to bind cluster port");

    loop {
        let Ok((stream, _)) = listener.accept().await else {
            continue;
        };
        let state = Arc::clone(&state);
        tokio::spawn(handle_cluster_connection(stream, state));
    }
}

async fn handle_cluster_connection(stream: TcpStream, state: SharedState) {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    while let Ok(Some(line)) = lines.next_line().await {
        let Ok(msg) = Message::from_line(&line) else {
            continue;
        };

        match msg {
            Message::Heartbeat { leader_id, term } => {
                let mut s = state.lock().unwrap();
                if term >= s.term {
                    s.term = term;
                    s.leader_id = Some(leader_id);
                    s.role = Role::Follower;
                    s.last_heartbeat = Instant::now();
                    s.leader_http_addr = s.peer_http_addrs.get(&leader_id).cloned();
                }
            }

            Message::Election { candidate_id } => {
                let my_id = state.lock().unwrap().id;
                if my_id > candidate_id {
                    // Reply OK and start our own election.
                    let ok = Message::Ok { from_id: my_id }.to_line();
                    let _ = writer.write_all(ok.as_bytes()).await;
                    tokio::spawn(trigger_election(Arc::clone(&state)));
                }
            }

            Message::Coordinator { leader_id } => {
                let mut s = state.lock().unwrap();
                s.leader_id = Some(leader_id);
                s.role = Role::Follower;
                s.term += 1;
                s.election_in_progress = false;
                s.last_heartbeat = Instant::now();
                s.leader_http_addr = s.peer_http_addrs.get(&leader_id).cloned();
                tracing_log(&format!(
                    "Node {}: coordinator is now node {leader_id}",
                    s.id
                ));
            }

            Message::Replicate { key, value, seq } => {
                // Follower: apply the entry and send ACK (strong consistency).
                let (_, ack_line) = {
                    let s = state.lock().unwrap();
                    sync::apply(&s.store, &key, &value);
                    let from_id = s.id;
                    let ack = Message::ReplicateAck { seq, from_id }.to_line();
                    (from_id, ack)
                };
                let _ = writer.write_all(ack_line.as_bytes()).await;
            }

            // Ignore messages that are not expected in this direction.
            Message::Ok { .. } | Message::ReplicateAck { .. } => {}
        }
    }
}

/// Trigger an election from within a spawned task.
async fn trigger_election(state: SharedState) {
    let (my_id, peers) = {
        let mut s = state.lock().unwrap();
        if s.election_in_progress {
            return;
        }
        s.election_in_progress = true;
        s.role = Role::Candidate;
        (s.id, s.peers.clone())
    };

    let winner = tokio::task::spawn_blocking(move || election::start_election(my_id, &peers))
        .await
        .unwrap_or(0);

    let mut s = state.lock().unwrap();
    s.election_in_progress = false;
    if winner == my_id {
        s.role = Role::Leader;
        s.leader_id = Some(my_id);
        s.leader_http_addr = None;
        s.term += 1;
        drop(s);
        tokio::spawn(heartbeat_sender(Arc::clone(&state)));
    } else {
        s.role = Role::Follower;
        s.last_heartbeat = Instant::now();
    }
}

// ─── HTTP server ──────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ReadQuery {
    key: String,
}

#[derive(Deserialize)]
struct WriteQuery {
    key: String,
    value: String,
}

#[derive(Serialize)]
struct FilaResponse {
    user_id: String,
    posicao: isize,
    node_id: u64,
}

#[derive(Deserialize)]
struct CheckoutRequest {
    user_id: String,
    ticket_id: i32,
}

/// Returns a JSON snapshot of this node's current status.
async fn handle_status(State(state): State<SharedState>) -> impl IntoResponse {
    let s = state.lock().unwrap();
    let body = serde_json::json!({
        "id": s.id,
        "role": format!("{:?}", s.role),
        "term": s.term,
        "leader_id": s.leader_id,
    });
    axum::Json(body)
}

/// Read a key from the store (any node can serve reads).
async fn handle_read(
    State(state): State<SharedState>,
    Query(q): Query<ReadQuery>,
) -> Response {
    let s = state.lock().unwrap();
    match sync::read(&s.store, &q.key) {
        Some(v) => (StatusCode::OK, v).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Write a key/value pair.  Only the leader accepts writes; followers redirect.
async fn handle_write(
    State(state): State<SharedState>,
    Query(q): Query<WriteQuery>,
) -> Response {
    let (is_leader, leader_http, my_id, peers, seq) = {
        let mut s = state.lock().unwrap();
        let is_leader = s.role == Role::Leader;
        let leader_http = s.leader_http_addr.clone();
        let peers: Vec<String> = s.peers.iter().map(|(_, a)| a.clone()).collect();
        let seq = if is_leader {
            s.seq += 1;
            s.seq
        } else {
            0
        };
        (is_leader, leader_http, s.id, peers, seq)
    };

    if !is_leader {
        // Redirect the client to the current leader (load redirection).
        if let Some(leader_addr) = leader_http {
            let public_addr = leader_addr
                .replace("node1", "localhost")
                .replace("node2", "localhost")
                .replace("node3", "localhost");
            let url = format!("http://{}/checkout", public_addr);
            return Redirect::temporary(&url).into_response();
        }
        return (StatusCode::SERVICE_UNAVAILABLE, "no leader elected yet").into_response();
    }

    // Strong consistency: replicate to every follower peer and wait for ACKs.
    let follower_addrs: Vec<String> = peers;
    let _acks = tokio::task::spawn_blocking({
        let key = q.key.clone();
        let value = q.value.clone();
        move || sync::replicate(&follower_addrs, &key, &value, seq)
    })
    .await
    .unwrap_or(0);

    // Apply locally after replication.
    {
        let s = state.lock().unwrap();
        sync::apply(&s.store, &q.key, &q.value);
    }

    tracing_log(&format!(
        "Node {my_id} (leader): wrote key='{}' value='{}' seq={seq}",
        q.key, q.value
    ));

    (StatusCode::OK, "OK").into_response()
}

// ─── NOVO: Endpoints do sistema de ingressos ─────────────────────────────────

/// Entrar na fila de espera (Redis sorted set).
async fn handle_entrar_fila(State(state): State<SharedState>) -> Result<Json<FilaResponse>, StatusCode> {
    let user_id = Uuid::new_v4().to_string();
    let score = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as f64;

    let (redis_client, node_id) = {
        let s = state.lock().unwrap();
        (s.redis_client.clone(), s.id)
    };

    let mut conn = redis_client
        .get_async_connection()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let _: () = conn
        .zadd("fila_ingressos", &user_id, score)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let posicao: isize = conn
        .zrank("fila_ingressos", &user_id)
        .await
        .unwrap_or(0);

    tracing_log(&format!(
        "Node {node_id}: user {user_id} entrou na fila (posição {posicao})"
    ));

    Ok(Json(FilaResponse {
        user_id,
        posicao,
        node_id,
    }))
}

/// Realizar checkout (somente o LEADER aceita).
async fn handle_checkout(
    State(state): State<SharedState>,
    Json(payload): Json<CheckoutRequest>,
) -> Response {
    // Verificar se este nó é o líder
    let (is_leader, leader_http, my_id) = {
        let s = state.lock().unwrap();
        (
            s.role == Role::Leader,
            s.leader_http_addr.clone(),
            s.id,
        )
    };

    if !is_leader {
        // Redirecionar para o líder
        if let Some(leader_addr) = leader_http {
            let url = format!(
                "http://{}/checkout",
                leader_addr
            );
            return Redirect::temporary(&url).into_response();
        }
        return (StatusCode::SERVICE_UNAVAILABLE, "no leader elected yet").into_response();
    }

    // Apenas o LEADER processa checkouts (evita race conditions)
    let (db_pool, redis_client) = {
        let s = state.lock().unwrap();
        (s.db_pool.clone(), s.redis_client.clone())
    };

    // Transação no Postgres com FOR UPDATE
    let mut tx = match db_pool.begin().await {
        Ok(t) => t,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "database error").into_response(),
    };

    let ticket = match sqlx::query_as::<_, (String,)>(
        "SELECT status FROM ingressos WHERE id = $1 FOR UPDATE"
    )
    .bind(payload.ticket_id)
    .fetch_optional(&mut *tx)
    .await
    {
        Ok(t) => t,
        Err(_) => {
            let _ = tx.rollback().await;
            return (StatusCode::INTERNAL_SERVER_ERROR, "query error").into_response();
        }
    };

    match ticket {
        Some((status,)) if status == "disponivel" => {
            // Atualizar para vendido
            if sqlx::query(
                "UPDATE ingressos SET status = 'vendido', user_id = $1 WHERE id = $2"
            )
            .bind(&payload.user_id)
            .bind(payload.ticket_id)
            .execute(&mut *tx)
            .await
            .is_err()
            {
                let _ = tx.rollback().await;
                return (StatusCode::INTERNAL_SERVER_ERROR, "update error").into_response();
            }

            // Commit
            if tx.commit().await.is_err() {
                return (StatusCode::INTERNAL_SERVER_ERROR, "commit error").into_response();
            }

            // Remover da fila
            if let Ok(mut conn) = redis_client.get_async_connection().await {
                let _: Result<(), _> = conn.zrem("fila_ingressos", &payload.user_id).await;
            }

            tracing_log(&format!(
                "Node {my_id} (leader): checkout OK - user={} ticket={}",
                payload.user_id, payload.ticket_id
            ));

            (StatusCode::OK, "Compra realizada com sucesso!").into_response()
        }
        _ => {
            let _ = tx.rollback().await;
            (StatusCode::CONFLICT, "Ingresso esgotado").into_response()
        }
    }
}

/// Build the HTTP router for this node.
pub fn http_router(state: SharedState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST])
        .allow_headers(Any);

    Router::new()
        .route("/status", get(handle_status))
        .route("/read", get(handle_read))
        .route("/write", post(handle_write).get(handle_write))
        // NOVO: Endpoints de ingressos
        .route("/entrar_fila", post(handle_entrar_fila))
        .route("/checkout", post(handle_checkout))
        .layer(cors)
        .with_state(state)
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn tracing_log(msg: &str) {
    eprintln!("[node] {msg}");
}
