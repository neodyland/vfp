use std::{collections::HashMap, sync::Arc, fmt::Display};

use tokio::{sync::{mpsc, Mutex}, task::JoinHandle};

use super::tcp;

#[derive(Debug)]
pub enum NxError {
    Io(tcp::IoError),
    Closed,
    IdFormatInvalid,
}

impl Display for NxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NxError::Io(e) => write!(f, "IoError: {}", e),
            NxError::Closed => write!(f, "Closed"),
            NxError::IdFormatInvalid => write!(f, "IdFormatInvalid"),
        }
    }
}

pub async fn connect(addr: &str) -> Result<NxClient,NxError> {
    NxClient::new(addr).await
}

pub async fn listen(port: &u16) -> Result<NxListener,NxError> {
    NxListener::listen(port).await
}

fn append_id(id: u128, message: Vec<u8>) -> Vec<u8> {
    let mut message = message;
    message.splice(0..0, id.to_be_bytes().iter().cloned());
    message
}

fn shift_id(mut message: Vec<u8>) -> Result<(u128,Vec<u8>),NxError> {
    if message.len() < 16 {
        return Err(NxError::IdFormatInvalid);
    }
    let id = message.drain(0..16).collect::<Vec<u8>>();
    if id.len() != 16 {
        return Err(NxError::IdFormatInvalid);
    }
    let id = u128::from_be_bytes(id.as_slice().try_into().unwrap());
    Ok((id, message))
}

pub type NxPoolHashMap = HashMap<u128, mpsc::Sender<Vec<u8>>>;
pub type NxPool = Arc<Mutex<NxPoolHashMap>>;

pub struct NxBoxReader {
    pub id: u128,
    pub receiver: mpsc::Receiver<Vec<u8>>,
}

impl NxBoxReader {
    pub fn new(id: u128, receiver: mpsc::Receiver<Vec<u8>>) -> Self {
        Self {
            id,
            receiver,
        }
    }
    pub async fn read(&mut self) -> Result<Vec<u8>,NxError> {
        match self.receiver.recv().await {
            Some(message) => Ok(message),
            None => Err(NxError::Closed),
        }
    }
}

pub enum NxWriteChannel {
    Write(Vec<u8>),
    Close,
}

pub struct NxBoxWriter {
    pub id: u128,
    pub writechannel: mpsc::Sender<NxWriteChannel>,
}

impl NxBoxWriter {
    pub fn new(id: u128, writechannel: mpsc::Sender<NxWriteChannel>) -> Self {
        Self {
            id,
            writechannel,
        }
    }
    pub async fn write(&mut self, mut message: Vec<u8>) -> Result<(),NxError> {
        message = append_id(self.id, message);
        self.writechannel.send(NxWriteChannel::Write(message)).await.map_err(|_| NxError::Closed)
    }
    pub async fn close(self) -> Result<(),NxError> {
        self.writechannel.send(NxWriteChannel::Close).await.map_err(|_| NxError::Closed)
    }
}

pub struct NxBoxIo {
    pub id: u128,
    pub writer: NxBoxWriter,
    pub reader: NxBoxReader,
}

impl NxBoxIo {
    pub fn new(id: u128, writer: NxBoxWriter, reader: NxBoxReader) -> Self {
        Self {
            id,
            writer,
            reader,
        }
    }
    pub async fn read(&mut self) -> Result<Vec<u8>,NxError> {
        self.reader.read().await
    }
    pub async fn write(&mut self, message: Vec<u8>) -> Result<(),NxError> {
        self.writer.write(message).await
    }
    pub async fn close(self) -> Result<(),NxError> {
        self.writer.close().await
    }
}

pub struct NxReader {
    pool: NxPool,
    receiver: tokio::task::JoinHandle<()>,
    newidreceiver: mpsc::Receiver<(u128,Vec<u8>)>,
}

