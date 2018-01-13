
use std::collections::{HashSet,HashMap};
use std::net::ToSocketAddrs;
use std::vec::Drain;
use std::thread;
use std::net::{TcpStream, TcpListener};
use std::marker::PhantomData;
use std::io::{Read,Write};
use std::io;

use magnetic::buffer::dynamic::DynamicBuffer;
use magnetic::{Producer, Consumer};
use magnetic::spsc::{SPSCConsumer,SPSCProducer,spsc_queue};
use magnetic::mpsc::{MPSCConsumer,MPSCProducer,mpsc_queue};
use magnetic::{PopError, TryPopError, PushError, TryPushError};

use messaging::*;

use trailing_cell::{TakesMessage,TcReader,TcWriter};


use byteorder::{ReadBytesExt, WriteBytesExt, LittleEndian};


#[derive(Eq, PartialEq, Copy, Clone, Hash, Debug, Serialize, Deserialize)]
pub struct ClientId(u32);


////////////////////////////////////////////////////////////////////////////////

// struct ClientConnection {
//     stream: TcpStream,
//     cid: ClientId,
// }

// impl TakesMessage<ClientConnection> for HashMap<ClientId, TcpStream> {
//     fn take_message(&mut self, c: &ClientConnection) {
//         if let Ok(s) = c.stream.try_clone() {
//             self.insert(c.cid, s);
//         }
//     }
// }

type TrailingStreams = TcReader<HashMap<ClientId, TcpStream>, StateChange>;
type SpscPair<S> = (SPSCProducer<S, DynamicBuffer<S>>, SPSCConsumer<S, DynamicBuffer<S>>);
type MpscPair<S> = (MPSCProducer<S, DynamicBuffer<S>>, MPSCConsumer<S, DynamicBuffer<S>>);

// fn read_one<M>(stream: &mut TcpStream, buf: &mut [u8]) -> Option<M>
// where M: Message {
//     if let Ok(pre_len_buf) = stream.read_u16::<LittleEndian>() {
//         let limited_buf: &mut [u8] = &mut buf[..(pre_len_buf as usize)];
//         stream.read_exact(limited_buf);
//         M::deserialize(limited_buf)
//     } else { None }
// }

// fn write_one<M>(stream: &mut TcpStream, msg: &M) -> bool
// where M: Message {
//     let data: Vec<u8> = msg.serialize();
//     stream.write_u32::<LittleEndian>(data.len() as u32);
//     stream.write(&data[..]).is_ok()
// }

