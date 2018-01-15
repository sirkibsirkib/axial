
use serde::{Serialize};
use serde::de::DeserializeOwned;
use magnetic::Consumer;
use std::marker::PhantomData;
use magnetic::{PopError, TryPopError};

pub trait Message: Serialize + DeserializeOwned {}
pub trait Serverward: Message {}
pub trait Clientward: Message {}

#[derive(Eq, PartialEq, Copy, Clone, Hash, Debug, Serialize, Deserialize)]
pub struct ClientId(pub u32);

pub trait Authenticator: Send {
    fn try_authenticate(&mut self, user: &str, pass: &str)
     -> Result<ClientId, AuthenticationError>;
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum AuthenticationError {
    AlreadyLoggedIn,
    UnknownUsername,
    PasswordMismatch,
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

pub fn new_receiver<C,M>(consumer: C) -> Receiver<C,M>
where C: Consumer<M>, M: Message {
    Receiver {consumer: consumer, _phantom: PhantomData::default()}
}