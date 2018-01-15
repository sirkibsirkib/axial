use bincode;
use std::collections::{HashSet,HashMap};
use std::net::ToSocketAddrs;
use std::thread;
use std::net::{TcpStream, TcpListener};
use std::marker::PhantomData;
use std::io;
use std::sync::Arc;

extern crate trailing_cell;
use self::trailing_cell::{TakesMessage,TcReader,TcWriter};

use magnetic::buffer::dynamic::DynamicBuffer;
use magnetic::{Producer};
use magnetic::mpsc::{MPSCConsumer,MPSCProducer,mpsc_queue};




use common::*;
use messaging::*;



#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct Signed<M> {
    pub msg: M,
    pub signature: ClientId,
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

//TODO
trait Server<C: Clientward> {
    fn send_to(&mut self, &C, ClientId) -> bool;
    fn send_to_some<'a, I>(&mut self, &C, I) -> u32 where I: Iterator<Item = &'a ClientId>;
    fn send_to_all(&mut self, &C) -> u32;
    fn shutdown(&mut self);
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


////////////////////////////////////////////////////////////////////////////////

type ServRes<C,S> = (
    ClientwardSender<C>,
    Receiver<MPSCConsumer<Signed<S>, DynamicBuffer<Signed<S>>>, Signed<S>>,
    ServerControl<C,S>,
);

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
    let sender_reader = w.add_reader(HashMap::new());
    Ok ((
        ClientwardSender { streams: sender_reader, _phantom: PhantomData::default() },
        ::common::new_receiver(c),
        ServerControl::new(w, listener, a_p),
    ))
} 

//TODO make coupler


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