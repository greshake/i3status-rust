use crate::errors::*;

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

pub struct ApcAccess {
    socket_addr: SocketAddr,
    timeout_seconds: u64,
}

impl ApcAccess {
    pub fn new(addr: &str, timeout_seconds: u64) -> Result<ApcAccess> {
        Ok(ApcAccess {
            socket_addr: addr.to_socket_addrs()?.find(|x| x.is_ipv4()).unwrap(),
            timeout_seconds,
        })
    }

    fn connect(&self) -> Result<TcpStream> {
        let stream =
            TcpStream::connect_timeout(&self.socket_addr, Duration::new(self.timeout_seconds, 0))?;
        stream.set_write_timeout(Some(Duration::new(self.timeout_seconds, 0)))?;
        stream.set_read_timeout(Some(Duration::new(self.timeout_seconds, 0)))?;

        Ok(stream)
    }

    pub fn is_available(&self, status_result: &Result<HashMap<String, String>>) -> bool {
        if let Ok(status_data) = status_result {
            if let Some(status) = status_data.get("STATUS") {
                return !status.contains("COMMLOST");
            }
        }
        false
    }

    pub fn get_status(&self) -> Result<HashMap<String, String>> {
        let mut stream = self.connect()?;

        self.write(&mut stream, b"status")?;

        let mut result = String::new();
        self.read_to_string(&mut stream, &mut result)?;

        let mut status_data: HashMap<String, String> = HashMap::new();
        for line in result.lines() {
            let (key, value) = line.split_once(':').unwrap();
            status_data.insert(String::from(key.trim()), String::from(value.trim()));
        }

        Ok(status_data)
    }

    fn write(&self, stream: &mut TcpStream, msg: &[u8]) -> Result<()> {
        let msg_len = msg.len();
        if msg_len >= 1 << 16 {
            return Err(InternalError(
                "apcaccess".to_string(),
                "msg is too long, it must be less than 2^16 characters long".to_string(),
                None,
            ));
        }

        stream.write_all(&msg_len.to_be_bytes()[6..])?;
        stream.write_all(msg)?;

        Ok(())
    }

    fn read_to_string(&self, stream: &mut TcpStream, buf: &mut String) -> Result<usize> {
        let mut read_info = [0_u8, 2];
        loop {
            stream.read_exact(&mut read_info)?;
            let read_size = ((read_info[0] as usize) << 8) + (read_info[1] as usize);
            if read_size == 0 {
                break;
            }
            let mut read_buf: Vec<u8> = vec![0; read_size];
            stream.read_exact(&mut read_buf)?;
            buf.extend(String::from_utf8(read_buf));
        }
        Ok(buf.len())
    }
}
