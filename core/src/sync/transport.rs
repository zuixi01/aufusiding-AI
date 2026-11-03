//! 传输层
//!
//! WebSocket 传输层，负责与同步中继服务器的实时通信。

use async_trait::async_trait;
use tokio::sync::mpsc;

/// 传输层 trait
#[async_trait]
pub trait Transport: Send + Sync {
    /// 连接到服务器
    async fn connect(&mut self, url: &str) -> anyhow::Result<()>;
    /// 发送数据
    async fn send(&self, data: &[u8]) -> anyhow::Result<()>;
    /// 接收数据
    async fn receive(&self) -> anyhow::Result<Vec<u8>>;
    /// 断开连接
    async fn disconnect(&mut self) -> anyhow::Result<()>;
    /// 检查连接状态
    fn is_connected(&self) -> bool;
}

/// Mock 传输实现（内存通道）
pub struct MockTransport {
    connected: bool,
    tx: mpsc::UnboundedSender<Vec<u8>>,
    rx: std::sync::Mutex<mpsc::UnboundedReceiver<Vec<u8>>>,
}

impl MockTransport {
    fn from_channel(
        tx: mpsc::UnboundedSender<Vec<u8>>,
        rx: mpsc::UnboundedReceiver<Vec<u8>>,
    ) -> Self {
        Self {
            connected: false,
            tx,
            rx: std::sync::Mutex::new(rx),
        }
    }
}

/// 创建配对传输（双向通信）
pub fn create_transport_pair() -> (MockTransport, MockTransport) {
    let (tx1, rx1) = mpsc::unbounded_channel();
    let (tx2, rx2) = mpsc::unbounded_channel();
    // A sends to B's rx, B sends to A's rx
    (
        MockTransport::from_channel(tx2, rx1),
        MockTransport::from_channel(tx1, rx2),
    )
}

#[async_trait]
impl Transport for MockTransport {
    async fn connect(&mut self, _url: &str) -> anyhow::Result<()> {
        self.connected = true;
        Ok(())
    }

    async fn send(&self, data: &[u8]) -> anyhow::Result<()> {
        if !self.connected {
            anyhow::bail!("not connected");
        }
        self.tx
            .send(data.to_vec())
            .map_err(|_| anyhow::anyhow!("channel closed"))?;
        Ok(())
    }

    async fn receive(&self) -> anyhow::Result<Vec<u8>> {
        if !self.connected {
            anyhow::bail!("not connected");
        }
        let mut rx = self.rx.lock().unwrap_or_else(|e| e.into_inner());
        rx.try_recv()
            .map_err(|_| anyhow::anyhow!("no data available"))
    }

    async fn disconnect(&mut self) -> anyhow::Result<()> {
        self.connected = false;
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected
    }
}

/// WebSocket 传输实现
pub struct WebSocketTransport {
    writer: Option<
        tokio::sync::Mutex<
            futures_util::stream::SplitSink<
                tokio_tungstenite::WebSocketStream<
                    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
                >,
                tokio_tungstenite::tungstenite::Message,
            >,
        >,
    >,
    reader: Option<
        tokio::sync::Mutex<
            futures_util::stream::SplitStream<
                tokio_tungstenite::WebSocketStream<
                    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
                >,
            >,
        >,
    >,
    connected: bool,
}

impl WebSocketTransport {
    pub fn new() -> Self {
        Self {
            writer: None,
            reader: None,
            connected: false,
        }
    }
}

#[async_trait]
impl Transport for WebSocketTransport {
    async fn connect(&mut self, url: &str) -> anyhow::Result<()> {
        use futures_util::StreamExt;

        let (ws_stream, _) = tokio_tungstenite::connect_async(url)
            .await
            .map_err(|e| anyhow::anyhow!("websocket connect failed: {}", e))?;

        let (writer, reader) = ws_stream.split();
        self.writer = Some(tokio::sync::Mutex::new(writer));
        self.reader = Some(tokio::sync::Mutex::new(reader));
        self.connected = true;
        Ok(())
    }

