use bidir_map::BidirMap;
use std::collections::HashMap;
use std::thread;

use super::*;

///////////////////////////// TEST IMPLEMENTATION //////////////////////////////

#[derive(Debug)]
struct TestAuthenticator {
    users: BidirMap<ClientId, String>,
    passwords: HashMap<ClientId, String>,
    next_cid: ClientId,
}
impl TestAuthenticator {
    fn new() -> Self {
        TestAuthenticator {
            users: BidirMap::new(),
            passwords: HashMap::new(),
            next_cid: ClientId(0),
        }
    }

    fn add_user(&mut self, user: &str, pass: &str) -> ClientId {
        let cid = self.next_cid;
        self.next_cid = ClientId(cid.0 + 1);
        self.users.insert(cid, user.to_owned());
        self.passwords.insert(cid, pass.to_owned());
        cid
    }
}
impl Authenticator for TestAuthenticator {
    fn try_authenticate(&mut self, user: &str, pass: &str) -> Result<ClientId, AuthenticationError> {
        if let Some(cid) = self.users.get_by_second(user) {
            if self.passwords.get(cid).unwrap() == pass {
                Ok(*cid)
            } else {
                Err(AuthenticationError::PasswordMismatch)
            }
        } else {
            Err(AuthenticationError::UnknownUsername)
        }
     }
}

fn test_auth() -> TestAuthenticator {
    let mut auth = TestAuthenticator::new();
    auth.add_user("alice", "alice_pass");
    auth.add_user("bob", "bob_pass");
    auth.add_user("charlie", "charlie_pass");
    auth
}


#[derive(Serialize, Deserialize, Debug, Copy, Clone, PartialEq, Eq)]
enum TestClientward {
    HelloToClient,
}
impl Message for TestClientward {}
impl Clientward for TestClientward {}

#[derive(Serialize, Deserialize, Debug, Copy, Clone, PartialEq, Eq)]
enum TestServerward {
    HelloToServer,
}
impl Message for TestServerward {}
impl Serverward for TestServerward {}



///////////////////////////////// TESTS ////////////////////////////////////////

#[test]
fn server_bind() {
    let addr = "127.0.0.1:5553";
    //start server
    let s_result = server_start::<_,TestClientward,TestServerward>(addr);
    assert!(s_result.is_ok());
}

#[test]
fn bind_fail() {
    let addr = "127.0.0.1:5554";
    //start server
    let (_, _, _cntl) = server_start::<_,TestClientward,TestServerward>(addr)
    //         ^ NOT throwing away the controller. 
    .expect("server start failed");

    //start server
    let _ = server_start::<_,TestClientward,TestServerward>(addr)
    .err().expect("server bound again?");
}

#[test]
fn no_server() {
    let addr = "127.0.0.1:5555";
    let _ = client_start::<TestClientward, TestServerward, _>(addr, "alice", "alice_pass", None)
    .err().expect("client was authenticated, but shouldnt have been!");
    //no server running on port 5555 probably
}

#[test]
fn client_bad_user() {
    let mut auth = test_auth();
    let addr = "127.0.0.1:5556";
    //start server
    let (_, _, mut cntl) = server_start::<_,TestClientward,TestServerward>(addr)
    .expect("server start failed");

    //start a new thread to listen for the server endlessly
    thread::spawn(move || cntl.accept_all(&mut auth) );
    
    //start client
    let err = client_start::<TestClientward, TestServerward, _>(addr, "NOT_A_USER_NAME", "alice_pass", None)
    .err().expect("client was authenticated, but shouldnt have been!");
    //expecting that the username will be rejected by our authenticator
    assert_eq!(err, ClientStartError::AuthenticationError(AuthenticationError::UnknownUsername));
}