////////////////////////////////////////////////////////////////////////////////
//CLIENT

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum InternalMessage {
    // GiveSalt(u32), //TODO
    LoginRequest(String, String),
    LoginAcceptance(ClientId),
    Err(InternalError),
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
enum InternalError {
    Auth(AuthenticationError),
    AlreadyLoggedIn,
    ParsingError,
    LoginRejected,
}
impl Message for InternalMessage {}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
enum AuthenticationError {
    UnknownUsername,
    PasswordMismatch 
}

// impl Message for InternalMessage {

// }

impl From<io::Error> for ClientStartError {
    fn from(t: io::Error) -> Self {
        //TODO ??
        ClientStartError::ConnectFailed
    }
}

pub enum ClientStartError {
    ConnectFailed,
    LoginFailed,
    UnexpectedInternal(InternalMessage),
}


pub fn client_start<C,S,T>(addr: T, user: &str, pass: &str)
-> Result<(ServerwardSender<S>, Receiver<SPSCConsumer<C,DynamicBuffer<C>>,C>, ClientId), ClientStartError>
where
    C: Clientward + 'static,
    S: Serverward, 
    T: ToSocketAddrs, {
    // create TcpStream
    // connect to server
    let mut stream = TcpStream::connect(addr)?;

    // complete handshake & get clientId
    // create Cward producer / consumer
    let login_msg = InternalMessage::LoginRequest(user.to_owned(), pass.to_owned());
    stream.single_write(login_msg);

    let mut lil_buffer = [0_u8; 32];
    let response = stream.single_read::<InternalMessage>(&mut lil_buffer[..])?;
    if let InternalMessage::LoginAcceptance(cid) = response {
        // start up ONE listener thread, give it the PRODUCER handle
        // return CONSUMER handle + naked socket for writing
        let (p, c) : SpscPair<C> = spsc_queue(DynamicBuffer::new(32).unwrap());
        let mut stream_clone = stream.try_clone()?;
        let _ = thread::spawn(move || {
            let mut buffer = [0u8; 1024];
            //pulls messages off the line, PRODUCES them
            while let Ok(msg) = stream_clone.single_read(&mut buffer) {
                p.push(msg);
            }
        });
        Ok((
        ServerwardSender { stream: stream, _phantom: PhantomData::default() },
        Receiver { consumer: c, _phantom: PhantomData::default() },
        cid,
    ))
    } else {
        Err(ClientStartError::UnexpectedInternal(response))
    }
} 





pub trait Authenticator {
    //TODO
}

pub enum ServerStartError {
    BindFailed,
}

impl From<io::Error> for ServerStartError {
    fn from(t: io::Error) -> Self {
        //TODO ??
        ServerStartError::BindFailed
    }
}

enum StateChange {
    Join(ClientId, TcpStream),
    Leave(ClientId),
}

impl TakesMessage<StateChange> for HashMap<ClientId, TcpStream> {
    fn take_message(&mut self, msg: &StateChange) {
        match msg {
            &StateChange::Join(cid, stream) => self.insert(cid, stream),
            &StateChange::Leave(cid) => self.remove(&cid),
        };
    }
}

impl Clone for StateChange {
    fn clone(&self) -> Self {
        match self {
            &StateChange::Join(cid, stream) => {
                StateChange::Join(
                    cid,
                    stream.try_clone().unwrap(), // unwrap!! :( TODO
                )
            },
            &StateChange::Leave(cid) => StateChange::Leave(cid),
        }
    }
}

type ServRes<C,S> = (
    ClientwardSender<C>,
    Receiver<MPSCConsumer<Signed<S>, DynamicBuffer<Signed<S>>>, Signed<S>>,
);

pub fn server_start<A,C,S,T>(addr: T, auth: A)
 -> Result<ServRes<C,S>, ServerStartError>
where
    A: Authenticator,
    C: Clientward,
    S: Serverward + 'static, 
    T: ToSocketAddrs, {
    // create TcpListener
    let listener = TcpListener::bind(addr)?;
    let (p, c) : MpscPair<Signed<S>> = mpsc_queue(DynamicBuffer::new(32).unwrap());

    // keeps track of stream objects
    let w : TcWriter<StateChange> = TcWriter::new(16);
    let mut r = w.add_reader(HashMap::new());


    thread::spawn(move || {
        //acceptor thread
        for maybe_stream in listener.incoming() {
            if let Ok(stream) = maybe_stream {
                let mut stream_clone = stream.try_clone().unwrap();
                thread::spawn(move || {
                    //fwder thread
                    let mut buffer = [0u8; 1024];
                    //pulls messages off the line, PRODUCES them
                    while let Ok(msg) = stream_clone.single_read(&mut buffer) {
                        p.push(Signed::new(msg, ClientId(0)));
                    }
                });
            }
        }
    });
    Ok ((
        ClientwardSender { streams: r, _phantom: PhantomData::default() },
        Receiver { consumer: c, _phantom: PhantomData::default() },
    ))
} 




// TODO pub fn coupler() -> (...)


pub struct ServerwardSender<S: Serverward> {
	stream: TcpStream,
    _phantom: PhantomData<S>,
}
impl<S> ServerwardSender<S> 
where S: Serverward {
    
}


#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct Signed<M> {
    msg: M,
    signature: ClientId,
}
impl<M> Message for Signed<M> where M: Message {}
impl<M> Signed<M>
where M: Message {
    pub fn new(msg: M, signature: ClientId) -> Self {
        Signed {
            msg: msg,
            signature: signature,
        }
    }
}

// S only
// runs in local thread
pub struct ClientwardSender<C: Clientward> {
	streams: TrailingStreams,
    _phantom: PhantomData<C>,
}
impl<C> ClientwardSender<C> 
where C: Clientward {
    pub fn send_to(&mut self, msg: C, cid: ClientId) -> bool {
        unimplemented!()
    }

    pub fn send_to_all(&mut self, msg: C, cids: &HashSet<ClientId>) {
        unimplemented!()
    }
}



//same for C and S
//NOT cloneble
//thread runs until a writer shuts it down
pub struct Receiver<C,M>
where
    C: Consumer<M>,
    M: Message {
    consumer: C,
    _phantom: PhantomData<M>,
}
impl<C,M> Receiver<C,M> 
where
    C: Consumer<M>,
    M: Message {
    pub fn recv_blocking(&mut self) -> Result<M, PopError> {
        self.consumer.pop()
    }

    pub fn try_recv(&mut self) -> Result<M, TryPopError> {
        self.consumer.try_pop()
    }
}