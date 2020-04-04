use probe_rs::{config::MemoryRegion, Core, Session};
use std::cmp::min;
use std::collections::BTreeMap;
use std::convert::TryInto;
use thiserror::Error;

pub struct Rtt<'c> {
    ptr: u32,
    core: &'c Core,
    up_channels: BTreeMap<usize, RttChannel>,
    down_channels: BTreeMap<usize, RttChannel>,
}

// Rtt must follow this data layout when reading/writing memory in order to be compatible with the
// official RTT implementation.
//
// // The RTT "control block"
// struct Rtt {
//     char id[16]; // Used to find/validate the control block. Must equal "SEGGER RTT\0\0\0\0\0\0".
//     // Maximum number of up (target to host) channels in following array
//     unsigned int max_up_channels;
//     // Maximum number of down (host to target) channels in following array.
//     unsigned int max_down_channels;
//     RttChannel up_channels[max_up_channels]; // Array of up (target to host) channels.
//     RttChannel down_channels[max_down_channels]; // array of down (host to target) channels.
// }

impl Rtt<'_> {
    // Minimum size of struct in target memory in bytes with empty arrays
    const MIN_SIZE: usize = Self::O_CHANNEL_ARRAYS;

    // Offsets of fields in target memory in bytes
    const O_ID: usize = 0;
    const O_MAX_UP_CHANNELS: usize = 16;
    const O_MAX_DOWN_CHANNELS: usize = 20;
    const O_CHANNEL_ARRAYS: usize = 24;

    const RTT_ID: [u8; 16] = *b"SEGGER RTT\0\0\0\0\0\0";

    fn from<'c>(
        core: &'c Core,
        memory_map: &[MemoryRegion],
        ptr: u32,
        mem: &[u8],
    ) -> Result<Option<Rtt<'c>>, Error> {
        // Validate that the control block starts with the ID bytes
        if mem[Self::O_ID..(Self::O_ID + Self::RTT_ID.len())] != Self::RTT_ID {
            return Ok(None);
        }

        let max_up_channels = mem.get_u32(Self::O_MAX_UP_CHANNELS) as usize;
        let max_down_channels = mem.get_u32(Self::O_MAX_DOWN_CHANNELS) as usize;

        // Validate that the entire control block fits within the region
        if Self::O_CHANNEL_ARRAYS + (max_up_channels + max_down_channels) * RttChannel::SIZE
            >= mem.len()
        {
            return Ok(None);
        }

        let mut rtt = Rtt {
            ptr,
            core,
            up_channels: BTreeMap::new(),
            down_channels: BTreeMap::new(),
        };

        for i in 0..max_up_channels {
            let offset = Self::O_CHANNEL_ARRAYS + i * RttChannel::SIZE;

            if let Some(buf) = RttChannel::from(
                i,
                Direction::Up,
                core,
                memory_map,
                ptr + offset as u32,
                &mem[offset..],
            )? {
                rtt.up_channels.insert(i, buf);
            }
        }

        for i in 0..max_down_channels {
            let offset = Self::O_CHANNEL_ARRAYS
                + (max_up_channels * RttChannel::SIZE)
                + i * RttChannel::SIZE;

            if let Some(buf) = RttChannel::from(
                i,
                Direction::Down,
                core,
                memory_map,
                ptr + offset as u32,
                &mem[offset..],
            )? {
                rtt.down_channels.insert(i, buf);
            }
        }

        Ok(Some(rtt))
    }

    pub fn attach<'c>(core: &'c Core, session: &Session) -> Result<Rtt<'c>, Error> {
        let mut mem: Vec<u8> = Vec::new();
        let mut instances: Vec<Rtt> = Vec::new();
        let memory_map: &[MemoryRegion] = &*session.memory_map();

        'out: for region in memory_map.iter() {
            if let MemoryRegion::Ram(ram) = region {
                let range = &ram.range;

                mem.resize((range.end - range.start) as usize, 0);
                core.read_8(range.start, mem.as_mut())?;

                for offset in 0..(mem.len() - Self::MIN_SIZE) {
                    if let Some(rtt) = Rtt::from(
                        core,
                        memory_map,
                        range.start + offset as u32,
                        &mem[offset..],
                    )? {
                        instances.push(rtt);

                        if instances.len() > 5 {
                            break 'out;
                        }
                    }
                }
            }
        }

        if instances.len() == 0 {
            return Err(Error::ControlBlockNotFound);
        }

        if instances.len() > 1 {
            return Err(Error::MultipleControlBlocksFound(
                instances.into_iter().map(|i| i.ptr).collect(),
            ));
        }

        Ok(instances.remove(0))
    }

    /// Retrieves information about available up (target to host) channels.
    pub fn up_channels(&self) -> &BTreeMap<usize, RttChannel> {
        &self.up_channels
    }

    /// Retrieves information about available down (host to target) channels.
    pub fn down_channels(&self) -> &BTreeMap<usize, RttChannel> {
        &self.down_channels
    }

    /// Reads bytes from an up (target to host) channel and returns the number of bytes read.
    pub fn read(&mut self, channel: usize, data: &mut [u8]) -> Result<usize, Error> {
        let core = self.core;

        self.up_channels
            .get(&channel)
            .ok_or_else(|| Error::NoSuchChannel)
            .and_then(|buf| buf.read(core, data))
    }

    /// Writes bytes to a down (host to target) channel and returns the number of bytes written.
    pub fn write(&mut self, channel: usize, data: &[u8]) -> Result<usize, Error> {
        let core = self.core;

        self.down_channels
            .get(&channel)
            .ok_or_else(|| Error::NoSuchChannel)
            .and_then(|buf| buf.write(core, data))
    }
}

