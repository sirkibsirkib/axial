# SChannel
SChannel is 

SChannel provides a channel-like abstraction over a Tcp-based server-client(s)
connection. At each endpoint, a writer and reader of upstream and downstream


## Purpose



SChannel allows for simplified asynchronous 1-server-N-client systems.

SChannel is inspired by works such as `Tokio` and `mio` beneath it. However,
this is a less generic tool that allows for a more transparent encapsulation
for a more specific use case. 

```
Client0 W|----.
        R|<--. \
              \ `-->|\
               `----| |-->|R  Server
Client1 W|--------->| |---|W*
        R|<---------|/
      
      ...
```
At each endpoint, SChannel exposes a reader and writer object for sending and receiving



1. Distributed system with a 1-server, N-client architecture
1. A central state 
1. Optional user authentication



## Usefulness


## Using It Yourself

## Examples
