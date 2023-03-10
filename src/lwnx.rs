use std::time::Instant;

#[derive(Debug)]
pub enum LwnxError {
    DeviceError,
    ReadError,
    WriteError,
    DeviceClosed,
    PacketTimeout,
    CommandRetriesExhausted,
}

impl From<LwnxError> for String {
    fn from(value: LwnxError) -> Self {
        std::format!("{:?}", value)
    }
}

/// Creates a packet CRC.
pub fn create_crc(data: &[u8]) -> u16 {
    let mut crc: u16 = 0;

    for b in data {
        let mut code = crc >> 8;
        code ^= *b as u16;
        code ^= code >> 4;
        crc = crc << 8;
        crc ^= code;
        code = code << 5;
        crc ^= code;
        code = code << 7;
        crc ^= code;
    }

    return crc;
}

/// Fills a buffer with bytes that describe a packet.
pub fn create_packet_bytes<'a>(
    buffer: &'a mut [u8],
    command_id: u8,
    write: bool,
    data: &[u8],
) -> &'a [u8] {
    let data_size = data.len();
    let payload_length = 1 + data_size as i32;

    let flags: u16 = match write {
        true => ((payload_length << 6) | (0x1)) as u16,
        false => (payload_length << 6) as u16,
    };

    buffer[0] = 0xAA;
    buffer[1..3].copy_from_slice(&flags.to_le_bytes());
    buffer[3] = command_id;
    buffer[4..4 + data_size].copy_from_slice(data);

    let crc = create_crc(&buffer[0..=3 + data_size]);
    buffer[4 + data_size..6 + data_size].copy_from_slice(&crc.to_le_bytes());

    return &buffer[0..6 + data_size];
}

enum ResponseParseState {
    StartByte,
    PayloadSize0,
    PayloadSize1,
    Payload,
}

pub struct Response {
    data: [u8; 1024],
    size: i32,
    payload_size: i32,
    parse_state: ResponseParseState,
}

impl Response {
    pub fn new() -> Response {
        Response {
            data: [0u8; 1024],
            size: 0,
            payload_size: 0,
            parse_state: ResponseParseState::StartByte,
        }
    }

    pub fn reset(&mut self) {
        self.size = 0;
        self.payload_size = 0;
        self.parse_state = ResponseParseState::StartByte;
    }

    pub fn get_command(&self) -> u8 {
        self.data[3]
    }

    pub fn get_size(&self) -> i32 {
        self.size
    }

    pub fn get_string_data(&self) -> Option<String> {
        if let Ok(s) = std::str::from_utf8(&self.data[4..20]) {
            Some(s.to_owned())
        } else {
            None
        }
    }

    pub fn get_uint32_data(&self) -> u32 {
        u32::from_le_bytes(self.data[4..8].try_into().unwrap())
    }

    pub fn parse_data(&mut self, data: u8) -> bool {
        match self.parse_state {
            ResponseParseState::StartByte => {
                if data == 0xAA {
                    self.parse_state = ResponseParseState::PayloadSize0;
                    self.data[0] = data;
                }
            }
            ResponseParseState::PayloadSize0 => {
                self.parse_state = ResponseParseState::PayloadSize1;
                self.data[1] = data;
            }
            ResponseParseState::PayloadSize1 => {
                self.parse_state = ResponseParseState::Payload;
                self.data[2] = data;
                self.payload_size = (self.data[1] as i32 | ((self.data[2] as i32) << 8)) >> 6;
                self.payload_size += 2;
                self.size = 3;

                if self.payload_size > 1019 {
                    self.parse_state = ResponseParseState::StartByte;
                }
            }
            ResponseParseState::Payload => {
                self.data[self.size as usize] = data;
                self.size += 1;
                self.payload_size -= 1;

                if self.payload_size == 0 {
                    self.parse_state = ResponseParseState::StartByte;
                    let crc = self.data[(self.size - 2) as usize] as u16
                        | ((self.data[(self.size - 1) as usize] as u16) << 8);
                    let verify_crc = create_crc(&self.data[..(self.size - 2) as usize]);

                    if crc == verify_crc {
                        return true;
                    } else {
                        println!("Packet has invalid CRC");
                    }
                }
            }
        }
        return false;
    }
}

pub trait UserPlatform {
    fn write_callback(&mut self, data: &[u8]) -> Result<usize, LwnxError>;
    fn read_callback<'a>(&mut self, data: &'a mut [u8]) -> Result<&'a [u8], LwnxError>;
    fn delay_callback(&mut self, duration_ms: u64);
}

pub struct DeviceContext<T: UserPlatform> {
    pub user_platform: T,
    pub command_timeout: u64,
    pub command_retries: i32,
}

impl<T: UserPlatform> DeviceContext<T> {
    pub fn new(user_platform: T) -> DeviceContext<T> {
        DeviceContext {
            user_platform,
            command_timeout: 500,
            command_retries: 4,
        }
    }
}

/// Sends a command 0 packet to alert the device that LWNX mode is required.
///
/// **Note**: Does not consume any packet response if there is one.
pub fn engage_lwnx_mode<T: UserPlatform>(
    device_context: &mut DeviceContext<T>,
) -> Result<(), LwnxError> {
    let mut packet_buffer = [0u8; 1024];
    let packet_bytes = create_packet_bytes(&mut packet_buffer, 0, false, &[]);

    match cmd_write(device_context, packet_bytes) {
        Ok(_) => Ok(()),
        Err(s) => Err(s),
    }
}

