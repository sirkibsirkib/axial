use bincode;
use std::collections::{HashSet,HashMap};
use std::net::ToSocketAddrs;
use std::thread;
use std::net::{TcpStream, TcpListener};
use std::marker::PhantomData;
use std::io;
use std::time::Duration;
use std::sync::Arc;

use magnetic::buffer::dynamic::DynamicBuffer;
use magnetic::{Producer, Consumer};
use magnetic::spsc::{SPSCConsumer,SPSCProducer,spsc_queue};
use magnetic::mpsc::{MPSCConsumer,MPSCProducer,mpsc_queue};
use magnetic::{PopError, TryPopError};


use trailing_cell::{TakesMessage,TcReader,TcWriter};


use messaging::*;


//////////////////////////// RETURN TYPES & API ////////////////////////////////

#[derive(Eq, PartialEq, Copy, Clone, Hash, Debug, Serialize, Deserialize)]
pub struct ClientId(pub u32);

pub trait Serverward: Message {}
pub trait Clientward: Message {}

pub trait Authenticator: Send {
    fn try_authenticate(&mut self, user: &str, pass: &str)
     -> Result<ClientId, AuthenticationError>;
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct Signed<M> {
    pub msg: M,
    pub signature: ClientId,
}

pub struct ServerControl<C,S>
where
C: Clientward,
S: Serverward + 'static {
    w: TcWriter<StateChange>,
    streams: TrailingStreams,
    dead: bool,
    acceptor_buffer: [u8; 256],
    listener: TcpListener,
    _phantom: PhantomData<C>,
    producer: Arc<MPSCProducer<Signed<S>, DynamicBuffer<Signed<S>>>>,
}
impl<C,S> ServerControl<C,S>
where C: Clientward, S: Serverward + 'static {
    fn new(w: TcWriter<StateChange>, listener: TcpListener, producer: Arc<MPSCProducer<Signed<S>, DynamicBuffer<Signed<S>>>>) -> Self {
        let r = w.add_reader(HashMap::new());
        ServerControl {
            w: w,
            streams: r,
            dead: false,
            acceptor_buffer: [0u8; 256],
            listener: listener,
            _phantom: PhantomData::default(),
            producer: producer,
        }
    }

    pub fn accept_all<A: Authenticator>(&mut self, auth: &mut A) {
        while self.accept_one(auth) {}
    }

    pub fn accept_one<A: Authenticator>(&mut self, auth: &mut A) -> bool {
        if let Ok((mut stream, _)) = self.listener.accept() {
            if server_handshake(auth, &mut stream, &mut self.acceptor_buffer, &mut self.w, &mut self.streams) {
                let mut stream_clone = stream.try_clone().unwrap();
                let mut a_p_clone = self.producer.clone();
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
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    pub fn kick(&mut self, cid: ClientId) -> bool {
        self.streams.update();
        if self.dead {return false}
        if self.streams.get_mut().contains_key(&cid) {
            self.w.apply_change(StateChange::Leave(cid));
            true
        } else {
            false
        }
    }

    fn shutdown_wrapper(&mut self) {
        self.streams.update();
        for stream in self.streams.get_mut().iter_mut() {
            let _ = stream.1.shutdown(::std::net::Shutdown::Both); //TODO
        }
        self.dead = true;
    }

    pub fn shutdown(mut self) {
        self.shutdown_wrapper();
    }

    pub fn connected_clients(&mut self) -> HashSet<ClientId> {
        if self.dead {return HashSet::new()}
        self.streams.update();
        self.streams.get_mut().keys().map(|x| *x).collect()
    }

    pub fn client_is_connected(&mut self, cid: ClientId) -> bool {
        if self.dead {return false}
        self.streams.update();
        self.streams.get_mut().contains_key(&cid)
    }
}
impl<C,S> Drop for ServerControl<C,S>
where
C: Clientward,
S: Serverward + 'static {
    fn drop(&mut self) {
        self.shutdown_wrapper();
    }
}


pub struct ServerwardSender<S: Serverward> {
	stream: TcpStream,
    _phantom: PhantomData<S>,
}
impl<S> ServerwardSender<S> 
where
S: Serverward {
    pub fn send(&mut self, msg: &S) -> bool {
        self.stream.single_write(msg).is_ok()
    }

    pub fn shutdown(&mut self) {
        let _ = self.stream.shutdown(::std::net::Shutdown::Both); //TODO
    }
}

impl<M> Message for Signed<M> where M: Message {}
impl<M> Signed<M>
where
M: Message {
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
where
C: Clientward {
    pub fn send_to(&mut self, msg: &C, cid: ClientId) -> bool {
        self.streams.update();
        if let Some(stream) = self.streams.get_mut().get_mut(&cid) {
            stream.single_write(&msg).is_ok()
        } else {
            false
        }
    }

    pub fn send_to_set(&mut self, msg: &C, cids: &HashSet<ClientId>) -> u32 {
        let bytes = bincode::serialize(msg, bincode::Infinite).expect("went kk lel");
        self.streams.update();
        let borrow = self.streams.get_mut();
        let mut successes = 0;
        for cid in cids.iter() {
            if let Some(stream) = borrow.get_mut(cid) {
                if stream.single_write_bytes(&bytes).is_ok() {
                    successes += 1;
                }
            }
        }
        successes
    }

    pub fn send_to_all(&mut self, msg: &C) -> u32 {
        let bytes = bincode::serialize(msg, bincode::Infinite).expect("went kk lel");
        self.streams.update();
        let mut successes = 0;
        for stream in self.streams.get_mut().values_mut() {
            if stream.single_write_bytes(&bytes).is_ok() {
                successes += 1;
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

/////////////////////// MESSAGING & ERROR ENUMS ////////////////////////////////

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
    #[inline]
    fn from(_: io::Error) -> Self {
        ClientStartError::ConnectFailed
    }
}
impl From<MessageError> for ClientStartError {
    #[inline]
    fn from(_: MessageError) -> Self {
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


#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum ServerStartError {
    BindFailed,
}
impl From<io::Error> for ServerStartError {
    #[inline]
    fn from(_: io::Error) -> Self {
        //TODO ??
        ServerStartError::BindFailed
    }
}
impl From<MessageError> for ServerStartError {
    #[inline]
    fn from(_: MessageError) -> Self {
        //TODO ??
        ServerStartError::BindFailed
    }
}

///////////////////////// PRIVATE HELPERS //////////////////////////////////////

type TrailingStreams = TcReader<HashMap<ClientId, TcpStream>, StateChange>;
type SpscPair<S> = (SPSCProducer<S, DynamicBuffer<S>>, SPSCConsumer<S, DynamicBuffer<S>>);
type MpscPair<S> = (MPSCProducer<S, DynamicBuffer<S>>, MPSCConsumer<S, DynamicBuffer<S>>);

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
    ServerControl<C,S>,
);

fn client_connect<T: ToSocketAddrs>(addr: T, connect_timeout: Option<Duration>) -> Result<TcpStream, ClientStartError> {
    match connect_timeout {
        Some(duration) => {
            for a in addr.to_socket_addrs().unwrap()   {
                let x = TcpStream::connect_timeout(&a, duration)?;
                return Ok(x);
            }
            Err(ClientStartError::ConnectFailed)
        },
        None => {
            Ok(TcpStream::connect(addr)?)
        },
    }
}

///////////////////////////// FUNCTIONS ////////////////////////////////////////

pub fn client_start<C,S,T>(addr: T, user: &str, pass: &str, connect_timeout: Option<Duration>)
-> Result<(ServerwardSender<S>, Receiver<SPSCConsumer<C,DynamicBuffer<C>>,C>, ClientId), ClientStartError>
where
    C: Clientward + 'static,
    S: Serverward, 
    T: ToSocketAddrs, {
    // create TcpStream
    // connect to server
    
    let mut stream = client_connect(addr, connect_timeout)?;

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
                    if let Err(_) = p.push(msg) {
                        //failed to write to stream
                        return;
                    }
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


pub fn server_start<A,C,S>(addr: A)
 -> Result<ServRes<C,S>, ServerStartError>
where
A: ToSocketAddrs,
C: Clientward,
S: Serverward + 'static, {
    // create TcpListener
    let listener = TcpListener::bind(addr)?;
    let (p, c) : MpscPair<Signed<S>> = mpsc_queue(DynamicBuffer::new(32).unwrap());
    let a_p = Arc::new(p);

    // keeps track of stream objects
    let w : TcWriter<StateChange> = TcWriter::new(16);
    // let mut listener_reader = w.add_reader(HashMap::new());
    let sender_reader = w.add_reader(HashMap::new());
    // let mut control_reader = w.add_reader(HashMap::new());


    // thread::spawn(move || {
    //     //acceptor thread
    //     let mut acceptor_buffer = [0u8; 512];
    //     for maybe_stream in listener.incoming() {
    //         if let Ok(mut stream) = maybe_stream {
    //             if server_handshake(&mut auth, &mut stream, &mut acceptor_buffer, &mut w, &mut listener_reader) {
    //                 let mut stream_clone = stream.try_clone().unwrap();
    //                 let mut a_p_clone = a_p.clone();
    //                 thread::spawn(move || {
    //                     //fwder thread
    //                     let mut buffer = [0u8; 1024];
    //                     //pulls messages off the line, PRODUCES them
    //                     while let Ok(msg) = stream_clone.single_read(&mut buffer) {
    //                         if let Err(_) = a_p_clone.push(Signed::new(msg, ClientId(0))) {
    //                             // write to queue failed
    //                             return;
    //                         }
    //                     }
    //                     // socket closed!
    //                 });
    //             }
    //         }
    //     }
    // });
    Ok ((
        ClientwardSender { streams: sender_reader, _phantom: PhantomData::default() },
        Receiver { consumer: c, _phantom: PhantomData::default() },
        ServerControl::new(w, listener, a_p),
        // ServerControl {streams: control_reader, dead: false, w: TcWriter::new(16)},
    ))
} 

// TODO pub fn coupler() -> (...)


////////////////////////////// AUX FUNCTIONS ///////////////////////////////////

fn server_handshake<A>(auth: &mut A, stream: &mut TcpStream, acceptor_buffer: &mut [u8], w: &mut TcWriter<StateChange>, r: &mut TrailingStreams) -> bool
where
    A: Authenticator, {
    let mut success = false;
    if let Ok(MetaServerward::LoginRequest(user, pass)) = stream.single_read(acceptor_buffer) {
        let reply = match auth.try_authenticate(&user, &pass) {
            Ok(cid) => {
                r.update();
                if r.get_mut().contains_key(&cid) {
                    MetaClientward::AuthenticationError(AuthenticationError::AlreadyLoggedIn)
                } else {
                    w.apply_change(StateChange::Join(cid, stream.try_clone().unwrap())); //TODO
                    success = true;
                    MetaClientward::LoginAcceptance(cid)
                }
            },
            Err(e) => MetaClientward::AuthenticationError(e),
        };
        stream.single_write(&reply).expect("scurvy server handshake failed");
    }
    success
}