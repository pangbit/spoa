//! SPOP server implementation.
//!
//! Provides [`Server`] (builder pattern) and [`run`] (function) for starting
//! an async SPOP agent server. Supports both TCP and Unix sockets via the
//! [`SpoaListener`] trait.
//!
//! Configuration is done through [`ServerConfig`] which controls timeouts,
//! connection limits, and frame sizes.

use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use futures::{SinkExt, StreamExt};
use semver::Version;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::{TcpListener, TcpStream, UnixListener, UnixStream};
use tokio::sync::{RwLock, Semaphore, broadcast, mpsc};
use tokio::time::{self, Duration};
use tokio_util::codec::Framed;
use tracing::{debug, error, info};

use crate::protocol::frames::{Ack, AgentDisconnectFrame, AgentHelloFrame, FrameCapabilities, HaproxyHello};
use crate::protocol::{FramePayload, FrameType, SpopCodec, SpopFrame};
use crate::{Error, ProcesserHolder, Result, Shutdown};

/// Live server statistics, safe to read from any thread.
///
/// Obtain via [`Server::stats`] before calling [`Server::run`],
/// then query counters while the server is running.
#[derive(Debug, Default)]
pub struct ServerStats {
    /// Total connections accepted since server start.
    pub total_connections: AtomicU64,
    /// Currently active connections.
    pub active_connections: AtomicU64,
    /// Total NOTIFY messages processed.
    pub total_messages: AtomicU64,
    /// Total connection errors (timeout, parse, etc.).
    pub total_errors: AtomicU64,
}

impl ServerStats {
    /// Snapshot current stats as a plain struct (useful for logging/metrics export).
    pub fn snapshot(&self) -> StatsSnapshot {
        StatsSnapshot {
            total_connections: self.total_connections.load(Ordering::Relaxed),
            active_connections: self.active_connections.load(Ordering::Relaxed),
            total_messages: self.total_messages.load(Ordering::Relaxed),
            total_errors: self.total_errors.load(Ordering::Relaxed),
        }
    }
}

/// Point-in-time copy of server statistics.
#[derive(Debug, Clone)]
pub struct StatsSnapshot {
    pub total_connections: u64,
    pub active_connections: u64,
    pub total_messages: u64,
    pub total_errors: u64,
}

/// Server configuration with sensible defaults.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Read timeout per connection. Default: 30s.
    pub read_timeout: Duration,
    /// Write timeout per connection. Default: 30s.
    pub write_timeout: Duration,
    /// Maximum concurrent connections. Default: 100_000.
    pub max_connections: usize,
    /// Maximum SPOP frame size in bytes. Default: 16_384.
    pub max_frame_size: usize,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            read_timeout: Duration::from_secs(30),
            write_timeout: Duration::from_secs(30),
            max_connections: 100_000,
            max_frame_size: 16_384,
        }
    }
}

/// Builder for configuring and running a SPOA server.
pub struct Server<L: SpoaListener> {
    listener: L,
    processer: Arc<RwLock<ProcesserHolder>>,
    config: ServerConfig,
    stats: Arc<ServerStats>,
}

impl<L: SpoaListener> Server<L> {
    pub fn new(listener: L, processer: Arc<RwLock<ProcesserHolder>>) -> Self {
        Self {
            listener,
            processer,
            config: ServerConfig::default(),
            stats: Arc::new(ServerStats::default()),
        }
    }

    pub fn config(mut self, config: ServerConfig) -> Self {
        self.config = config;
        self
    }

    /// Returns a shared reference to live server statistics.
    ///
    /// Call this before [`run`](Server::run) to retain access to stats
    /// while the server is running.
    pub fn stats(&self) -> Arc<ServerStats> {
        Arc::clone(&self.stats)
    }

    pub async fn run(self, shutdown: impl Future) {
        run(self.listener, self.processer, shutdown, self.config, Some(self.stats)).await;
    }
}

