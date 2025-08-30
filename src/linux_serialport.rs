use std::{io::{Read, Write}, time::Duration};
use serialport::{self, DataBits, FlowControl, Parity, SerialPort, StopBits};

#[derive(Debug)]
pub enum LinuxSerialPortError {
    InvalidSerialPort,
    OpenFailed,
    WriteFailed,
    DidNotWriteAllBytes,
    ReadFailed,
}

impl From<LinuxSerialPortError> for String {
    fn from(v: LinuxSerialPortError) -> Self { format!("{v:?}") }
}

pub struct LinuxSerialPort {
    port: Option<Box<dyn SerialPort>>,
}

impl LinuxSerialPort {
    pub fn new() -> Self { Self { port: None } }
    pub fn is_invalid(&self) -> bool { self.port.is_none() }

    pub fn connect(&mut self, path: &str, bit_rate: u32) -> Result<(), String> {
        let p = serialport::new(path, bit_rate)
            .data_bits(DataBits::Eight)
            .parity(Parity::None)
            .stop_bits(StopBits::One)
            .flow_control(FlowControl::None)
            .timeout(Duration::from_millis(10))
            .open()
            .map_err(|_| String::from(LinuxSerialPortError::OpenFailed))?;
        self.port = Some(p);
        Ok(())
    }

    pub fn disconnect(&mut self) { self.port = None; }

    pub fn write(&mut self, buffer: &[u8]) -> Result<u32, LinuxSerialPortError> {
        let p = self.port.as_mut().ok_or(LinuxSerialPortError::InvalidSerialPort)?;
        let mut total = 0;
        while total < buffer.len() {
            match p.write(&buffer[total..]) {
                Ok(n) if n > 0 => total += n,
                Ok(_) => return Err(LinuxSerialPortError::WriteFailed),
                Err(_) => return Err(LinuxSerialPortError::WriteFailed),
            }
        }
        if total != buffer.len() { return Err(LinuxSerialPortError::DidNotWriteAllBytes); }
        Ok(total as u32)
    }

    pub fn read<'a>(&mut self, buf: &'a mut [u8]) -> Result<&'a [u8], LinuxSerialPortError> {
        let p = self.port.as_mut().ok_or(LinuxSerialPortError::InvalidSerialPort)?;
        match p.read(buf) {
            Ok(n) => Ok(&buf[..n]),
            Err(_) => Err(LinuxSerialPortError::ReadFailed),
        }
    }
}
