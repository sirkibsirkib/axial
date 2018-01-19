# sc-entral
## Purpose
sc-entral provides a channel-like abstraction over a centralized distributed
system. It shines in the case of persistent user "accounts".
The goal is to abstract away the details of:
1. Newcomer client identification & authentication
1. Message serialization & deserialization
1. Server-side broadcast, multicast

Each node in the system interacts with their peers via personal
(Sender, Receiver) objects. Communication is done in terms of user-defined
_structs_*, rather than bytes.

On the server-side, the server _identifies_ and _authenticates_ incoming
client connections with an authenticator* component, and performs a secure
handshake before accepting a newcomer.

Details above marked with an asterisk (*) are _user-defined_ by trait
implementations.


```
Client0 S|----.
        R|<--. \
              \ `-->|\
               `----| |-->|R  Server
Client1 S|--------->| |---|S
        R|<---------|/
```

## Usefulness
The most obvious use case is that of a request-update server-client architecture
for a multiplayer game. This takes advantage of the asynchronicity of the 

## Security
To authenticate a newcomer client, sc-entral participants use the following
protocol:

```java
S[x]    secret of client x  | I[x]  | client id for client x
R       random number       | X+Y  concatenation of x and y
H[x(]   hash of string x    | 

   client-side  ~  server-side
=============== ~ ========================================
 Client c       ~        Server               Authenticator    
    |------login(c)----->   |                         |
    |           ~           |                         |
    |  <---question(R)------|--get_secret_for(c)--->  |
    |\          ~           |                        /|
    | `-answer(H[S[c]+R])-> |   <----------S[c]-----` |
    |           ~           |---cmp-.                 |
    |           ~           |       |                 :
    |           ~           |  <----`
    |  <--accept(I[c])-----OR      
    |           ~         / |    
    |  <---reject()------`  |
    :           ~           :
``` 
__Note:__ The (question, answer) step is repeated 3 times. This is
omitted for brevity.

It is important to note that a client `secret` ie. `S[C]` is never sent over the line 
'in the clear'. Nevertheless, note that __secret != password__. The client
secret is privy information that _the server knows_. Thus, it would be more
suitable for the secret to be some salted hash of the client's true password.
When the server is trusted, then it is OK for the password itself to be a secret.

This protocol does not protect you from MitM attacks. 


## Using It Yourself
1. On both server and client side, mark your serializable message
structs using the traits `Serverward` and `Clientward`. These structs should
also be the same on both sides.
2. On the server side, acquire something that implements `Authenticator`. This
object will be required for running the server. You can either write one yourself,
or use one provided in the `authenticators` module.
3. Call `server_start` on the server side. Call `client_start` on the client(s)
side to acquire the sending/receiving objects.  

That's it! On both sides you could make an easy event loop by periodically 
draining messages from the receiver `r` with something like:
```Rust
while let msg = r.recv_nonblocking() {
    ...
}
```
In the event you'd like to define more than one `Serverward` message
(the same goes for `Clientward`) used by the
system, the recipients need a way of knowing which message to expect. There are
two ways of approaching this:
1.  Messages are on separate channels:
Layer as many networks over one another simply by initializing multiple server /
client connections. You can even re-use the same `Authenticator` object.
1. Messages are marked with their own identity:
Create a new `enum`, with variants for each type of message you'd like. Make
this enum your `Serverward` message type. 

## Examples
See `tests.rs` in the repo for some annotated examples.