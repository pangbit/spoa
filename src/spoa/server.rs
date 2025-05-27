use std::future::Future;
use std::sync::Arc;

use futures::{SinkExt, StreamExt};
use semver::Version;
use spop::frames::{Ack, AgentDisconnect, AgentHello, FrameCapabilities, HaproxyHello};
use spop::{FramePayload, FrameType, SpopCodec, SpopFrame};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Semaphore, broadcast, mpsc};
use tokio::time::{self, Duration};
use tokio_util::codec::Framed;
use tracing::{error, info, instrument};

use super::process::Processer;
use super::{Error, Result, Shutdown};

// #[derive(Debug)]
struct Listener {
    listener: TcpListener,
    limit_connections: Arc<Semaphore>,
    notify_shutdown: broadcast::Sender<()>,
    shutdown_complete_tx: mpsc::Sender<()>,

    processer: Arc<Processer>,
}

// #[derive(Debug)]
struct Handler {
    // connection: Connection,
    socket: Framed<TcpStream, SpopCodec>,

    shutdown: Shutdown,

    _shutdown_complete: mpsc::Sender<()>,

    processer: Arc<Processer>,

    read_timeout: Duration,
    write_timeout: Duration,
}

const MAX_CONNECTIONS: usize = 8192;

pub async fn run(listener: TcpListener, processer: Arc<Processer>, shutdown: impl Future) {
    let (notify_shutdown, _) = broadcast::channel(1);
    let (shutdown_complete_tx, mut shutdown_complete_rx) = mpsc::channel(1);

    let mut server = Listener {
        listener,
        limit_connections: Arc::new(Semaphore::new(MAX_CONNECTIONS)),
        notify_shutdown,
        shutdown_complete_tx,

        processer: Arc::clone(&processer),
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

    // drop(server.notify_shutdown);
    // drop(server.shutdown_complete_tx);
    // server.shutdown_complete_rx.recv().await;

    let Listener {
        shutdown_complete_tx,
        notify_shutdown,
        ..
    } = server;

    drop(notify_shutdown);
    drop(shutdown_complete_tx);

    let _ = shutdown_complete_rx.recv().await;
}

impl Listener {
    async fn run(&mut self) -> Result<()> {
        info!("accepting inbound connections");

        loop {
            let permit = self
                .limit_connections
                .clone()
                .acquire_owned()
                .await
                .unwrap();

            let socket = self.accept().await?;

            let mut handler = Handler {
                // connection: Connection::new(socket),
                socket: Framed::new(socket, SpopCodec),
                shutdown: Shutdown::new(self.notify_shutdown.subscribe()),
                _shutdown_complete: self.shutdown_complete_tx.clone(),

                processer: Arc::clone(&self.processer),

                read_timeout: Duration::from_secs(30),
                write_timeout: Duration::from_secs(30),
            };

            tokio::spawn(async move {
                if let Err(err) = handler.run().await {
                    error!(cause = ?err, "connection error");
                }

                drop(permit)
            });
        }
    }

    async fn accept(&mut self) -> Result<TcpStream> {
        let mut backoff = 1;

        loop {
            match self.listener.accept().await {
                Ok((socket, _)) => return Ok(socket),
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

impl Handler {
    #[instrument(skip(self))]
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
                // Respond with AgentHello frame
                FrameType::HaproxyHello => {
                    let hello = HaproxyHello::try_from(frame.payload())
                        .map_err(Error::InvalidHaproxyHello)?;

                    let max_frame_size = hello.max_frame_size;
                    let is_healthcheck = hello.healthcheck.unwrap_or(false);
                    // * "version"    <STRING>
                    // This is the SPOP version the agent supports. It must follow the format
                    // "Major.Minor" and it must be lower or equal than one of major versions
                    // announced by HAProxy.
                    let version = Version::parse("2.0.0").unwrap();

                    // Create the AgentHello with the values
                    let agent_hello = AgentHello {
                        version,
                        max_frame_size,
                        capabilities: vec![FrameCapabilities::Pipelining],
                    };

                    info!("Sending AgentHello: {:?}", agent_hello.payload());

                    match time::timeout(self.write_timeout, self.socket.send(Box::new(agent_hello)))
                        .await
                    {
                        Ok(Ok(_)) => {}
                        Ok(Err(e)) => return Err(e.into()),
                        Err(_) => return Err(Error::WriteTimeout),
                    };

                    // If "healthcheck" item was set to TRUE in the HAPROXY-HELLO frame, the
                    // agent can safely close the connection without DISCONNECT frame. In all
                    // cases, HAProxy will close the connection at the end of the health check.
                    if is_healthcheck {
                        info!("Handled healthcheck. Closing socket.");
                        return Ok(());
                    }
                }

                // Respond with AgentDisconnect frame
                FrameType::HaproxyDisconnect => {
                    let agent_disconnect = AgentDisconnect {
                        status_code: 0,
                        message: "Goodbye".to_string(),
                    };

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

                // Respond with Ack frame
                FrameType::Notify => {
                    if let FramePayload::ListOfMessages(messages) = &frame.payload() {
                        let vars = self.processer.handle_frame(messages).await?;

                        // Create the Ack frame
                        let ack = vars.into_iter().fold(
                            Ack::new(frame.metadata().stream_id, frame.metadata().frame_id),
                            |ack, (scope, name, value)| ack.set_var(scope, &name, value),
                        );

                        // Create the response frame
                        info!("Sending Ack: {:?}", ack.payload());
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
