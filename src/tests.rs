use bidir_map::BidirMap;
use std::collections::HashMap;
use std::thread;

use super::*;

///////////////////////////// TEST IMPLEMENTATION //////////////////////////////

#[derive(Debug)]
struct TestAuthenticator {
    users: BidirMap<ClientId, String>,
    secretwords: HashMap<ClientId, String>,
    next_cid: ClientId,
}
impl TestAuthenticator {
    fn new() -> Self {
        TestAuthenticator {
            users: BidirMap::new(),
            secretwords: HashMap::new(),
            next_cid: ClientId(0),
        }
    }

    fn add_user(&mut self, user: &str, secret: &str) -> ClientId {
        let cid = self.next_cid;
        self.next_cid = ClientId(cid.0 + 1);
        self.users.insert(cid, user.to_owned());
        self.secretwords.insert(cid, secret.to_owned());
        cid
    }
}
impl Authenticator for TestAuthenticator {
    fn identity_and_secret(&mut self, user: &str) -> Option<(ClientId, String)> {
        if let Some(cid) = self.users.get_by_second(user) {
            Some((*cid, self.secretwords.get(cid).unwrap().to_owned()))
        } else {
            None
        }
    }
}

fn test_auth() -> TestAuthenticator {
    let mut auth = TestAuthenticator::new();
    auth.add_user("alice", "alice_secret");
    auth.add_user("bob", "bob_secret");
    auth.add_user("charlie", "charlie_secret");
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



// ///////////////////////////////// TESTS ////////////////////////////////////////

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
    let _ = client_start::<TestClientward, TestServerward, _>(addr, "alice", "alice_secret", None)
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
    let err = client_start::<TestClientward, TestServerward, _>(addr, "NOT_A_USER_NAME", "alice_secret", None)
    .err().expect("client was authenticated, but shouldnt have been!");
    //expecting that the username will be rejected by our authenticator
    assert_eq!(err, ClientStartError::AuthenticationError(AuthenticationError::UnknownUsername));
}

#[test]
fn client_secretword_mismatch() {
    let mut auth = test_auth();
    let addr = "127.0.0.1:5557";
    //start server
    let (_, _, mut cntl) = server_start::<_,TestClientward,TestServerward>(addr)
    .expect("server start failed");

    //start a new thread to listen for the server endlessly
    thread::spawn(move || cntl.accept_all(&mut auth) );
    
    //start client
    let err = client_start::<TestClientward, TestServerward, _>(addr, "alice", "WRONG_secret", None)
    .err().expect("client was authenticated, but shouldnt have been!");
    //expecting that the username will be rejected by our authenticator
    assert_eq!(err, ClientStartError::AuthenticationError(AuthenticationError::ChallengeFailed));
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
    let (_, _, cid) = client_start::<TestClientward, TestServerward, _>(addr, "alice", "alice_secret", None)
    .expect("first alice failed to join");
    assert_eq!(cid, ClientId(0));

    let err = client_start::<TestClientward, TestServerward, _>(addr, "alice", "alice_secret", None)
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
    let (_, _, cid) = client_start::<TestClientward, TestServerward, _>(addr, "alice", "alice_secret", None)
    .expect("alice failed to join");
    assert_eq!(cid, ClientId(0));

    //start client 1
    let (_, _, cid) = client_start::<TestClientward, TestServerward, _>(addr, "bob", "bob_secret", None)
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
    let (_, mut c_r, cid) = client_start::<TestClientward, TestServerward, _>(addr, "alice", "alice_secret", None)
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
    let (mut c_w, _, cid) = client_start::<TestClientward, TestServerward, _>(addr, "alice", "alice_secret", None)
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

    let (_, mut alice_reader, cid) = client_start::<TestClientward, TestServerward, _>(addr, "alice", "alice_secret", None)
    .expect("alice failed to join");
    assert_eq!(cid, ClientId(0));

    let (_, mut bob_reader, cid) = client_start::<TestClientward, TestServerward, _>(addr, "bob", "bob_secret", None)
    .expect("bob failed to join");
    assert_eq!(cid, ClientId(1));

    let (_, mut charlie_reader, cid) = client_start::<TestClientward, TestServerward, _>(addr, "charlie", "charlie_secret", None)
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
    client_start::<TestClientward, TestServerward, _>(addr, "alice", "alice_secret", None)
    .err().expect("alice was authenticated, but shouldnt have been!");

    //start a new thread to listen for the server endlessly
    thread::spawn(move || cntl.accept_all(&mut auth) );

    
    // bob gets accepted now that the server is listening
    client_start::<TestClientward, TestServerward, _>(addr, "bob", "bob_secret", None)
    .expect("bob was expecting to be authenticated!");
}

