use serde::{Serialize};
use serde::de::DeserializeOwned;
use magnetic::Consumer;
use std::marker::PhantomData;
use magnetic::{PopError, TryPopError};
use crypto_hash::{Algorithm,Hasher};
use std::io::Write;


// macro_rules! debug_println {
//     ($lit:expr, $e:expr) => {{ println!($lit, $e); }};
//     ($e:expr) => {{ println!("{:?}", $e); }};
// }


macro_rules! debug_println {
    ($lit:expr, $e:expr) => ();
    ($e:expr) => ();
}

/// Any object that can be sent by a Receiver or Sender requires this trait to
/// be implemented, to ensure it has the necessary properties.
pub trait Message: Serialize + DeserializeOwned + Clone + 'static {}

/// This marker trait is differentiated from `Clientward`. A message should only
/// be marked as `Serverward` if it is logically sent FROM client TO server.
/// 
/// 
/// This is done to make
/// the internal AND your external code more ergonomic, and make it less likely
/// for the message direction to be accidentally reversed. 
pub trait Serverward: Message {}

/// This marker trait is differentiated from `Serverward`. A message should only
/// be marked as `Clientward` if it is logically sent FROM server TO client.
/// 
/// This is done to make
/// the internal AND your external code more ergonomic, and make it less likely
/// for the message direction to be accidentally reversed.
pub trait Clientward: Message {}

#[derive(Eq, PartialEq, Copy, Clone, Hash, Debug, Serialize, Deserialize)]
/// Particular clients / users correpond with a unqiue ClientId.
pub struct ClientId(pub u32);

/// On the server side, an authenticator does the job of identifying incoming 
/// connections (ie. mapping from username to ClientId), and providinig their
/// expected secret (ie. mapping from username to secret).
/// 
/// Two connections for the same ClientId are not permitted.
/// See the README for details on what makes a good secret at:
/// https://github.com/sirkibsirkib/axial/blob/master/README.md
pub trait Authenticator: Send {

    /// Maps the given username to (c, s) where `c` is the client's ClientId
    /// and `s` is the client's secret. These values determine how messages from
    /// this client will be identified, AND prevent the client from connecting 
    /// twice. The secret is part of the secure handshake to ensure a username
    /// is not sufficient for clients to impersonate one another.
    fn identity_and_secret(&mut self, user: &str) -> Option<(ClientId, String)>;
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
/// Some errors on the client side are aggregated into this enum class.
pub enum AuthenticationError {
    AlreadyLoggedIn,
    UnknownUsername,
    ChallengeFailed,
}

/// All participants in the network get a Receiver object for reading incoming
/// messages. The object provides a cleaner wrapper for some magnetic::Consumer
/// object.
/// 
/// On the client side, messages are simply of the user-defined
/// Clientward type. On the serverside, the messages are essentially tuples of
/// the defined Serverward type and ClientId (to indicate the sender).
/// 
/// __Note__: Each Receiver object (except for a coupler) is initialized along
/// with a newly-spawned
/// thread. The thread 'babysits' the socket and incrimentally builds message
/// objects. As they are completed it _produces_ them for the Reader to
/// _consume_ as needed. 
/// 
/// To kill this thread:
///     for a remote client: the corresponding Writer object can call `shutdown`
///     for a remote server: the ServerController object can call `shutdown`
pub struct Receiver<M>
where
M: Message {
    _phantom: PhantomData<M>,
    consumer: Box<Consumer<M>>,
}
impl<M> Receiver<M> 
where M: Message {
    /// Analogous to `magnetic::Consumer::pop` for your message type M
    pub fn recv_blocking(&mut self) -> Result<M, PopError> {
        self.consumer.pop()
    }

    /// Analogous to `magnetic::Consumer::try_pop` for your message type M
    pub fn recv_nonblocking(&mut self) -> Result<M, TryPopError> {
        self.consumer.try_pop()
    }
}

/////////////////////////////// PRIVATE ////////////////////////////////////////


#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub enum MetaServerward {
    LoginRequest(String),
    ChallengeAnswer(Vec<u8>),
}
impl Message for MetaServerward {}
impl Serverward for MetaServerward {}


#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub enum MetaClientward {
    LoginAcceptance(ClientId),
    ChallengeQuestion(Vec<u8>),
    HandshakeTimeout,
    ClientMisbehaved,
    AuthenticationError(AuthenticationError),
    ClientThresholdReached,
}
impl Message for MetaClientward {}
impl Clientward for MetaClientward {}

pub fn new_receiver<C, M>(consumer: C) -> Receiver<M>
where C: Consumer<M> + 'static, M: Message {
    Receiver {consumer: Box::new(consumer), _phantom: PhantomData::default()}
}

pub fn secret_challenge_hash(secret: &str, challenge: &Vec<u8>) -> Vec<u8> {
    let mut hasher = Hasher::new(Algorithm::SHA256);
    hasher.write_all(secret.as_bytes()).expect("hash1!");
    hasher.write_all(challenge).expect("hash2!");
    hasher.finish()
}