pub struct RttChannel {
    number: usize,
    direction: Direction,
    ptr: u32,
    name: Option<String>,
    buffer_ptr: u32,
    size: u32,
    flags: u32,
}

// RttChannel must follow this data layout when reading/writing memory in order to be compatible with
// the official RTT implementation.
//
// struct RttChannel {
//     const char *name; // Name of channel, pointer to null-terminated string. Optional.
//     char *buffer; // Pointer to buffer data
//     unsigned int size; // Size of data buffer. The actual capacity is one byte less.
//     unsigned int write; // Offset in data buffer of next byte to write.
//     unsigned int read; // Offset in data buffer of next byte to read.
//     // The low 2 bits of flags are used for blocking/non blocking modes, the rest are ignored.
//     unsigned int flags;
// }

impl RttChannel {
    // Size of this struct in target memory in bytes
    const SIZE: usize = 24;

    // Offsets of fields in target memory in bytes
    const O_NAME: usize = 0;
    const O_BUFFER_PTR: usize = 4;
    const O_SIZE: usize = 8;
    const O_WRITE: usize = 12;
    const O_READ: usize = 16;
    const O_FLAGS: usize = 20;

    fn from(
        number: usize,
        direction: Direction,
        core: &Core,
        memory_map: &[MemoryRegion],
        ptr: u32,
        mem: &[u8],
    ) -> Result<Option<RttChannel>, Error> {
        let buffer_ptr = mem.get_u32(Self::O_BUFFER_PTR);
        if buffer_ptr == 0 {
            // This buffer isn't in use
            return Ok(None);
        }

        let name_ptr = mem.get_u32(Self::O_NAME);

        let name = if name_ptr == 0 {
            None
        } else {
            read_c_string(core, memory_map, name_ptr)?
        };

        Ok(Some(RttChannel {
            number,
            direction,
            ptr,
            name,
            buffer_ptr: mem.get_u32(Self::O_BUFFER_PTR),
            size: mem.get_u32(Self::O_SIZE),
            flags: mem.get_u32(Self::O_FLAGS),
        }))
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_ref().map(|s| s.as_ref())
    }

    pub fn size(&self) -> usize {
        self.buffer_size() - 1
    }

    pub fn buffer_size(&self) -> usize {
        self.size as usize
    }

    pub fn flags(&self) -> u32 {
        self.flags
    }

    pub fn mode(&self) -> RttChannelMode {
        match self.flags & 0x3 {
            0 => RttChannelMode::NoBlockSkip,
            1 => RttChannelMode::NoBlockTrim,
            2 => RttChannelMode::BlockIfFifoFull,
            _ => RttChannelMode::Invalid,
        }
    }

    // This method should only be called for up channels.
    fn read(&self, core: &Core, mut buf: &mut [u8]) -> Result<usize, Error> {
        let (write, mut read) = self.read_pointers(core)?;

        if self.readable_contiguous(write, read) == 0 {
            // Buffer is empty - do nothing.
            return Ok(0);
        }

        let mut total = 0;

        // Read while buffer contains data and output buffer has space (maximum of two iterations)
        while buf.len() > 0 {
            let count = min(self.readable_contiguous(write, read), buf.len());
            if count == 0 {
                break;
            }

            core.read_8(self.buffer_ptr + read, &mut buf[..count])?;

            total += count;
            read += count as u32;

            if read >= self.size {
                // Wrap around to start
                read = 0;
            }

            buf = &mut buf[count..];
        }

        // Write read pointer back to target
        core.write_8(self.ptr + Self::O_READ as u32, &read.to_le_bytes())?;

        Ok(total)
    }