#[test]
fn file_secretword_auth() {
    use std::path::Path;
    let addr = "127.0.0.1:5566";
    //create an authenticator given by Axial. Start it with a path to the folder
    //this folder contains one file: `alice` with contents `12$alice_secret`
    let mut auth = authenticators::FilessecretwordAuth::new(Path::new("./file_secretword_auth_data"));
    //start server
    let (_, _, mut cntl) = server_start::<_,TestClientward,TestServerward>(addr)
    .expect("server start failed");
    //start client 0

    //start a new thread to listen for the server endlessly
    thread::spawn(move || cntl.accept_all(&mut auth) );

    let (_, _, cid) = client_start::<TestClientward, TestServerward, _>(addr, "alice", "alice_secret", None)
    .expect("alice failed to join");
    assert_eq!(cid, ClientId(12));

    client_start::<TestClientward, TestServerward, _>(addr, "charlie", "charlie_secret", None)
    .err().expect("charlie joined somehow");
}

#[test]
fn secretword_auth() {
    let addr = "127.0.0.1:5567";
    let mut auth = authenticators::secretwordAuth::new_from_vec(
        vec![("alice".to_owned(),   "alice_secret".to_owned()),
             ("bob".to_owned(),     "bob_secret".to_owned())]
    );
    //create an authenticator given by Axial. Start it with a path to the folder
    //this folder contains one file: `alice` with contents `12$alice_secret`
    //start server
    let (_, _, mut cntl) = server_start::<_,TestClientward,TestServerward>(addr)
    .expect("server start failed");
    //start client 0

    //start a new thread to listen for the server endlessly
    thread::spawn(move || cntl.accept_all(&mut auth) );

    let (_, _, cid) = client_start::<TestClientward, TestServerward, _>(addr, "alice", "alice_secret", None)
    .expect("alice failed to join");
    assert_eq!(cid, ClientId(0));

    let (_, _, cid) = client_start::<TestClientward, TestServerward, _>(addr, "bob", "bob_secret", None)
    .expect("alice failed to join");
    assert_eq!(cid, ClientId(1));

    client_start::<TestClientward, TestServerward, _>(addr, "charlie", "charlie_secret", None)
    .err().expect("charlie joined somehow");
}

#[test]
fn coupler() {
    // create a coupler with one client: client 0
    let (mut cward_send, mut cward_recv,
         mut sward_send, mut sward_recv) = coupler_start(ClientId(0));

    //fails. nonblocking call doesnt wait for something to be sent
    assert!(cward_recv.recv_nonblocking().is_err());

    //--------------------------------------------------------------------------

    //server sends something to all clients (just client 0)
    assert_eq!(
        cward_send.send_to_all(&TestClientward::HelloToClient),
        1, //one client gets the message
    );
    
    // client receives the message
    assert_eq!(
        cward_recv.recv_blocking().unwrap(),
        TestClientward::HelloToClient,
    );

    //--------------------------------------------------------------------------

    //client successfully sends a message toward server,
    //which server receives annotated with cid 0
    assert!(sward_send.send(&TestServerward::HelloToServer));
    assert_eq!(
        sward_recv.recv_blocking().unwrap(),
        Signed(TestServerward::HelloToServer, ClientId(0)),
    );

    //--------------------------------------------------------------------------

    //server sends messages to clients 1,2 and 1 again. 
    let seq = vec![ClientId(1), ClientId(2), ClientId(1)];
    assert_eq!(
        cward_send.send_to_sequence(&TestClientward::HelloToClient, seq.iter()),
        0, //zero of the messages are sent successfully
    );

    //Client0 doesn't receive it
    assert!(cward_recv.recv_nonblocking().is_err());
}