use bincode;
use std::collections::{HashSet,HashMap};
use std::net::ToSocketAddrs;
use std::thread;
use std::net::{TcpStream, TcpListener};
use std::marker::PhantomData;
use std::io;
use std::time::Duration;
use std::sync::Arc;
use rand;
use rand::Rng;

use magnetic::buffer::dynamic::DynamicBuffer;
use magnetic::{Producer};
use magnetic::mpsc::{MPSCConsumer,MPSCProducer,mpsc_queue};
// use net2::TcpListenerExt;

extern crate trailing_cell;
use self::trailing_cell::{TakesMessage,TcReader,TcWriter};

use common::*;
use messaging::*;

pub const MAX_CLIENT_CHALLENGES: u8 = 3; 


//////////////////////////// RETURN TYPES & API ////////////////////////////////

/// This trait defines the API for any object that sends messages toward a
/// server.
pub trait ClientwardSender<C: Clientward> {

    /// Send the given message to a specific client (blocking)
    fn send_to(&mut self, &C, ClientId) -> bool;

    /// Send the given message to a sequence of clients defined by the given iterator
    fn send_to_sequence<'a, I>(&mut self, &C, I) -> u32 where I: Iterator<Item = &'a ClientId>;

    /// Send the given message once to each client currently conneted
    fn send_to_all(&mut self, &C) -> u32;
    fn online_clients(&mut self) -> HashSet<ClientId>;
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct Signed<M>(pub M, pub ClientId);
impl<M> Message for Signed<M> where M: Message {}
impl<M> Signed<M> where M: Message {
    pub fn new(msg: M, signature: ClientId) -> Self {
        Signed(msg, signature)
    }
}

/// This object is returned as a result of `server_start` and has no analogue on
/// the client-side. This object is the owner of the underlying stream objects.
/// 
/// It offers functions to accept clients, kick clients, and return information
/// about the clients currently connected.
/// 
/// The underlying TcpListener and TcpStreams are owned by this object, and are
/// dropped when this object is dropped or `shutdown`. All listening threads are
/// killed when their respective stream is shutdown.
pub struct ServerControl<C,S>
where C: Clientward, S: Serverward, {
    w: TcWriter<StateChange>,
    streams: TrailingStreams,
    dead: bool,
    acceptor_buffer: [u8; 256],
    listener: TcpListener,
    _phantom: PhantomData<C>,
    producer: Arc<MPSCProducer<Signed<S>, DynamicBuffer<Signed<S>>>>,
}
impl<C,S> ServerControl<C,S>
where C: Clientward, S: Serverward, {
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

    /// 
    pub fn accept_all<A: Authenticator>(&mut self, mut auth: A) {
        loop { self.accept_one(&mut auth); }
    }

    pub fn accept_all_waiting<A: Authenticator>(&mut self, auth: &mut A) -> u32 {
        let mut total = 0;
        if self.listener.set_nonblocking(true).is_ok() {
            while let Some(_) = self.accept_one(auth) { total += 1 }
            let _ = self.listener.set_nonblocking(false);
        }
        total
        
    }

    pub fn accept_one<A: Authenticator>(&mut self, auth: &mut A) -> Option<ClientId> {
        if let Ok((mut stream, _)) = self.listener.accept() {
            if let Some(cid) = server_handshake(auth, &mut stream, &mut self.acceptor_buffer, &mut self.w, &mut self.streams) {
                let mut stream_clone = stream.try_clone().unwrap();
                let mut a_p_clone = self.producer.clone();
                thread::spawn(move || {
                    // forwarder thread. Dies when socket dies
                    let mut buffer = [0u8; 512]; //incoming messages aren't that big
                    while let Ok(msg) = stream_clone.single_read(&mut buffer) {
                        if let Err(_) = a_p_clone.push(Signed::new(msg, cid)) {
                            return;
                        }
                    }
                });
                Some(cid)
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn kick(&mut self, cid: ClientId) -> bool {
        self.streams.update();
        if self.dead {return false}
        if self.streams.contains_key(&cid) {
            self.w.apply_change(StateChange::Leave(cid));
            true
        } else {
            false
        }
    }

    fn shutdown_wrapper(&mut self) { //PRIVATE
        self.streams.update();
        for stream in self.streams.iter_mut() {
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
        self.streams.keys().map(|x| *x).collect()
    }

    pub fn client_is_connected(&mut self, cid: ClientId) -> bool {
        if self.dead {return false}
        self.streams.update();
        self.streams.contains_key(&cid)
    }
}
impl<C,S> Drop for ServerControl<C,S>
where
C: Clientward,
S: Serverward, {
    fn drop(&mut self) {
        self.shutdown_wrapper();
    }
}


// S only
// runs in local thread
pub struct RemoteClientwardSender<C: Clientward> {
	streams: TrailingStreams,
    _phantom: PhantomData<C>,
}
impl<C> ClientwardSender<C> for RemoteClientwardSender<C> 
where
C: Clientward {
    fn send_to(&mut self, msg: &C, cid: ClientId) -> bool {
        self.streams.update();
        if let Some(stream) = self.streams.get_mut(&cid) {
            stream.single_write(&msg).is_ok()
        } else {
            false
        }
    }

    fn send_to_sequence<'a, I>(&mut self, msg: &C, cids: I) -> u32
    where I: Iterator<Item = &'a ClientId> {
        let bytes = bincode::serialize(msg, bincode::Infinite).expect("went kk lel");
        self.streams.update();
        let mut successes = 0;
        for cid in cids {
            if let Some(stream) = self.streams.get_mut(cid) {
                if stream.single_write_bytes(&bytes).is_ok() {
                    successes += 1;
                }
            }
        }
        successes
    }

    fn send_to_all(&mut self, msg: &C) -> u32 {
        let bytes = bincode::serialize(msg, bincode::Infinite).expect("went kk lel");
        self.streams.update();
        let mut successes = 0;
        for stream in self.streams.values_mut() {
            if stream.single_write_bytes(&bytes).is_ok() {
                successes += 1;
            }
        }
        successes
    }
    
    fn online_clients(&mut self) -> HashSet<ClientId> {
        self.streams.keys().map(|x| *x).collect()
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
    RemoteClientwardSender<C>,
    Receiver<Signed<S>>,
    ServerControl<C,S>,
);

pub fn server_start<A,C,S>(addr: A)
 -> Result<ServRes<C,S>, ServerStartError>
where
A: ToSocketAddrs,
C: Clientward,
S: Serverward, {
    // create TcpListener
    let listener = TcpListener::bind(addr)?;
    let (p, c) : MpscPair<Signed<S>> = mpsc_queue(DynamicBuffer::new(128).unwrap());
    let a_p = Arc::new(p);

    // keeps track of stream objects
    let w : TcWriter<StateChange> = TcWriter::new(16);
    let sender_reader = w.add_reader(HashMap::new());
    Ok ((
        RemoteClientwardSender { streams: sender_reader, _phantom: PhantomData::default() },
        ::common::new_receiver(c),
        ServerControl::new(w, listener, a_p),
    ))
} 

//TODO make coupler


////////////////////////////// AUX FUNCTIONS ///////////////////////////////////

lazy_static! {
    static ref ANSWER_DUR: Duration = Duration::from_millis(2000);
}

fn server_handshake<A>(auth: &mut A, stream: &mut TcpStream,
                       acceptor_buffer: &mut [u8], w: &mut TcWriter<StateChange>,
                       r: &mut TrailingStreams) -> Option<ClientId>
where
    A: Authenticator
{
    let mut accepted: Option<ClientId> = None;
    r.update();
    if let Ok(MetaServerward::LoginRequest(user)) = stream.single_read(acceptor_buffer) {
        if let Some((cid, ref secret)) = auth.identity_and_secret(&user) {
            debug_println!("answer was correct! OK");
            for _ in 0..3 {
                // send the client challenge questions, receive answers
                if !challenge_client(stream, acceptor_buffer, secret) { return None }
            }
            if ! r.contains_key(&cid) {
                debug_println!("not already logged in. YAY");
                w.apply_change(StateChange::Join(cid, stream.try_clone().unwrap())); //TODO
                if stream.single_write(& MetaClientward::LoginAcceptance(cid)).is_ok() {
                    accepted = Some(cid);
                }
            } else {
                debug_println!("D: already logged in");
                let _ = stream.single_write(
                    & MetaClientward::AuthenticationError(
                        AuthenticationError::AlreadyLoggedIn));
            }
        } else {
            let _ = stream.single_write(
                & MetaClientward::AuthenticationError(
                    AuthenticationError::UnknownUsername));
        };
    }
    accepted
}

fn challenge_client(stream: &mut TcpStream, buf: &mut [u8], secret: &str) -> bool {
    let challenge: Vec<u8> = random_challenge();
    if stream.single_write(& MetaClientward::ChallengeQuestion(challenge.clone())).is_ok() {
        debug_println!("sent challenge OK");
        if let Ok(answer) = stream.single_timout_silence_read(buf, *ANSWER_DUR) {
            debug_println!("got challenge response OK");
            if let MetaServerward::ChallengeAnswer(ans) = answer {
                debug_println!("challenge response is correct type OK");
                if secret_challenge_hash(secret, &challenge) == ans {
                    return true;
                } else {
                    debug_println!("D: challenge failed");
                    let _ = stream.single_write(
                        & MetaClientward::AuthenticationError(
                            AuthenticationError::ChallengeFailed));
                }
            } else {
                debug_println!("D: challenge response bad type");
                let _ = stream.single_write(& MetaClientward::ClientMisbehaved);
            }
        } else {
            debug_println!("D: timeout on challenge reply");
            let _ = stream.single_write(& MetaClientward::HandshakeTimeout);
        }
    } else {
        debug_println!("D: failed to send challenge");
    }
    false
}

fn random_challenge() -> Vec<u8> {
    (0..15).map(|_| rand::thread_rng().gen()).collect()
}