    // This method should only be called for down channels.
    fn write(&self, core: &Core, mut buf: &[u8]) -> Result<usize, Error> {
        let (mut write, read) = self.read_pointers(core)?;

        if self.writable_contiguous(write, read) == 0 {
            // Buffer is full - do nothing.
            return Ok(0);
        }

        let mut total = 0;

        // Write while buffer has space for data and output contains data (maximum of two iterations)
        while buf.len() > 0 {
            let count = min(self.writable_contiguous(write, read), buf.len());
            if count == 0 {
                break;
            }

            core.write_8(self.buffer_ptr + write, &buf[..count])?;

            total += count;
            write += count as u32;

            if write >= self.size {
                // Wrap around to start
                write = 0;
            }

            buf = &buf[count..];
        }

        // Write write pointer back to target
        core.write_8(self.ptr + Self::O_WRITE as u32, &write.to_le_bytes())?;

        Ok(total)
    }

    /// Calculates amount of contiguous data available for reading
    fn readable_contiguous(&self, write: u32, read: u32) -> usize {
        (if read > write {
            self.size - read
        } else {
            write - read
        }) as usize
    }

    /// Calculates amount of contiguous space available for writing
    fn writable_contiguous(&self, write: u32, read: u32) -> usize {
        (if read > write {
            read - write - 1
        } else {
            self.size - write
        }) as usize
    }

    fn read_pointers(&self, core: &Core) -> Result<(u32, u32), Error> {
        let mut block = [0u8; 8];
        core.read_8(self.ptr + Self::O_WRITE as u32, block.as_mut())?;

        let write = block.as_ref().get_u32(0);
        let read = block.as_ref().get_u32(4);

        let validate = |which, value| {
            if value >= self.size {
                Err(Error::ControlBlockCorrupted(format!(
                    "{} pointer is {} while buffer size is {} for {:?} channel {} ({})",
                    which,
                    value,
                    self.size,
                    self.direction,
                    self.number,
                    self.name().unwrap_or("no name"),
                )))
            } else {
                Ok(())
            }
        };

        validate("write", write)?;
        validate("read", read)?;

        Ok((write, read))
    }
}

/// Reads a null-terminated string from target memory. Lossy UTF-8 decoding is used.
fn read_c_string(
    core: &Core,
    memory_map: &[MemoryRegion],
    ptr: u32,
) -> Result<Option<String>, Error> {
    // Find out which memory range contains the pointer
    let range = memory_map
        .iter()
        .filter_map(|r| match r {
            MemoryRegion::Flash(r) => Some(&r.range),
            MemoryRegion::Ram(r) => Some(&r.range),
            _ => None,
        })
        .find(|r| r.contains(&ptr));

    // If the pointer is not within any valid range, return None.
    let range = match range {
        Some(r) => r,
        None => return Ok(None),
    };

    // Read up to 128 bytes not going past the end of the region
    let mut bytes = vec![0u8; min(128, (range.end - ptr) as usize)];
    core.read_8(ptr, bytes.as_mut())?;

    // If the bytes read contain a null, return the preceding part as a string, otherwise None.
    Ok(bytes
        .iter()
        .position(|&b| b == 0)
        .map(|p| String::from_utf8_lossy(&bytes[..p]).into_owned()))
}

pub enum RttChannelMode {
    NoBlockSkip,
    NoBlockTrim,
    BlockIfFifoFull,
    Invalid,
}

#[derive(Debug)]
enum Direction {
    Up,
    Down,
}

trait SliceExt {
    fn get_u32(&self, offset: usize) -> u32;
}

impl SliceExt for &[u8] {
    fn get_u32(&self, offset: usize) -> u32 {
        u32::from_le_bytes(
            (&self[offset..offset + 4])
                .try_into()
                .expect("Invalid read offset"),
        )
    }
}

#[derive(Error, Debug)]
pub enum Error {
    #[error(
        "RTT control block not found in memory. Make sure you've initialized RTT on the target."
    )]
    ControlBlockNotFound,

    #[error("Multiple control blocks found in memory.")]
    MultipleControlBlocksFound(Vec<u32>),

    #[error("Invalid channel number.")]
    NoSuchChannel,

    #[error("Control block corrupted: {0}")]
    ControlBlockCorrupted(String),

    #[error("Error communicating with probe: {0}")]
    Probe(#[from] probe_rs::Error),
}