#[test]
fn client_password_mismatch() {
    let mut auth = test_auth();
    let addr = "127.0.0.1:5557";
    //start server
    let (_, _, mut cntl) = server_start::<_,TestClientward,TestServerward>(addr)
    .expect("server start failed");

    //start a new thread to listen for the server endlessly
    thread::spawn(move || cntl.accept_all(&mut auth) );
    
    //start client
    let err = client_start::<TestClientward, TestServerward, _>(addr, "alice", "WRONG_PASS", None)
    .err().expect("client was authenticated, but shouldnt have been!");
    //expecting that the username will be rejected by our authenticator
    assert_eq!(err, ClientStartError::AuthenticationError(AuthenticationError::PasswordMismatch));
}

#[test]
fn client_twice() {
    let mut auth = test_auth();
    let addr = "127.0.0.1:5558";
    //start server
    let (_, _, mut cntl) = server_start::<_,TestClientward,TestServerward>(addr)
    .expect("server start failed");
    
    //start a new thread to listen for the server endlessly
    thread::spawn(move || cntl.accept_all(&mut auth) );
    
    //start client 0
    let (_, _, cid) = client_start::<TestClientward, TestServerward, _>(addr, "alice", "alice_pass", None)
    .expect("first alice failed to join");
    assert_eq!(cid, ClientId(0));

    let err = client_start::<TestClientward, TestServerward, _>(addr, "alice", "alice_pass", None)
    .err().expect("client was authenticated, but shouldnt have been!");
    //alice cannot be logged in twice
    assert_eq!(err, ClientStartError::AuthenticationError(AuthenticationError::AlreadyLoggedIn));
}

#[test]
fn two_clients() {
    let mut auth = test_auth();
    let addr = "127.0.0.1:5559";
    //start server
    let (_, _, mut cntl) = server_start::<_,TestClientward,TestServerward>(addr)
    .expect("server start failed");
    
    //start a new thread to listen for the server endlessly
    thread::spawn(move || cntl.accept_all(&mut auth) );
    
    //start client 0
    let (_, _, cid) = client_start::<TestClientward, TestServerward, _>(addr, "alice", "alice_pass", None)
    .expect("alice failed to join");
    assert_eq!(cid, ClientId(0));

    //start client 1
    let (_, _, cid) = client_start::<TestClientward, TestServerward, _>(addr, "bob", "bob_pass", None)
    .expect("bob failed to join");
    assert_eq!(cid, ClientId(1));
}

#[test]
fn server_send() {
    let mut auth = test_auth();
    let addr = "127.0.0.1:5560";
    //start server
    let (mut s_w, _, mut cntl) = server_start::<_,TestClientward,TestServerward>(addr)
    .expect("server start failed");
    
    //start a new thread to listen for the server endlessly
    thread::spawn(move || cntl.accept_all(&mut auth) );
    
    //start client 0
    let (_, mut c_r, cid) = client_start::<TestClientward, TestServerward, _>(addr, "alice", "alice_pass", None)
    .expect("alice failed to join");
    assert_eq!(cid, ClientId(0));

    assert!(s_w.send_to(&TestClientward::HelloToClient, ClientId(0)));
    let msg = c_r.recv_blocking().expect("bad reply");
    assert_eq!(msg, TestClientward::HelloToClient);
}

#[test]
fn client_send() {
    let mut auth = test_auth();
    let addr = "127.0.0.1:5561";
    //start server
    let (_, mut s_r, mut cntl) = server_start::<_,TestClientward,TestServerward>(addr)
    .expect("server start failed");
    
    //start a new thread to listen for the server endlessly
    thread::spawn(move || cntl.accept_all(&mut auth) );
    
    //start client 0
    let (mut c_w, _, cid) = client_start::<TestClientward, TestServerward, _>(addr, "alice", "alice_pass", None)
    .expect("alice failed to join");
    assert_eq!(cid, ClientId(0));

    assert!(c_w.send(&TestServerward::HelloToServer));
    let msg = s_r.recv_blocking().expect("bad reply");
    assert_eq!(msg, Signed::new(TestServerward::HelloToServer, ClientId(0)));
}

