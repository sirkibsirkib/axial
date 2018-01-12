
use std::collections::{HashSet,HashMap};
use std::net::ToSocketAddrs;
use std::vec::Drain;
use std::thread;
use std::net::{TcpStream, TcpListener};
use std::marker::PhantomData;
use std::io::{Read,Write};
use std::io;

use magnetic::spsc::spsc_queue;
use magnetic::buffer::dynamic::DynamicBuffer;
use magnetic::{Producer, Consumer};
use magnetic::spsc::{SPSCConsumer,SPSCProducer};
use magnetic::{PopError, TryPopError, PushError, TryPushError};

use messaging::*;

use trailing_cell::{TakesMessage,TcReader,TcWriter};


use byteorder::{ReadBytesExt, WriteBytesExt, LittleEndian};


#[derive(Eq, PartialEq, Copy, Clone, Hash, Debug, Serialize, Deserialize)]
pub struct ClientId(u32);


////////////////////////////////////////////////////////////////////////////////

struct ClientConnection {
    stream: TcpStream,
    cid: ClientId,
}

impl TakesMessage<ClientConnection> for HashMap<ClientId, TcpStream> {
    fn take_message(&mut self, c: &ClientConnection) {
        if let Ok(s) = c.stream.try_clone() {
            self.insert(c.cid, s);
        }
    }
}

type TrailingStreams = TcReader<HashMap<ClientId, TcpStream>, ClientConnection>;  
type SpscPair<S> = (SPSCProducer<S, DynamicBuffer<S>>, SPSCConsumer<S, DynamicBuffer<S>>);

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
enum InternalMessage {
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

impl From<io::Error> for InternalError {
    fn from(t: io::Error) -> Self {
        //TODO ??
        InternalError::LoginRejected
    }
}



pub fn client_start<C,S,T>(addr: T, user: &str, pass: &str) -> Result<(ServerwardSender<S>, Receiver<C>, ClientId), InternalError>
where
    C: Clientward,
    S: Serverward + 'static, 
    T: ToSocketAddrs, {
    // create TcpStream
    // connect to server
    let mut stream = TcpStream::connect(addr)?;

    // complete handshake & get clientId
    // create Cward producer / consumer
    let login_msg = InternalMessage::LoginRequest(user.to_owned(), pass.to_owned());
    stream.single_write(login_msg);

    let mut lil_buffer = [0_u8; 32];
    let my_cid: ClientId;
    if let Ok(InternalMessage::LoginAcceptance(cid)) = stream.single_read(&mut lil_buffer[..]) {
        // start up ONE listener thread, give it the PRODUCER handle
        // return CONSUMER handle + naked socket for writing
        let (p, c) : SpscPair<S> = spsc_queue(DynamicBuffer::new(32).unwrap());
        let buf = [0u8; 1024];
        let _ = thread::spawn(move || {
            //CONSUMES e
            while let Ok(msg) = c.pop() {

            }
        });
        Ok((
            ServerwardSender {consumer: c},
            Receiver {stream: stream.try_clone().unwrap(), _phantom: PhantomData::default()},
            my_cid,
        ))
    } else {
        Err(InternalError::LoginRejected)
    }

} 


pub fn server_start<C,S>() -> (ClientwardSender<C>, Receiver<Signed<S>>, ClientId)
where
    C: Clientward,
    S: Serverward, {
    unimplemented!()
}















trait Authenticator {
    //TODO
}




















// TODO pub fn coupler() -> (...)


pub struct ServerwardSender<S: Serverward> {
	consumer: SPSCConsumer<S, DynamicBuffer<S>>,
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

// S only
// runs in local thread
pub struct ClientwardSender<C: Clientward> {
	streams: HashMap<ClientId, TcpStream>,
    phantom: PhantomData<C>,
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
pub struct Receiver<M: Message> {
    producer: SPSCConsumer<M, DynamicBuffer<M>>,
}
impl<M> Receiver<M> 
where M: Message {
    pub fn recv_blocking(&mut self) -> Result<M, PopError> {
        self.producer.pop()
    }

    pub fn try_recv(&mut self) -> Result<M, TryPopError> {
        self.producer.try_pop()
    }

    pub fn drain(&mut self) -> Drain<M> {
        let mut v = vec![];
        while let Ok(msg) = self.producer.try_pop() {
            v.push(msg)
        }
        v.drain(..)
    }
}