pub fn cmd_read<'a, T: UserPlatform>(
    platform: &mut DeviceContext<T>,
    buffer: &'a mut [u8],
) -> Result<&'a [u8], LwnxError> {
    let read_result = platform.user_platform.read_callback(buffer);
    if let Ok(s) = read_result {
        return Ok(s);
    }

    Err(LwnxError::ReadError)
}

pub fn cmd_write<T: UserPlatform>(
    device_context: &mut DeviceContext<T>,
    buffer: &[u8],
) -> Result<usize, LwnxError> {
    if let Ok(s) = device_context.user_platform.write_callback(buffer) {
        return Ok(s);
    }

    Err(LwnxError::WriteError)
}

pub fn recv_packet<T: UserPlatform>(
    device_context: &mut DeviceContext<T>,
    command_id: u8,
    response: &mut Response,
    timeout: u64,
) -> Result<(), LwnxError> {
    let mut byte = [0u8];

    response.reset();

    let instant_time = Instant::now();
    let timeout_time = instant_time.elapsed().as_millis() as u64 + timeout;

    while (instant_time.elapsed().as_millis() as u64) < timeout_time {
        let byte_read = cmd_read(device_context, &mut byte[..])?;

        if byte_read.len() > 0 {
            if response.parse_data(byte_read[0]) {
                if response.get_command() == command_id {
                    return Ok(());
                }
            }
        }
    }

    Err(LwnxError::PacketTimeout)
}

pub fn handle_managed_cmd<T: UserPlatform>(
    device_context: &mut DeviceContext<T>,
    command_id: u8,
    write: bool,
    write_data: &[u8],
    response: &mut Response,
) -> Result<(), LwnxError> {
    let mut packet_buffer = [0u8; 1024];
    let packet_bytes = create_packet_bytes(&mut packet_buffer, command_id, write, write_data);

    for _ in 0..device_context.command_retries {
        cmd_write(device_context, packet_bytes)?;
        match recv_packet(
            device_context,
            command_id,
            response,
            device_context.command_timeout,
        ) {
            Ok(_) => return Ok(()),
            Err(LwnxError::PacketTimeout) => continue,
            Err(e) => return Err(e),
        }
    }

    Err(LwnxError::CommandRetriesExhausted)
}

pub fn cmd_read_i8<T: UserPlatform>(
    device_context: &mut DeviceContext<T>,
    command_id: u8,
) -> Result<i8, LwnxError> {
    let mut response = Response::new();
    handle_managed_cmd(device_context, command_id, false, &[], &mut response)?;
    Ok(response.data[4] as i8)
}

pub fn cmd_read_i16<T: UserPlatform>(
    device_context: &mut DeviceContext<T>,
    command_id: u8,
) -> Result<i16, LwnxError> {
    let mut response = Response::new();
    handle_managed_cmd(device_context, command_id, false, &[], &mut response)?;
    Ok(i16::from_le_bytes(response.data[4..6].try_into().unwrap()))
}

pub fn cmd_read_i32<T: UserPlatform>(
    device_context: &mut DeviceContext<T>,
    command_id: u8,
) -> Result<i32, LwnxError> {
    let mut response = Response::new();
    handle_managed_cmd(device_context, command_id, false, &[], &mut response)?;
    Ok(i32::from_le_bytes(response.data[4..8].try_into().unwrap()))
}

pub fn cmd_read_u8<T: UserPlatform>(
    device_context: &mut DeviceContext<T>,
    command_id: u8,
) -> Result<u8, LwnxError> {
    let mut response = Response::new();
    handle_managed_cmd(device_context, command_id, false, &[], &mut response)?;
    Ok(response.data[4])
}

pub fn cmd_read_u16<T: UserPlatform>(
    device_context: &mut DeviceContext<T>,
    command_id: u8,
) -> Result<u16, LwnxError> {
    let mut response = Response::new();
    handle_managed_cmd(device_context, command_id, false, &[], &mut response)?;
    Ok(u16::from_le_bytes(response.data[4..6].try_into().unwrap()))
}

pub fn cmd_read_u32<T: UserPlatform>(
    device_context: &mut DeviceContext<T>,
    command_id: u8,
) -> Result<u32, LwnxError> {
    let mut response = Response::new();
    handle_managed_cmd(device_context, command_id, false, &[], &mut response)?;
    Ok(u32::from_le_bytes(response.data[4..8].try_into().unwrap()))
}

pub fn cmd_read_string<T: UserPlatform>(
    device_context: &mut DeviceContext<T>,
    command_id: u8,
) -> Result<String, LwnxError> {
    let mut response = Response::new();
    handle_managed_cmd(device_context, command_id, false, &[], &mut response)?;

    let mut str_len = 0;
    for (index, c) in response.data[4..20].iter().enumerate() {
        if *c == 0 {
            str_len = index;
            break;
        }
    }

    Ok(std::str::from_utf8(&response.data[4..4 + str_len])
        .unwrap()
        .to_owned())
}

pub fn cmd_read_data<T: UserPlatform>(
    device_context: &mut DeviceContext<T>,
    command_id: u8,
    buffer: &mut [u8],
) -> Result<(), LwnxError> {
    let mut response = Response::new();
    handle_managed_cmd(device_context, command_id, false, &[], &mut response)?;
    buffer.copy_from_slice(&response.data[4..4 + buffer.len()]);
    Ok(())
}
