use super::*;
use bidir_map::BidirMap;
use std::collections::HashMap;
// use std::time;
use std::thread;


///////////////////////////// TEST IMPLEMENTATION //////////////////////////////

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
    let s_result = api::server_start::<_,TestClientward,TestServerward>(addr);
    assert!(s_result.is_ok());
}

#[test]
fn bind_fail() {
    let addr = "127.0.0.1:5554";
    //start server
    let (_, _, _cntl) = api::server_start::<_,TestClientward,TestServerward>(addr)
    //         ^ NOT throwing away the controller. 
    .expect("server start failed");

    //start server
    let _ = api::server_start::<_,TestClientward,TestServerward>(addr)
    .err().expect("server bound again?");
}

#[test]
fn no_server() {
    let addr = "127.0.0.1:5555";
    let _ = api::client_start::<TestClientward, TestServerward, _>(addr, "alice", "alice_pass", None)
    .err().expect("client was authenticated, but shouldnt have been!");
    //no server running on port 5555 probably
}

#[test]
fn client_bad_user() {
    let mut auth = test_auth();
    let addr = "127.0.0.1:5556";
    //start server
    let (_, _, mut cntl) = api::server_start::<_,TestClientward,TestServerward>(addr)
    .expect("server start failed");

    //start a new thread to listen for the server endlessly
    thread::spawn(move || cntl.accept_all(&mut auth) );
    
    //start client
    let err = api::client_start::<TestClientward, TestServerward, _>(addr, "NOT_A_USER_NAME", "alice_pass", None)
    .err().expect("client was authenticated, but shouldnt have been!");
    //expecting that the username will be rejected by our authenticator
    assert_eq!(err, ClientStartError::AuthenticationError(AuthenticationError::UnknownUsername));
}

#[test]
fn client_password_mismatch() {
    let mut auth = test_auth();
    let addr = "127.0.0.1:5557";
    //start server
    let (_, _, mut cntl) = api::server_start::<_,TestClientward,TestServerward>(addr)
    .expect("server start failed");

    //start a new thread to listen for the server endlessly
    thread::spawn(move || cntl.accept_all(&mut auth) );
    
    //start client
    let err = api::client_start::<TestClientward, TestServerward, _>(addr, "alice", "WRONG_PASS", None)
    .err().expect("client was authenticated, but shouldnt have been!");
    //expecting that the username will be rejected by our authenticator
    assert_eq!(err, ClientStartError::AuthenticationError(AuthenticationError::PasswordMismatch));
}

#[test]
fn client_twice() {
    let mut auth = test_auth();
    let addr = "127.0.0.1:5558";
    //start server
    let (_, _, mut cntl) = api::server_start::<_,TestClientward,TestServerward>(addr)
    .expect("server start failed");
    
    //start a new thread to listen for the server endlessly
    thread::spawn(move || cntl.accept_all(&mut auth) );
    
    //start client 0
    let (_, _, cid) = api::client_start::<TestClientward, TestServerward, _>(addr, "alice", "alice_pass", None)
    .expect("first alice failed to join");
    assert_eq!(cid, ClientId(0));

    let err = api::client_start::<TestClientward, TestServerward, _>(addr, "alice", "alice_pass", None)
    .err().expect("client was authenticated, but shouldnt have been!");
    //alice cannot be logged in twice
    assert_eq!(err, ClientStartError::AuthenticationError(AuthenticationError::AlreadyLoggedIn));
}

#[test]
fn two_clients() {
    let mut auth = test_auth();
    let addr = "127.0.0.1:5559";
    //start server
    let (_, _, mut cntl) = api::server_start::<_,TestClientward,TestServerward>(addr)
    .expect("server start failed");
    
    //start a new thread to listen for the server endlessly
    thread::spawn(move || cntl.accept_all(&mut auth) );
    
    //start client 0
    let (_, _, cid) = api::client_start::<TestClientward, TestServerward, _>(addr, "alice", "alice_pass", None)
    .expect("alice failed to join");
    assert_eq!(cid, ClientId(0));

    //start client 1
    let (_, _, cid) = api::client_start::<TestClientward, TestServerward, _>(addr, "bob", "bob_pass", None)
    .expect("bob failed to join");
    assert_eq!(cid, ClientId(1));
}