#[test]
fn drop() {
    let addr = "127.0.0.1:5562";
    let (_s_w, _s_r, _) = server_start::<_,TestClientward,TestServerward>(addr)
    //               ^ dropping controller causes sockets to close and listener to be freed.
    //                 this can also be achieved with shutdown() (which calls drop())
    .expect("server start failed");

    // the port is free to be used again! first server has freed sockets
    let _ = server_start::<_,TestClientward,TestServerward>(addr)
    .expect("server start failed");
}

#[test]
fn broadcast() {
    let addr = "127.0.0.1:5563";
    let mut auth = test_auth();
    let (mut s_w, _, mut cntl) = server_start::<_,TestClientward,TestServerward>(addr)
    .expect("server start failed");
    
    //start a new thread to listen for the server endlessly
    thread::spawn(move || cntl.accept_all(&mut auth) );

    let (_, mut alice_reader, cid) = client_start::<TestClientward, TestServerward, _>(addr, "alice", "alice_pass", None)
    .expect("alice failed to join");
    assert_eq!(cid, ClientId(0));

    let (_, mut bob_reader, cid) = client_start::<TestClientward, TestServerward, _>(addr, "bob", "bob_pass", None)
    .expect("bob failed to join");
    assert_eq!(cid, ClientId(1));

    let (_, mut charlie_reader, cid) = client_start::<TestClientward, TestServerward, _>(addr, "charlie", "charlie_pass", None)
    .expect("charlie failed to join");
    assert_eq!(cid, ClientId(2));

    // send this message to everyone
    assert_eq!(s_w.send_to_all(&TestClientward::HelloToClient), 3);
    
    // all three clients do indeed get a copy of the message
    assert_eq!(TestClientward::HelloToClient, alice_reader.recv_blocking()  .expect("alice failed"));
    assert_eq!(TestClientward::HelloToClient, bob_reader.recv_blocking()    .expect("bob failed"));
    assert_eq!(TestClientward::HelloToClient, charlie_reader.recv_blocking().expect("charlie failed"));
}

#[test]
fn fine_server_control() {
    let addr = "127.0.0.1:5564";
    let mut auth = test_auth();
    let (_, _, mut cntl) = server_start::<_,TestClientward,TestServerward>(addr)
    .expect("server start failed");

    // the server isnt listening. alice can't connect!
    client_start::<TestClientward, TestServerward, _>(addr, "alice", "alice_pass", None)
    .err().expect("alice was authenticated, but shouldnt have been!");

    //start a new thread to listen for the server endlessly
    thread::spawn(move || cntl.accept_all(&mut auth) );

    
    // bob gets accepted now that the server is listening
    client_start::<TestClientward, TestServerward, _>(addr, "bob", "bob_pass", None)
    .expect("bob was expecting to be authenticated!");
}

#[test]
fn coupler() {
    let (mut cward_send, mut cward_recv,
         mut sward_send, mut sward_recv) = coupler_start(ClientId(0));

    //fails. nonblocking call doesnt wait for something to be sent
    assert!(cward_recv.recv_nonblocking().is_err());

    //server sends something to all clients (just client 0)
    cward_send.send_to_all(&TestClientward::HelloToClient);
    assert_eq!(
        cward_recv.recv_blocking().unwrap(),
        TestClientward::HelloToClient,
    );

    //client successfully sends a message toward server, which server receives annotated with cid 0
    assert!(sward_send.send(&TestServerward::HelloToServer));
    assert_eq!(
        sward_recv.recv_blocking().unwrap(),
        Signed(TestServerward::HelloToServer, ClientId(0)),
    );

    //server sends messages to clients 1,2,3. 
    let seq = vec![ClientId(1), ClientId(2), ClientId(3)];
    // 0 of the messages are send successfully
    assert_eq!(
        cward_send.send_to_sequence(&TestClientward::HelloToClient, seq.iter()),
        0,
    );

    //Client0 doesn't receive it
    assert!(cward_recv.recv_nonblocking().is_err());
}