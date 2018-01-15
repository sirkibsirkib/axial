
use std::collections::{HashSet,HashMap};
use std::net::ToSocketAddrs;
use std::thread;
use std::net::{TcpStream, TcpListener};
use std::marker::PhantomData;
use std::io;
use std::sync::Arc;

use magnetic::buffer::dynamic::DynamicBuffer;
use magnetic::{Producer, Consumer};
use magnetic::spsc::{SPSCConsumer,SPSCProducer,spsc_queue};
use magnetic::mpsc::{MPSCConsumer,MPSCProducer,mpsc_queue};
use magnetic::{PopError, TryPopError};

use messaging::*;

use trailing_cell::{TakesMessage,TcReader,TcWriter};


#[derive(Eq, PartialEq, Copy, Clone, Hash, Debug, Serialize, Deserialize)]
pub struct ClientId(pub u32);


type TrailingStreams = TcReader<HashMap<ClientId, TcpStream>, StateChange>;
type SpscPair<S> = (SPSCProducer<S, DynamicBuffer<S>>, SPSCConsumer<S, DynamicBuffer<S>>);
type MpscPair<S> = (MPSCProducer<S, DynamicBuffer<S>>, MPSCConsumer<S, DynamicBuffer<S>>);

////////////////////////////////////////////////////////////////////////////////
//CLIENT

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum MetaServerward {
    LoginRequest(String, String),
}
impl Message for MetaServerward {}
impl Serverward for MetaServerward {}


#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum MetaClientward {
    LoginAcceptance(ClientId),
    AuthenticationError(AuthenticationError),
    ClientThresholdReached,
}
impl Message for MetaClientward {}
impl Clientward for MetaClientward {}

impl From<io::Error> for ClientStartError {
    fn from(t: io::Error) -> Self {
        ClientStartError::ConnectFailed
    }
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum ClientStartError {
    ConnectFailed,
    LoginFailed,
    ClientThresholdReached,
    AuthenticationError(AuthenticationError),
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum AuthenticationError {
    AlreadyLoggedIn,
    UnknownUsername,
    PasswordMismatch,
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
    let login_msg = MetaServerward::LoginRequest(user.to_owned(), pass.to_owned());
    //identify self to server
    if stream.single_write(&login_msg).is_err() {
        return Err(ClientStartError::LoginFailed);
    }

    let mut lil_buffer = [0_u8; 64];
    let response = stream.single_read::<MetaClientward>(&mut lil_buffer[..])?;
    use MetaClientward as MC;
    match response {
        MC::LoginAcceptance(cid) => {
            //server responded that login was successful! begin messaging
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
        },
        MC::ClientThresholdReached => Err(ClientStartError::ClientThresholdReached),
        MC::AuthenticationError(err) => Err(ClientStartError::AuthenticationError(err)),
    }
}

pub trait Authenticator: Send {
    fn try_authenticate(&mut self, user: &str, pass: &str)
     -> Result<ClientId, AuthenticationError>;
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
            &StateChange::Join(cid, ref stream) => self.insert(cid, stream.try_clone().unwrap()),
            &StateChange::Leave(cid) => self.remove(&cid),
        };
    }
}

impl Clone for StateChange {
    fn clone(&self) -> Self {
        match self {
            &StateChange::Join(cid, ref stream) => {
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

pub fn server_start<A,T,C,S>(addr: T, mut auth: A)
 -> Result<ServRes<C,S>, ServerStartError>
where
    A: Authenticator + 'static, 
    T: ToSocketAddrs,
    C: Clientward,
    S: Serverward + 'static, {
    // create TcpListener
    let listener = TcpListener::bind(addr)?;
    let (p, c) : MpscPair<Signed<S>> = mpsc_queue(DynamicBuffer::new(32).unwrap());
    let a_p = Arc::new(p);

    // keeps track of stream objects
    let w : TcWriter<StateChange> = TcWriter::new(16);
    let mut r = w.add_reader(HashMap::new());


    thread::spawn(move || {
        //acceptor thread
        let mut acceptor_buffer = [0u8; 512];
        for maybe_stream in listener.incoming() {
            if let Ok(mut stream) = maybe_stream {

                //TODO complete handshake OR drop
                if let Ok(MetaServerward::LoginRequest(user, pass)) = stream.single_read(&mut acceptor_buffer) {
                    let mut success = false;
                    let reply = match auth.try_authenticate(&user, &pass) {
                        Ok(cid) => {
                            r.update();
                            if r.get_mut().contains_key(&cid) {
                                success = true;
                                MetaClientward::LoginAcceptance(cid)
                            } else {
                                MetaClientward::AuthenticationError(AuthenticationError::AlreadyLoggedIn)
                            }
                        },
                        Err(e) => MetaClientward::AuthenticationError(e)
                    };
                    stream.single_write(&reply).expect("scurvy server handshake failed");
                    if !success {
                        continue;
                        //handshake failed
                    }
                    let mut stream_clone = stream.try_clone().unwrap();
                    let mut a_p_clone = a_p.clone();
                    thread::spawn(move || {
                        //fwder thread
                        let mut buffer = [0u8; 1024];
                        //pulls messages off the line, PRODUCES them
                        while let Ok(msg) = stream_clone.single_read(&mut buffer) {
                            if let Err(_) = a_p_clone.push(Signed::new(msg, ClientId(0))) {
                                // write to queue failed
                                return;
                            }
                        }
                        // socket closed!
                    });
                }
            }
        }
    });
    Ok ((
        ClientwardSender { streams: w.add_reader(HashMap::new()), _phantom: PhantomData::default() },
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
    pub fn send(&mut self, msg: &S) -> bool {
        self.stream.single_write(msg).is_ok()
    }
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
        self.streams.update();
        if let Some(stream) = self.streams.get_mut().get_mut(&cid) {
            stream.single_write(&msg).is_ok()
        } else {
            false
        }
    }

    pub fn send_to_all(&mut self, msg: C, cids: &HashSet<ClientId>) -> u32 {
        self.streams.update();
        let borrow = self.streams.get_mut();
        let mut successes = 0;
        for cid in cids.iter() {
            if let Some(stream) = borrow.get_mut(cid) {
                if stream.single_write(&msg).is_ok() {
                    successes += 1;
                }
            }
        }
        successes
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