/// Trait abstracting TCP and Unix socket listeners.
pub trait SpoaListener: Send + 'static {
    type Stream: AsyncRead + AsyncWrite + Send + Unpin + 'static;

    fn accept(&self) -> impl Future<Output = std::io::Result<Self::Stream>> + Send;
}

impl SpoaListener for TcpListener {
    type Stream = TcpStream;

    async fn accept(&self) -> std::io::Result<TcpStream> {
        let (stream, _addr) = TcpListener::accept(self).await?;
        Ok(stream)
    }
}

impl SpoaListener for UnixListener {
    type Stream = UnixStream;

    async fn accept(&self) -> std::io::Result<UnixStream> {
        let (stream, _addr) = UnixListener::accept(self).await?;
        Ok(stream)
    }
}

struct Listener<L: SpoaListener> {
    listener: L,
    config: ServerConfig,
    stats: Arc<ServerStats>,
    limit_connections: Arc<Semaphore>,
    notify_shutdown: broadcast::Sender<()>,
    shutdown_complete_tx: mpsc::Sender<()>,
    processer_holder: Arc<RwLock<ProcesserHolder>>,
}

struct Handler<S: AsyncRead + AsyncWrite + Send + Unpin> {
    socket: Framed<S, SpopCodec>,
    read_timeout: Duration,
    write_timeout: Duration,
    stats: Arc<ServerStats>,
    shutdown: Shutdown,
    _shutdown_complete: mpsc::Sender<()>,
    processer_holder: Arc<RwLock<ProcesserHolder>>,
}

pub async fn run<L: SpoaListener>(
    listener: L,
    processer: Arc<RwLock<ProcesserHolder>>,
    shutdown: impl Future,
    config: ServerConfig,
    stats: Option<Arc<ServerStats>>,
) {
    let stats = stats.unwrap_or_default();
    let (notify_shutdown, _) = broadcast::channel(1);
    let (shutdown_complete_tx, mut shutdown_complete_rx) = mpsc::channel(1);

    let mut server = Listener {
        listener,
        config: config.clone(),
        stats,
        limit_connections: Arc::new(Semaphore::new(config.max_connections)),
        notify_shutdown,
        shutdown_complete_tx,
        processer_holder: Arc::clone(&processer),
    };

    tokio::select! {
        res = server.run() => {
            if let Err(err) = res {
                error!(cause = %err, "failed to accept");
            }
        }
        _ = shutdown => {
            info!("shutting down");
        }
    }

    let Listener {
        shutdown_complete_tx,
        notify_shutdown,
        ..
    } = server;

    drop(notify_shutdown);
    drop(shutdown_complete_tx);

    let _ = shutdown_complete_rx.recv().await;
}

impl<L: SpoaListener> Listener<L> {
    async fn run(&mut self) -> Result<()> {
        info!("accepting inbound connections");

        loop {
            let permit = self
                .limit_connections
                .clone()
                .acquire_owned()
                .await
                .unwrap();

            let socket = self.accept_with_backoff().await?;

            let stats = Arc::clone(&self.stats);
            stats.total_connections.fetch_add(1, Ordering::Relaxed);
            stats.active_connections.fetch_add(1, Ordering::Relaxed);

            let mut handler = Handler {
                socket: Framed::new(socket, SpopCodec { max_frame_size: self.config.max_frame_size }),
                shutdown: Shutdown::new(self.notify_shutdown.subscribe()),
                _shutdown_complete: self.shutdown_complete_tx.clone(),
                processer_holder: Arc::clone(&self.processer_holder),
                read_timeout: self.config.read_timeout,
                write_timeout: self.config.write_timeout,
                stats: Arc::clone(&stats),
            };

            tokio::spawn(async move {
                if let Err(err) = handler.run().await {
                    debug!(cause = ?err, "connection error");
                    stats.total_errors.fetch_add(1, Ordering::Relaxed);
                }
                stats.active_connections.fetch_sub(1, Ordering::Relaxed);
                drop(permit)
            });
        }
    }

