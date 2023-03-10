use std::{thread, time::Duration};

use serialport::{available_ports, SerialPort, SerialPortType};
use win32_serialport::WinSerialPort;

mod lwnx;
mod win32_serialport;

/// Implementation example for the Rust serialport crate.
impl lwnx::UserPlatform for Box<dyn SerialPort> {
    fn write_callback(&mut self, data: &[u8]) -> Result<usize, lwnx::LwnxError> {
        match self.write(data) {
            Ok(bytes_written) => Ok(bytes_written),
            Err(_) => Err(lwnx::LwnxError::DeviceError),
        }
    }

    fn read_callback<'a>(&mut self, data: &'a mut [u8]) -> Result<&'a [u8], lwnx::LwnxError> {
        match self.read(data) {
            Ok(bytes_read) => Ok(&data[0..bytes_read]),
            Err(_) => Err(lwnx::LwnxError::DeviceError),
        }
    }

    fn delay_callback(&mut self, duration_ms: u64) {
        thread::sleep(Duration::from_millis(duration_ms));
    }
}

/// Implementation example for the LightWare serial port implementation.
impl lwnx::UserPlatform for &WinSerialPort {
    fn write_callback(&mut self, data: &[u8]) -> Result<usize, lwnx::LwnxError> {
        match self.write(data) {
            Ok(bytes_written) => Ok(bytes_written as usize),
            Err(_) => Err(lwnx::LwnxError::DeviceError),
        }
    }

    fn read_callback<'a>(&mut self, data: &'a mut [u8]) -> Result<&'a [u8], lwnx::LwnxError> {
        match self.read(data) {
            Ok(bytes) => Ok(bytes),
            Err(_) => Err(lwnx::LwnxError::DeviceError),
        }
    }

    fn delay_callback(&mut self, duration_ms: u64) {
        thread::sleep(Duration::from_millis(duration_ms));
    }
}

/// Implementation example for a user struct that references a serial port.
struct MyPlatform<'a> {
    port: &'a WinSerialPort,
    trace_packet: bool,
}

impl lwnx::UserPlatform for &MyPlatform<'_> {
    fn write_callback(&mut self, data: &[u8]) -> Result<usize, lwnx::LwnxError> {
        if self.trace_packet {
            println!("Writing bytes: {:X?}", data);
        }
        match self.port.write(data) {
            Ok(bytes_written) => Ok(bytes_written as usize),
            Err(_) => Err(lwnx::LwnxError::DeviceError),
        }
    }

    fn read_callback<'a>(&mut self, data: &'a mut [u8]) -> Result<&'a [u8], lwnx::LwnxError> {
        match self.port.read(data) {
            Ok(bytes) => {
                if self.trace_packet {
                    println!("Read: {:X?}", bytes);
                }
                Ok(bytes)
            }
            Err(_) => Err(lwnx::LwnxError::DeviceError),
        }
    }

    fn delay_callback(&mut self, duration_ms: u64) {
        if self.trace_packet {
            println!("Delay for: {} ms", duration_ms);
        }
        thread::sleep(Duration::from_millis(duration_ms));
    }
}

fn main() -> Result<(), String> {
    println!("Serial port list:");

    let ports = available_ports().unwrap();

    for p in ports {
        println!("{} {:?}", p.port_name, p.port_type);

        match p.port_type {
            SerialPortType::UsbPort(info) => println!("Serial: {:?}", info.serial_number),
            _ => (),
        };
    }

    let mut port = WinSerialPort::new();
    port.connect("COM5", 921600)?;
    // let mut device_context = lwnx::DeviceContext::new(&port);

    let my_platform = MyPlatform {
        port: &port,
        trace_packet: true,
    };
    let mut device_context = lwnx::DeviceContext::new(&my_platform);

    // let mut port = serialport::new("COM5", 921600)
    //     .timeout(Duration::from_millis(1))
    //     .open()
    //     .expect("Failed to open port");

    // // NOTE: Apparently DTR change only required on Windows.
    // port.write_data_terminal_ready(true).unwrap();
    // // let mut platform_context = lwnx::PlatformContext::new(port);
    // let mut device_context = lwnx::DeviceContext::new(port);

    // Attempt to start LWNX mode.
    lwnx::engage_lwnx_mode(&mut device_context)?;

    let model_name = lwnx::cmd_read_string(&mut device_context, 0)?;
    println!("Model name: {}", model_name);

    let hardware_version = lwnx::cmd_read_u32(&mut device_context, 1)?;
    println!("Hardware version: {}", hardware_version);

    let firmware_version = lwnx::cmd_read_u32(&mut device_context, 2)?;
    println!("Firmware version: {}", firmware_version);

    let serial_number = lwnx::cmd_read_string(&mut device_context, 3)?;
    println!("Serial number: {}", serial_number);

    let user_data = lwnx::cmd_read_string(&mut device_context, 9)?;
    println!("User data: {}", user_data);

    let distance_output = lwnx::cmd_read_u32(&mut device_context, 27)?;
    println!("Distance output: {}", distance_output);

    Ok(())
}
