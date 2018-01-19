use common::*;
use magnetic::spsc::SPSCProducer;
use magnetic::buffer::dynamic::DynamicBuffer;
use std::collections::HashSet;
use magnetic::Producer;
use server::Signed;
use magnetic::spsc::spsc_queue;
use server::ClientwardSender;
use client::ServerwardSender;

/// This object implements ClientwardSender, but sends messages locally
/// (producing them in memory for the Reader to consume)
/// 
/// This struct is created with `coupler_start`.
pub struct LocalClientwardSender<C: Clientward> {
    consuming_client_id: ClientId,
	producer: SPSCProducer<C, DynamicBuffer<C>>,
}
impl<C> ClientwardSender<C> for LocalClientwardSender<C> 
where
C: Clientward {
    fn send_to(&mut self, msg: &C, cid: ClientId) -> bool {
        if cid == self.consuming_client_id {
            self.producer.push(msg.clone()).is_ok()
        } else {
            false
        }
    }

    fn send_to_sequence<'a, I>(&mut self, msg: &C, cids: I) -> u32
    where I: Iterator<Item = &'a ClientId> {
        let mut successes = 0;
        for cid in cids {
            if *cid == self.consuming_client_id {
                if self.producer.push(msg.clone()).is_ok() {
                    successes += 1;
                }
            }
        }
        successes
    }

    fn send_to_all(&mut self, msg: &C) -> u32 {
        if self.producer.push(msg.clone()).is_ok() {1} else {0}
    }

    fn online_clients(&mut self) -> HashSet<ClientId> {
        let mut s = HashSet::new();
        s.insert(self.consuming_client_id);
        s
    }
}

/// This object implements ServerwardSender, but sends messages locally
/// (producing them in memory for the Reader to consume)
/// 
/// This struct is created with `coupler_start`.
pub struct LocalServerwardSender<S: Serverward> {
    my_cid: ClientId,
	producer: SPSCProducer<Signed<S>, DynamicBuffer<Signed<S>>>
}
impl<S> ServerwardSender<S> for LocalServerwardSender<S>
where S: Serverward {
    fn send(&mut self, msg: &S) -> bool {
        self.producer.push(Signed::new(msg.clone(), self.my_cid)).is_ok()
    }
    fn shutdown(self) { drop(self) }
}

///////////////////////////// FUNCTIONS ////////////////////////////////////////

/// The main function for creating both halves of a local server-client
/// connection. This behaves exactly the same as a remote client and remote
/// server created on the same machine, but circumvents the needless networking.
/// 
/// The function requires the CliendId to associate with the client half. As
/// there can only be one client connected, this is provided from the start. It
/// is presumed that the user ensures that this client is identified and 
/// authenticated already as necessary. This function does NOT spawn any
/// threads. 
pub fn coupler_start<C,S>(client_id: ClientId)
 -> (
        LocalClientwardSender<C>,
        Receiver<C>,
        LocalServerwardSender<S>,
        Receiver<Signed<S>>,
    )
where
C: Clientward,
S: Serverward, {
    // clientward
    let (cward_p, cward_c) = spsc_queue(DynamicBuffer::new(128).unwrap()); 

    // serverward
    let (sward_p, sward_c) = spsc_queue(DynamicBuffer::new(128).unwrap());
    (
        LocalClientwardSender { consuming_client_id: client_id, producer: cward_p },
        ::common::new_receiver(cward_c),
        LocalServerwardSender { my_cid: client_id, producer: sward_p },
        ::common::new_receiver(sward_c),
    )
} 
