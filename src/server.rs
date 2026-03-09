use std::future::Future;
use std::sync::Arc;

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

const MAX_CONNECTIONS: usize = 100_000;

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
    limit_connections: Arc<Semaphore>,
    notify_shutdown: broadcast::Sender<()>,
    shutdown_complete_tx: mpsc::Sender<()>,
    processer_holder: Arc<RwLock<ProcesserHolder>>,
}

struct Handler<S: AsyncRead + AsyncWrite + Send + Unpin> {
    socket: Framed<S, SpopCodec>,
    read_timeout: Duration,
    write_timeout: Duration,
    shutdown: Shutdown,
    _shutdown_complete: mpsc::Sender<()>,
    processer_holder: Arc<RwLock<ProcesserHolder>>,
}

pub async fn run<L: SpoaListener>(
    listener: L,
    processer: Arc<RwLock<ProcesserHolder>>,
    shutdown: impl Future,
) {
    let (notify_shutdown, _) = broadcast::channel(1);
    let (shutdown_complete_tx, mut shutdown_complete_rx) = mpsc::channel(1);

    let mut server = Listener {
        listener,
        limit_connections: Arc::new(Semaphore::new(MAX_CONNECTIONS)),
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

            let mut handler = Handler {
                socket: Framed::new(socket, SpopCodec { max_frame_size: 0 }),
                shutdown: Shutdown::new(self.notify_shutdown.subscribe()),
                _shutdown_complete: self.shutdown_complete_tx.clone(),
                processer_holder: Arc::clone(&self.processer_holder),
                read_timeout: Duration::from_secs(30),
                write_timeout: Duration::from_secs(30),
            };

            tokio::spawn(async move {
                if let Err(err) = handler.run().await {
                    debug!(cause = ?err, "connection error");
                }
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