impl NxReader {
    pub fn new(mut reader: tcp::TcpReader) -> Self {
        let pool = Arc::new(Mutex::new(HashMap::new() as NxPoolHashMap));
        let pool_clone = pool.clone();
        let (newidnotify, newidreceiver) = mpsc::channel(32);
        let receiver = tokio::spawn(async move {
            loop {
                let message = match reader.read().await {
                    Some(message) => message,
                    None => {
                        break;
                    }
                };
                let message = match message {
                    Ok(message) => message,
                    Err(_) => {
                        continue;
                    }
                };
                let (id, message) = match shift_id(message.to_vec()) {
                    Ok((id, message)) => (id, message),
                    Err(_) => {
                        continue;
                    }
                };
                let mut pool = pool_clone.lock().await;
                let sender = match pool.get_mut(&id) {
                    Some(sender) => sender,
                    None => {
                        newidnotify.send((id,message)).await.unwrap();
                        continue;
                    }
                };
                match sender.send(message).await {
                    Ok(_) => {}
                    Err(_) => {
                        continue;
                    }
                }
            }
        });
        Self {
            pool,
            receiver,
            newidreceiver,
        }
    }
    pub async fn open_send(&mut self, id: u128, message: Vec<u8>) -> mpsc::Receiver<Vec<u8>> {
        let (sender, receiver) = mpsc::channel(32);
        let mut pool = self.pool.lock().await;
        match sender.send(message).await {
            Ok(_) => {}
            Err(_) => {
            }
        };
        pool.insert(id, sender);
        receiver
    }
    pub async fn open(&mut self, id: u128) -> mpsc::Receiver<Vec<u8>> {
        let (sender, receiver) = mpsc::channel(32);
        let mut pool = self.pool.lock().await;
        pool.insert(id, sender);
        receiver
    }
    pub async fn next<'a>(&'a mut self, writer: &'a mut NxWriter) -> Option<NxBoxIo> {
        let id = self.newidreceiver.recv().await;
        let (id,message) = match id {
            Some(id) => id,
            None => return None,
        };
        let reader = self.open_send(id, message).await;
        let reader = NxBoxReader::new(id, reader);
        let writer = writer.sender.clone();
        let writer = NxBoxWriter::new(id, writer);
        Some(NxBoxIo::new(id, writer, reader))
    }
    pub async fn close_id(&mut self, id: u128) {
        let mut pool = self.pool.lock().await;
        pool.remove(&id);
    }
    pub fn close(&mut self) {
        self.receiver.abort();
    }
}

pub struct NxWriter {
    pub receiver: JoinHandle<()>,
    pub sender: mpsc::Sender<NxWriteChannel>,
}

impl NxWriter {
    pub fn new(mut writer: tcp::TcpWriter) -> Self {
        let (sender, mut receiver) = mpsc::channel::<NxWriteChannel>(32);
        let receiver = tokio::spawn(async move {
            loop {
                let message = match receiver.recv().await {
                    Some(message) => message,
                    None => {
                        break;
                    }
                };
                match message {
                    NxWriteChannel::Write(message) => {
                        match writer.send(message).await {
                            Ok(_) => {}
                            Err(_) => {
                                break;
                            }
                        }
                    }
                    NxWriteChannel::Close => {
                        break;
                    }
                }
            }
        });
        Self {
            sender,
            receiver,
        }
    }
    pub async fn close(&mut self) -> Result<(),NxError> {
        self.sender.send(NxWriteChannel::Close).await.map_err(|_| NxError::Closed)
    }
}

pub struct NxSession {
    writer: NxWriter,
    reader: NxReader,
}

impl NxSession {
    pub fn new(reader: NxReader, writer: NxWriter) -> Self {
        Self {
            writer,
            reader,
        }
    }
    pub async fn next(&mut self) -> Option<NxBoxIo> {
        self.reader.next(&mut self.writer).await
    }
}

pub struct NxClient {
    writer: NxWriter,
    reader: NxReader,
}

impl NxClient {
    pub async fn new(addr: &str) -> Result<Self,NxError> {
        let io = tcp::connect(addr).await;
        let io = match io {
            Ok(io) => io,
            Err(e) => return Err(NxError::Io(e)),
        };
        let (reader, writer) = io;
        let reader = NxReader::new(reader);
        let writer = NxWriter::new(writer);
        Ok(Self {
            writer,
            reader,
        })
    }
    pub async fn open(&mut self) -> Result<NxBoxIo,NxError> {
        let id = uuid::Uuid::new_v4().as_u128();
        let reader = NxBoxReader::new(id, self.reader.open(id).await);
        let sender = self.writer.sender.clone();
        let writer = NxBoxWriter::new(id, sender);
        let client = NxBoxIo::new(id, writer, reader);
        Ok(client)
    }
    pub async fn close(&mut self) -> Result<(),NxError> {
        self.writer.close().await
    }
}
pub struct NxListener {
    listener: tcp::TcpListener,
}

impl NxListener {
    pub async fn accept(&mut self) -> Result<(NxSession,tcp::TcpAddr),NxError> {
        match self.listener.accept().await {
            Ok((reader,writer,addr)) => Ok((NxSession::new(NxReader::new(reader),NxWriter::new(writer)),addr)),
            Err(e) => Err(NxError::Io(e)),
        }
    }
    pub async fn listen(port: &u16) -> Result<Self,NxError> {
        let listener = match tcp::listen(port).await {
            Ok(listener) => listener,
            Err(e) => {
                return Err(NxError::Io(e));
            }
        };
        Ok(Self { listener })
    }
}
