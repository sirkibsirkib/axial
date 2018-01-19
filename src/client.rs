use std::net::ToSocketAddrs;
use std::thread;
use std::net::{TcpStream};
use std::marker::PhantomData;
use std::io;
use std::time::Duration;

use magnetic::buffer::dynamic::DynamicBuffer;
use magnetic::{Producer};
use magnetic::spsc::{SPSCConsumer,SPSCProducer,spsc_queue};

use messaging::*;
use common::*;


//////////////////////////// RETURN TYPES & API ////////////////////////////////

pub trait ServerwardSender<S: Serverward> {
    fn send(&mut self, msg: &S) -> bool;
    fn shutdown(self);
}

#[derive(Debug)]
pub struct RemoteServerwardSender<S: Serverward> {
	stream: TcpStream,
    _phantom: PhantomData<S>,
}
impl<S> ServerwardSender<S> for RemoteServerwardSender<S>
where S: Serverward {
    fn send(&mut self, msg: &S) -> bool {
        self.stream.single_write(msg).is_ok()
    }

    fn shutdown(self) {
        let _ = self.stream.shutdown(::std::net::Shutdown::Both); //TODO
    }
}


/////////////////////// MESSAGING & ERROR ENUMS ////////////////////////////////


#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum ClientStartError {
    ConnectFailed,
    ClientThresholdReached,
    SocketMisbehaved,
    ServerMisbehaved,
    ClientMisbehaved,
    ChallengeFailed,
    HandshakeTimeout,
    AuthenticationError(AuthenticationError),
}
impl From<io::Error> for ClientStartError {
    #[inline]
    fn from(_: io::Error) -> Self { ClientStartError::ConnectFailed }
}
impl From<MessageError> for ClientStartError {
    #[inline]
    fn from(_: MessageError) -> Self { ClientStartError::ConnectFailed }
}

fn client_connect<T: ToSocketAddrs>(addr: T, connect_timeout: Option<Duration>) -> Result<TcpStream, ClientStartError> {
    match connect_timeout {
        Some(duration) => {
            for a in addr.to_socket_addrs().unwrap()   {
                let x = TcpStream::connect_timeout(&a, duration) ?;
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

type ClientResult<C,S> =
Result<
    (RemoteServerwardSender<S>, Receiver<C>, ClientId),
    ClientStartError,
>;
type SpscPair<S> = (
    SPSCProducer<S, DynamicBuffer<S>>,
    SPSCConsumer<S, DynamicBuffer<S>>,
);

pub fn client_start<C,S,T>(addr: T, user: &str, secret: &str, connect_timeout: Option<Duration>)
-> ClientResult<C,S>
where
    C: Clientward,
    S: Serverward, 
    T: ToSocketAddrs
{
    let mut stream = client_connect(addr, connect_timeout)?;
    let login_msg = MetaServerward::LoginRequest(user.to_owned());
    if stream.single_write(&login_msg).is_err() {
        return Err(ClientStartError::SocketMisbehaved);
    }
    let mut lil_buffer = [0_u8; 128];
    let timeout = Duration::from_millis(500);
    use common::MetaClientward as MC;

    for _ in 0..(::server::MAX_CLIENT_CHALLENGES+1) {
        let response = stream.single_timout_silence_read::<MetaClientward>(&mut lil_buffer[..], timeout)?;
        if let MC::ChallengeQuestion(question) = response {
            let ans = secret_challenge_hash(secret, &question);
            if ! stream.single_write(& MetaServerward::ChallengeAnswer(ans)).is_ok() {
                return Err(ClientStartError::SocketMisbehaved);
            }
        } else {
            return match response {
                MC::LoginAcceptance(cid) => {return login_acceptance(stream, cid);},
                MC::HandshakeTimeout => Err(ClientStartError::HandshakeTimeout),
                MC::ClientThresholdReached => Err(ClientStartError::ClientThresholdReached),
                MC::AuthenticationError(err) => Err(ClientStartError::AuthenticationError(err)),
                MC::ClientMisbehaved => Err(ClientStartError::ClientMisbehaved),
                MC::ChallengeQuestion(_) => panic!("rust broke"),
            };
        }
    }
    Err(ClientStartError::ServerMisbehaved)
}

fn login_acceptance<C,S>(stream: TcpStream, cid: ClientId) -> ClientResult<C,S>
where
    C: Clientward,
    S: Serverward,
{
    let (p, c) : SpscPair<C> = spsc_queue(DynamicBuffer::new(128).unwrap());
    if let Ok(mut stream_clone) = stream.try_clone() {
        let _ = thread::spawn(move || {
            let mut buffer = [0u8; 2048]; //incoming messages can be big
            //pulls messages off the line, PRODUCES them
            while let Ok(msg) = stream_clone.single_read(&mut buffer) {
                if let Err(_) = p.push(msg) {
                    //failed to write to stream
                    return;
                }
            }
        });
        Ok((
            RemoteServerwardSender { stream: stream, _phantom: PhantomData::default() },
            ::common::new_receiver(c),
            cid,
        ))
    } else {
        Err(ClientStartError::SocketMisbehaved)
    }
}