
use std::collections::HashSet;
use std::vec::Drain;
use std::thread::spawn;

extern crate magnetic;
use magnetic::spsc::spsc_queue;
use magnetic::buffer::dynamic::DynamicBuffer;
use magnetic::{Producer, Consumer};
use magnetic::spsc::{SPSCConsumer,SPSCProducer};
use magnetic::{PopError, TryPopError, PushError, TryPushError};

extern crate trailing_cell;
use trailing_cell::{TakesMessage,TcReader,TcWriter};

pub enum SetChange {
	Insert(ClientID), Remove(ClientID),
}

impl TakesMessage<SetChange> for HashSet<ClientID> {
	fn take_message(&mut self, msg: &SetChange) {
		match msg {
			&SetChange::Insert(x) => { self.insert(x); },
			&SetChange::Remove(x) => { self.remove(&x); },
		}
	}
}

pub trait Message {
    fn serialize(&self) -> Vec<u8>;
}
pub trait Serverward: Message {}
pub trait Clientward: Message {}

#[derive(Eq, PartialEq, Copy, Clone, Hash)]
pub struct ClientID(u32);

pub struct Signed<M> {
    msg: M,
    signature: u32,
}

impl<M> Message for Signed<M>
where M: Message {
    fn serialize(&self) -> Vec<u8> {
        let mut v = self.msg.serialize();
        v.push(1 as u8);
        v
    }
}


pub fn client_start<C,S>() -> (ServerwardSender<S>, Receiver<C>)
where
    C: Clientward,
    S: Serverward, {
    unimplemented!()
} 

type TrailingClientset = TcReader<HashSet<ClientID>, SetChange>;  

pub fn server_start<C,S>() -> (ClientwardSender<C>, Receiver<Signed<S>>, TrailingClientset)
where
    C: Clientward,
    S: Serverward, {
    unimplemented!()
}

// TODO pub fn coupler() -> (...)


pub struct ServerwardSender<S: Serverward> {
	producer: SPSCProducer<S, DynamicBuffer<S>>,
}
impl<S> ServerwardSender<S> 
where S: Serverward {
	
}

pub struct ClientwardSender<C: Clientward> {
	producer: SPSCProducer<C, DynamicBuffer<C>>,
}
impl<C> ClientwardSender<C> 
where C: Clientward {
    pub fn send_to(&mut self, msg: C, cid: ClientID) {

    }

    pub fn send_to_all(&mut self, msg: C, cids: &HashSet<ClientID>) {

    }
}

pub struct Receiver<M: Message> {
    consumer: SPSCConsumer<M, DynamicBuffer<M>>,
}
impl<M> Receiver<M> 
where M: Message {
    pub fn recv_blocking(&mut self) -> Result<M, PopError> {
        self.consumer.pop()
    }

    pub fn try_recv(&mut self) -> Result<M, TryPopError> {
        self.consumer.try_pop()
    }

    pub fn drain(&mut self) -> Drain<M> {
        //TODO
        unimplemented!()
    }
}