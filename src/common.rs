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

pub trait Message: Serialize + DeserializeOwned + Clone {}
pub trait Serverward: Message {}
pub trait Clientward: Message {}

#[derive(Eq, PartialEq, Copy, Clone, Hash, Debug, Serialize, Deserialize)]
pub struct ClientId(pub u32);

pub trait Authenticator: Send {
    fn identity_and_secret(&mut self, user: &str) -> Option<(ClientId, String)>;
}


#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub enum AuthenticationError {
    AlreadyLoggedIn,
    UnknownUsername,
    ChallengeFailed,
}

pub struct Receiver<C,M>
where
C: Consumer<M>,
M: Message {
    consumer: C,
    _phantom: PhantomData<M>,
}
impl<C,M> Receiver<C,M> 
where C: Consumer<M>, M: Message {
    pub fn recv_blocking(&mut self) -> Result<M, PopError> {
        self.consumer.pop()
    }

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

pub fn new_receiver<C,M>(consumer: C) -> Receiver<C,M>
where C: Consumer<M>, M: Message {
    Receiver {consumer: consumer, _phantom: PhantomData::default()}
}

pub fn secret_challenge_hash(secret: &str, challenge: &Vec<u8>) -> Vec<u8> {
    let mut hasher = Hasher::new(Algorithm::SHA256);
    hasher.write_all(secret.as_bytes()).expect("hash1!");
    hasher.write_all(challenge).expect("hash2!");
    hasher.finish()
}
