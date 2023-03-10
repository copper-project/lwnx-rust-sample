use std::ffi::CString;

use winapi::{
    ctypes::c_void,
    shared::{
        minwindef::{DWORD, FALSE, TRUE},
        winerror::ERROR_IO_PENDING,
    },
    um::{
        commapi::{GetCommState, GetCommTimeouts, PurgeComm, SetCommState, SetCommTimeouts},
        errhandlingapi::GetLastError,
        fileapi::{CreateFileA, ReadFile, WriteFile, OPEN_EXISTING},
        handleapi::{CloseHandle, INVALID_HANDLE_VALUE},
        ioapiset::GetOverlappedResult,
        minwinbase::OVERLAPPED,
        winbase::{
            COMMTIMEOUTS, DCB, DTR_CONTROL_ENABLE, FILE_FLAG_OVERLAPPED, NOPARITY, ONESTOPBIT,
            PURGE_RXABORT, PURGE_RXCLEAR, PURGE_TXABORT, PURGE_TXCLEAR, RTS_CONTROL_ENABLE,
        },
        winnt::{GENERIC_READ, GENERIC_WRITE, HANDLE},
    },
};

#[derive(Debug)]
pub enum WinSerialPortError {
    InvalidSerialPort,
    WriteFailed,
    WaitingError,
    DidNotWriteAllBytes,
    ReadPendingError,
    ReadFailed,
}

impl From<WinSerialPortError> for String {
    fn from(value: WinSerialPortError) -> Self {
        std::format!("{:?}", value)
    }
}

pub struct WinSerialPort {
    handle: HANDLE,
}

impl Drop for WinSerialPort {
    fn drop(&mut self) {
        self.disconnect();
    }
}

impl WinSerialPort {
    pub fn new() -> WinSerialPort {
        WinSerialPort {
            handle: std::ptr::null_mut(),
        }
    }

    pub fn is_invalid(&self) -> bool {
        self.handle == INVALID_HANDLE_VALUE
    }

    pub fn connect(&mut self, port_name: &str, bit_rate: u32) -> Result<(), String> {
        println!("Attempt com connection: {}", port_name);

        self.handle = INVALID_HANDLE_VALUE;

        let port_name_cstr = CString::new(port_name).unwrap();

        let handle: HANDLE = unsafe {
            CreateFileA(
                port_name_cstr.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                std::ptr::null_mut(),
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED,
                std::ptr::null_mut(),
            )
        };

        if handle == INVALID_HANDLE_VALUE {
            return Err(std::format!("Could not open port {}", port_name));
        }

        unsafe {
            if PurgeComm(
                handle,
                PURGE_RXABORT | PURGE_RXCLEAR | PURGE_TXABORT | PURGE_TXCLEAR,
            ) != 1
            {
                return Err(String::from("Could not purge COM port"));
            }
        };

        let mut com_params = DCB::default();
        com_params.DCBlength = std::mem::size_of::<DCB>() as u32;

        unsafe {
            if GetCommState(handle, &mut com_params) != 1 {
                return Err(String::from("Could not get COM port state"));
            }
        };

        com_params.BaudRate = bit_rate;
        com_params.ByteSize = 8;
        com_params.StopBits = ONESTOPBIT;
        com_params.Parity = NOPARITY;
        com_params.set_fDtrControl(DTR_CONTROL_ENABLE);
        com_params.set_fRtsControl(RTS_CONTROL_ENABLE);

        // NOTE: Some USB<->Serial drivers require the state to be set twice.
        unsafe {
            if SetCommState(handle, &mut com_params) == FALSE {
                if SetCommState(handle, &mut com_params) == FALSE {
                    return Err(String::from("Could not set COM port state"));
                }
            }
        };

        let mut timeouts = COMMTIMEOUTS::default();

        unsafe {
            if GetCommTimeouts(handle, &mut timeouts) == FALSE {
                return Err(String::from("Could not get COM port timeouts"));
            }
        }

        timeouts.ReadIntervalTimeout = 0;
        timeouts.ReadTotalTimeoutMultiplier = 0;
        timeouts.ReadTotalTimeoutConstant = 10;

        unsafe {
            if SetCommTimeouts(handle, &mut timeouts) == FALSE {
                return Err(String::from("Could not set COM port timeouts"));
            }
        }

        self.handle = handle;

        println!("COM port connected: {}", port_name);

        Ok(())
    }

    pub fn disconnect(&mut self) {
        if self.handle != INVALID_HANDLE_VALUE {
            unsafe {
                CloseHandle(self.handle);
            }
        }

        self.handle = INVALID_HANDLE_VALUE;
    }

    pub fn write(&self, buffer: &[u8]) -> Result<u32, WinSerialPortError> {
        if self.is_invalid() {
            return Err(WinSerialPortError::InvalidSerialPort);
        }

        let mut overlapped = OVERLAPPED::default();
        let mut bytes_written: DWORD = 0;

        unsafe {
            if WriteFile(
                self.handle,
                buffer.as_ptr() as *const c_void,
                buffer.len() as u32,
                &mut bytes_written,
                &mut overlapped,
            ) == FALSE
            {
                if GetLastError() != ERROR_IO_PENDING {
                    return Err(WinSerialPortError::WriteFailed);
                } else {
                    if GetOverlappedResult(self.handle, &mut overlapped, &mut bytes_written, TRUE)
                        == FALSE
                    {
                        return Err(WinSerialPortError::WaitingError);
                    }
                }
            }

            if bytes_written != buffer.len() as u32 {
                return Err(WinSerialPortError::DidNotWriteAllBytes);
            }

            return Ok(bytes_written);
        }
    }

    pub fn read<'a>(&self, buffer: &'a mut [u8]) -> Result<&'a [u8], WinSerialPortError> {
        if self.is_invalid() {
            return Err(WinSerialPortError::InvalidSerialPort);
        }

        let mut overlapped = OVERLAPPED::default();
        let mut bytes_read: DWORD = 0;

        unsafe {
            if ReadFile(
                self.handle,
                buffer.as_mut_ptr() as *mut c_void,
                buffer.len() as u32,
                &mut bytes_read,
                &mut overlapped,
            ) == FALSE
            {
                if GetLastError() != ERROR_IO_PENDING {
                    return Err(WinSerialPortError::ReadPendingError);
                } else {
                    if GetOverlappedResult(self.handle, &mut overlapped, &mut bytes_read, TRUE)
                        == FALSE
                    {
                        return Err(WinSerialPortError::WaitingError);
                    }
                }
            }

            return Ok(&buffer[0..bytes_read as usize]);
        }
    }
}
