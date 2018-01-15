use std::net::TcpStream;
use byteorder::{ReadBytesExt, WriteBytesExt, LittleEndian};
extern crate byteorder;
use std::io::prelude::Read;
use bincode;
use serde::{Serialize,Deserialize};
use serde::de::DeserializeOwned;
use std::io::Write;
use std::io;
use std::time::{Duration};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum MessageError {
    GotZero,
    Crash,
    Silence,
}
impl From<io::Error> for MessageError {
    fn from(_: io::Error) -> Self {
        MessageError::Crash
    }
}

pub trait Message: Serialize + DeserializeOwned {}

pub trait Messenger {
    fn single_read<'a, S>(&mut self, buf : &'a mut [u8]) -> Result<S, MessageError>
        where S : Deserialize<'a>;
    fn single_timout_silence_read<'a, S>(&mut self, buf : &'a mut [u8], timeout: Duration) -> Result<S, MessageError>
        where S : Deserialize<'a>;
    fn single_write<S>(&mut self, s : &S) -> Result<(), MessageError>
        where S : Serialize;
    fn single_write_bytes(&mut self, bytes : &[u8]) -> Result<(), MessageError>;
}

impl Messenger for TcpStream {
    fn single_read<'a, S>(&mut self, buf: &'a mut [u8]) -> Result<S, MessageError>
    where S : Deserialize<'a> {
        let mut bytes_read : usize = 0;
        while bytes_read < 4 {
            let bytes = self.read(&mut buf[bytes_read..4])?;
            if bytes == 0 {
                return Err(MessageError::GotZero)
            }
            bytes_read += bytes;
        }
        let num : usize = (&*buf).read_u32::<LittleEndian>().unwrap() as usize;
        // println!("Received header. will now wait for {} bytes", num);
        let msg_slice = &mut buf[..num];
        self.read_exact(msg_slice)?;
        if let Ok(got) = bincode::deserialize(msg_slice) {
            Ok(got)
        } else {
            Err(MessageError::Crash)
        }
    }

    fn single_timout_silence_read<'a, S>(&mut self, buf : &'a mut [u8], timeout: Duration) -> Result<S, MessageError>
    where S : Deserialize<'a> {
        let _ = self.set_read_timeout(Some(timeout));
        let mut bytes_read : usize = 0;
        while bytes_read < 4 {
            if let Ok(bytes) = self.read(&mut buf[bytes_read..4]) {
                if bytes | bytes_read == 0 {
                    return Err(MessageError::Silence)
                }
                bytes_read += bytes;
            } else {
                let _ = self.set_read_timeout(None);
                return Err(MessageError::Crash);
            }
            
        }
        let num : usize = (&*buf).read_u32::<LittleEndian>().unwrap() as usize;
        // println!("Received header. will now wait for {} bytes", num);
        let msg_slice = &mut buf[..num];
        if self.read_exact(msg_slice).is_err() {
            let _ = self.set_read_timeout(None);
            return Err(MessageError::Crash);
        }
        let _ = self.set_read_timeout(None); //done reading
        if let Ok(got) = bincode::deserialize(msg_slice) {
            Ok(got)
        } else {
            Err(MessageError::Crash)
        }
    }

    fn single_write<S>(&mut self, s : &S) -> Result<(), MessageError>
    where S : Serialize {
        // println!("STARTING SINGLE_WRITE");
        // let stringy = serde_json::to_string(&s).expect("serde outgoing json ONLY");
        // let bytes = stringy.as_bytes();
        let bytes = bincode::serialize(&s, bincode::Infinite).expect("went kk lel");
        let mut num : [u8;4] = [0;4];
        // println!("Writing {} bytes message `{}`", bytes.len(), &stringy);
        (&mut num[..]).write_u32::<LittleEndian>(bytes.len() as u32)?;
        self.write(&num)?;
        self.write(&bytes)?;
        Ok(())
    }

    fn single_write_bytes(&mut self, bytes : &[u8]) -> Result<(), MessageError> {
        // println!("STARTING single_write_bytes");
        let mut num : [u8;4] = [0;4];
        // println!("Writing {} bytes message [{}]", bytes.len(), bytes_to_hex(&bytes));
        (&mut num[..]).write_u32::<LittleEndian>(bytes.len() as u32)?;
        self.write(&num)?;
        self.write(&bytes)?;
        Ok(())
    }
}

// fn bytes_to_hex(bytes : &[u8]) -> String {
//     let mut s = String::new();
//     for b in bytes {
//         s.push_str(&format!("{:X}", b));
//     }
//     s
// }