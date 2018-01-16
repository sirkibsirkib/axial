use common::*;
use magnetic::spsc::{SPSCProducer,SPSCConsumer};
use magnetic::buffer::dynamic::DynamicBuffer;
use std::collections::HashSet;
use magnetic::Producer;
use server::Signed;
use magnetic::spsc::spsc_queue;
use server::ClientwardSender;
use client::ServerwardSender;
// use magnetic::spsc::SPSCConsumer;


pub struct LocalClientwardSender<C: Clientward> {
    consuming_client_id: ClientId,
	producer: SPSCProducer<C, DynamicBuffer<C>>,
}
impl<C> ClientwardSender<C> for LocalClientwardSender<C> 
where
C: Clientward {
    fn send_to(&mut self, msg: &C, cid: ClientId) -> bool {
        if cid == self.consuming_client_id {
            self.producer.push(msg.clone());
            true
        } else {
            false
        }
    }

    fn send_to_sequence<'a, I>(&mut self, msg: &C, cids: I) -> u32
    where I: Iterator<Item = &'a ClientId> {
        let mut successes = 0;
        for cid in cids {
            if *cid == self.consuming_client_id {
                self.producer.push(msg.clone());
                successes += 1;
            }
        }
        successes
    }

    fn send_to_all(&mut self, msg: &C) -> u32 {
        self.producer.push(msg.clone());
        1
    }

    fn online_clients(&mut self) -> HashSet<ClientId> {
        let mut s = HashSet::new();
        s.insert(self.consuming_client_id);
        s
    }
}


pub struct LocalServerwardSender<S: Serverward> {
    my_cid: ClientId,
	producer: SPSCProducer<Signed<S>, DynamicBuffer<Signed<S>>>
}
impl<S> ServerwardSender<S> for LocalServerwardSender<S>
where S: Serverward {
    fn send(&mut self, msg: &S) -> bool {
        self.producer.push(Signed::new(msg.clone(), self.my_cid));
        true
    }

    fn shutdown(self) {
        drop(self)
    }
}

///////////////////////////// FUNCTIONS ////////////////////////////////////////

pub fn coupler_start<C,S>(client_id: ClientId)
 -> (
        LocalClientwardSender<C>,
        Receiver<SPSCConsumer<Signed<S>, DynamicBuffer<Signed<S>>>, Signed<S>>,
        LocalServerwardSender<S>,
        Receiver<SPSCConsumer<C, DynamicBuffer<C>>, C>,
    )
where
C: Clientward,
S: Serverward, {
    // clientward
    let (p1, c1) = spsc_queue(DynamicBuffer::new(128).unwrap()); 
    
    // serverward
    let (p2, c2) = spsc_queue(DynamicBuffer::new(128).unwrap());
    (
        LocalClientwardSender { consuming_client_id: client_id, producer: p1 },
        ::common::new_receiver(c2),
        LocalServerwardSender { my_cid: client_id, producer: p2 },
        ::common::new_receiver(c1),
    )
} 