#[test]
fn server_send() {
    let mut auth = test_auth();
    let addr = "127.0.0.1:5560";
    //start server
    let (mut s_w, _, mut cntl) = api::server_start::<_,TestClientward,TestServerward>(addr)
    .expect("server start failed");
    
    //start a new thread to listen for the server endlessly
    thread::spawn(move || cntl.accept_all(&mut auth) );
    
    //start client 0
    let (_, mut c_r, cid) = api::client_start::<TestClientward, TestServerward, _>(addr, "alice", "alice_pass", None)
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
    let (_, mut s_r, mut cntl) = api::server_start::<_,TestClientward,TestServerward>(addr)
    .expect("server start failed");
    
    //start a new thread to listen for the server endlessly
    thread::spawn(move || cntl.accept_all(&mut auth) );
    
    //start client 0
    let (mut c_w, _, cid) = api::client_start::<TestClientward, TestServerward, _>(addr, "alice", "alice_pass", None)
    .expect("alice failed to join");
    assert_eq!(cid, ClientId(0));

    assert!(c_w.send(&TestServerward::HelloToServer));
    let msg = s_r.recv_blocking().expect("bad reply");
    assert_eq!(msg, Signed::new(TestServerward::HelloToServer, ClientId(0)));
}

#[test]
fn drop() {
    let addr = "127.0.0.1:5562";
    let (_s_w, _s_r, _) = api::server_start::<_,TestClientward,TestServerward>(addr)
    //               ^ dropping controller causes sockets to close and listener to be freed.
    //                 this can also be achieved with shutdown() (which calls drop())
    .expect("server start failed");

    // the port is free to be used again! first server has freed sockets
    let _ = api::server_start::<_,TestClientward,TestServerward>(addr)
    .expect("server start failed");
}

#[test]
fn broadcast() {
    let addr = "127.0.0.1:5563";
    let mut auth = test_auth();
    let (mut s_w, _, mut cntl) = api::server_start::<_,TestClientward,TestServerward>(addr)
    .expect("server start failed");
    
    //start a new thread to listen for the server endlessly
    thread::spawn(move || cntl.accept_all(&mut auth) );

    let (_, mut alice_reader, cid) = api::client_start::<TestClientward, TestServerward, _>(addr, "alice", "alice_pass", None)
    .expect("alice failed to join");
    assert_eq!(cid, ClientId(0));

    let (_, mut bob_reader, cid) = api::client_start::<TestClientward, TestServerward, _>(addr, "bob", "bob_pass", None)
    .expect("bob failed to join");
    assert_eq!(cid, ClientId(1));

    let (_, mut charlie_reader, cid) = api::client_start::<TestClientward, TestServerward, _>(addr, "charlie", "charlie_pass", None)
    .expect("charlie failed to join");
    assert_eq!(cid, ClientId(2));

    assert_eq!(s_w.send_to_all(&TestClientward::HelloToClient), 3);
    
    assert_eq!(TestClientward::HelloToClient, alice_reader.recv_blocking()  .expect("alice failed"));
    assert_eq!(TestClientward::HelloToClient, bob_reader.recv_blocking()    .expect("bob failed"));
    assert_eq!(TestClientward::HelloToClient, charlie_reader.recv_blocking().expect("charlie failed"));
}

#[test]
fn fine_server_control() {
    //TODO single_recv blocks indefinitely!


    // let addr = "127.0.0.1:5564";
    // let mut auth = test_auth();
    // let (_, _, mut cntl) = api::server_start::<_,TestClientward,TestServerward>(addr)
    // .expect("server start failed");

    // // the server isnt listening. alice can't connect!
    // let err = api::client_start::<TestClientward, TestServerward, _>(addr, "alice", "alice_pass")
    // .err().expect("client was authenticated, but shouldnt have been!");

    // thread::spawn(move || cntl.accept_one(&mut auth) );
    
    // // NOW alice gets accepted
    // let err = api::client_start::<TestClientward, TestServerward, _>(addr, "alice", "alice_pass")
    // .err().expect("client was authenticated, but shouldnt have been!");
}