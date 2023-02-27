use bytes::{Bytes, BytesMut};
use futures::SinkExt;
use tokio::net::{
    tcp::{OwnedReadHalf, OwnedWriteHalf},
    TcpListener as TokioTcpListener, TcpStream as TokioTcpStream,
};
use tokio_stream::StreamExt;
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};
use std::{io::Error, net::SocketAddr};

type Codec = LengthDelimitedCodec;
pub type IoWrite = FramedWrite<OwnedWriteHalf, Codec>;
pub type IoRead = FramedRead<OwnedReadHalf, Codec>;

pub type TcpAddr = SocketAddr;
pub type IoError = Error;

pub struct TcpWriter {
    write: IoWrite,
}

impl TcpWriter {
    pub fn new(write: OwnedWriteHalf) -> Self {
        let codec = Codec::new();
        Self { write: FramedWrite::new(write, codec) }
    }
    pub async fn send(&mut self, bytes: Vec<u8>) -> Result<(), Error> {
        self.write.send(Bytes::from(bytes)).await
    }
    pub async fn close(&mut self) -> Result<(),Error> {
        self.write.close().await
    }
}

pub struct TcpReader {
    read: IoRead,
}

impl TcpReader {
    pub fn new(read: OwnedReadHalf) -> Self {
        let codec = Codec::new();
        Self { read: FramedRead::new(read, codec) }
    }
    pub async fn read(&mut self) -> Option<Result<BytesMut,Error>> {
        self.read.next().await
    }
}

pub struct TcpListener {
    listener: TokioTcpListener,
}

impl TcpListener {
    pub async fn accept(&mut self) -> Result<(TcpReader,TcpWriter,SocketAddr),Error> {
        match self.listener.accept().await {
            Ok((stream,addr)) => {
                let (reader, writer) = stream.into_split();
                let reader = TcpReader::new(reader);
                let writer = TcpWriter::new(writer);
                return Ok((reader,writer,addr));
            }
            Err(e) => {
                return Err(e);
            }
        }
    }
    pub fn new(listener: TokioTcpListener) -> Self {
        Self { listener }
    }
}

pub async fn listen(port: &u16) -> Result<TcpListener,Error> {
    let addr = format!("0.0.0.0:{}", port);
    let listener = TokioTcpListener::bind(addr).await;
    match listener {
        Ok(listener) => {
            return Ok(TcpListener::new(listener));
        }
        Err(e) => {
            return Err(e);
        }
    }
}

pub async fn connect(addr: &str) -> Result<(TcpReader,TcpWriter),Error> {
    let stream = TokioTcpStream::connect(addr).await;
    match stream {
        Ok(stream) => {
            let (reader, writer) = stream.into_split();
            return Ok((TcpReader::new(reader),TcpWriter::new(writer)));
        }
        Err(e) => {
            return Err(e);
        }
    }
}
