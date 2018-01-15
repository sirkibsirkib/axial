use super::*;
use bidir_map::BidirMap;
use std::collections::HashMap;


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
    fn try_authenticate(&mut self, user: &str, pass: &str)
     -> Result<ClientId, AuthenticationError> {
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


#[derive(Serialize, Deserialize)]
enum TestClientward {
    HelloToClient,
}
impl Message for TestClientward {}
impl Clientward for TestClientward {}

#[derive(Serialize, Deserialize)]
enum TestServerward {
    HelloToServer,
}
impl Message for TestServerward {}
impl Serverward for TestServerward {}



///////////////////////////////// TESTS ////////////////////////////////////////

#[test]
fn server_bind() {
    let auth = test_auth();
    //start server
    let s_result = api::server_start::<_,_,TestClientward,TestServerward>("127.0.0.1:5555", auth);
    assert!(s_result.is_ok());
}

#[test]
fn client_bad_user() {
    let auth = test_auth();
    let (writer, reader) = api::server_start::<_,_,TestClientward,TestServerward>("127.0.0.1:5555", auth)
    .expect("server start failed");
    let err = api::client_start("127.0.0.1:5555", "NOT_A_USER_NAME", "alice_pass")
    .err().expect("client was authenticated, but shouldnt have been!");
    assert_eq!(err, ClientStartError::UnexpectedInternal(ScurvyMessage::Err(ScurvyError::Auth(AuthenticationError::UnknownUsername))));
}