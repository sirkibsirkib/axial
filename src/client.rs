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


type SpscPair<S> = (SPSCProducer<S, DynamicBuffer<S>>, SPSCConsumer<S, DynamicBuffer<S>>);

//////////////////////////// RETURN TYPES & API ////////////////////////////////


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


/////////////////////// MESSAGING & ERROR ENUMS ////////////////////////////////


#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum ClientStartError {
    ConnectFailed,
    LoginFailed,
    ClientThresholdReached,
    AuthenticationError(AuthenticationError),
}
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
    let timeout = Duration::from_millis(500);
    let response = stream.single_timout_silence_read::<MetaClientward>(&mut lil_buffer[..], timeout)?;
    use common::MetaClientward as MC;
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
                ::common::new_receiver(c),
                cid,
            ))
        },
        MC::ClientThresholdReached => Err(ClientStartError::ClientThresholdReached),
        MC::AuthenticationError(err) => Err(ClientStartError::AuthenticationError(err)),
    }
}