    async fn accept_with_backoff(&self) -> Result<L::Stream> {
        let mut backoff = 1;

        loop {
            match self.listener.accept().await {
                Ok(stream) => return Ok(stream),
                Err(err) => {
                    if backoff > 64 {
                        return Err(Error::IO(err));
                    }
                }
            }

            time::sleep(Duration::from_secs(backoff)).await;
            backoff *= 2;
        }
    }
}

impl<S: AsyncRead + AsyncWrite + Send + Unpin> Handler<S> {
    async fn run(&mut self) -> Result<()> {
        while !self.shutdown.is_shutdown() {
            let maybe_frame = tokio::select! {
                res = self.socket.next() => res,
                _ = self.shutdown.recv() => {
                    return Ok(());
                }
                _ = time::sleep(self.read_timeout) => {
                    return Err(Error::ReadTimeout);
                }
            };

            let frame = match maybe_frame {
                Some(Ok(frame)) => frame,
                Some(Err(e)) => {
                    error!("read_frame failed: {}", e);
                    return Err(Error::IO(e));
                }
                None => return Ok(()),
            };

            match frame.frame_type() {
                FrameType::HaproxyHello => {
                    let hello = HaproxyHello::try_from(frame.payload())
                        .map_err(Error::HandshakeFailed)?;

                    let max_frame_size = hello.max_frame_size;
                    let is_healthcheck = hello.healthcheck.unwrap_or(false);
                    let version = Version::parse("2.0.0").unwrap();

                    let agent_hello = AgentHelloFrame::new(
                        version,
                        max_frame_size,
                        vec![FrameCapabilities::Pipelining],
                    );

                    debug!("Sending AgentHello: {:?}", agent_hello.payload());

                    match time::timeout(self.write_timeout, self.socket.send(Box::new(agent_hello)))
                        .await
                    {
                        Ok(Ok(_)) => {}
                        Ok(Err(e)) => return Err(e.into()),
                        Err(_) => return Err(Error::WriteTimeout),
                    };

                    if is_healthcheck {
                        info!("Handled healthcheck. Closing socket.");
                        return Ok(());
                    }
                }

                FrameType::HaproxyDisconnect => {
                    let agent_disconnect = AgentDisconnectFrame::new(0, "Goodbye".to_string());
                    info!("Sending AgentDisconnect: {:?}", agent_disconnect.payload());

                    match time::timeout(
                        self.write_timeout,
                        self.socket.send(Box::new(agent_disconnect)),
                    )
                    .await
                    {
                        Ok(Ok(_)) => self.socket.close().await?,
                        Ok(Err(e)) => return Err(e.into()),
                        Err(_) => return Err(Error::WriteTimeout),
                    }

                    return Ok(());
                }

                FrameType::Notify => {
                    if let FramePayload::ListOfMessages(messages) = &frame.payload() {
                        self.stats.total_messages.fetch_add(1, Ordering::Relaxed);
                        let meta = frame.metadata();

                        let ack = match self
                            .processer_holder
                            .read()
                            .await
                            .processer
                            .handle_messages(messages)
                            .await
                        {
                            Ok(vars) => {
                                vars.into_iter().fold(
                                    Ack::new(meta.stream_id, meta.frame_id),
                                    |ack, (scope, name, value)| ack.set_var(scope, &name, value),
                                )
                            }
                            Err(e) => {
                                error!("processer handle_messages failed: {}", e);
                                Ack::new(meta.stream_id, meta.frame_id)
                            }
                        };

                        debug!("Sending Ack: {:?}", ack.payload());
                        match time::timeout(self.write_timeout, self.socket.send(Box::new(ack)))
                            .await
                        {
                            Ok(Ok(_)) => {}
                            Ok(Err(e)) => return Err(e.into()),
                            Err(_) => return Err(Error::WriteTimeout),
                        }
                    }
                }

                _ => {
                    error!("Unsupported frame type: {:?}", frame.frame_type());
                }
            }
        }

        Ok(())
    }
}