    async fn send(&self, data: &[u8]) -> anyhow::Result<()> {
        use futures_util::SinkExt;

        let writer = self
            .writer
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("not connected"))?;
        let mut guard = writer.lock().await;
        guard
            .send(tokio_tungstenite::tungstenite::Message::binary(
                data.to_vec(),
            ))
            .await
            .map_err(|e| anyhow::anyhow!("websocket send failed: {}", e))?;
        Ok(())
    }

    async fn receive(&self) -> anyhow::Result<Vec<u8>> {
        use futures_util::StreamExt;

        let reader = self
            .reader
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("not connected"))?;
        let mut guard = reader.lock().await;
        match guard.next().await {
            Some(Ok(msg)) => Ok(msg.into_data().to_vec()),
            Some(Err(e)) => Err(anyhow::anyhow!("websocket receive failed: {}", e)),
            None => Err(anyhow::anyhow!("websocket connection closed")),
        }
    }

    async fn disconnect(&mut self) -> anyhow::Result<()> {
        use futures_util::SinkExt;

        if let Some(ref writer) = self.writer {
            let mut guard = writer.lock().await;
            let _ = guard
                .send(tokio_tungstenite::tungstenite::Message::Close(None))
                .await;
        }
        self.writer = None;
        self.reader = None;
        self.connected = false;
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn websocket_binary_roundtrip_preserves_payload() {
        use futures_util::{SinkExt, StreamExt};
        use std::time::Duration;

        const PAYLOAD: &[u8] = b"\x00\xffbinary-sync\x80";
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/sync", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut websocket = tokio_tungstenite::accept_async(socket).await.unwrap();
            let message = websocket.next().await.unwrap().unwrap();
            assert!(message.is_binary());
            assert_eq!(message.clone().into_data().as_ref(), PAYLOAD);
            websocket.send(message).await.unwrap();
            assert!(websocket.next().await.unwrap().unwrap().is_close());
        });

        let mut transport = WebSocketTransport::new();
        transport.connect(&url).await.unwrap();
        assert!(transport.is_connected());
        transport.send(PAYLOAD).await.unwrap();
        let received = tokio::time::timeout(Duration::from_secs(5), transport.receive())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(received, PAYLOAD);
        transport.disconnect().await.unwrap();
        assert!(!transport.is_connected());
        tokio::time::timeout(Duration::from_secs(5), server)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn connect_sets_flag() {
        let (mut a, _b) = create_transport_pair();
        assert!(!a.is_connected());
        a.connect("mock://").await.unwrap();
        assert!(a.is_connected());
    }

    #[tokio::test]
    async fn send_receive_works() {
        let (mut a, mut b) = create_transport_pair();
        a.connect("mock://").await.unwrap();
        b.connect("mock://").await.unwrap();
        a.send(b"hello").await.unwrap();
        let data = b.receive().await.unwrap();
        assert_eq!(data, b"hello");
    }

    #[tokio::test]
    async fn disconnect_works() {
        let (mut a, _b) = create_transport_pair();
        a.connect("mock://").await.unwrap();
        a.disconnect().await.unwrap();
        assert!(!a.is_connected());
    }

    #[tokio::test]
    async fn send_not_connected_fails() {
        let (a, _b) = create_transport_pair();
        let result = a.send(b"data").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn paired_bidirectional() {
        let (mut a, mut b) = create_transport_pair();
        a.connect("mock://").await.unwrap();
        b.connect("mock://").await.unwrap();
        a.send(b"from-a").await.unwrap();
        b.send(b"from-b").await.unwrap();
        assert_eq!(b.receive().await.unwrap(), b"from-a");
        assert_eq!(a.receive().await.unwrap(), b"from-b");
    }

    #[tokio::test]
    async fn large_data_transport() {
        let (mut a, mut b) = create_transport_pair();
        a.connect("mock://").await.unwrap();
        b.connect("mock://").await.unwrap();
        let big = vec![0xFFu8; 100_000];
        a.send(&big).await.unwrap();
        let received = b.receive().await.unwrap();
        assert_eq!(received.len(), 100_000);
    }
}
