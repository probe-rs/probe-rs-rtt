use probe_rs::{config::MemoryRegion, Core, Session};
use scroll::{Pread, LE};
use std::collections::BTreeMap;
use std::rc::Rc;

use crate::channel::*;
use crate::Error;

/// The RTT interface.
///
/// Use [`Rtt::attach`] to attach to a probe-rs `Core` and detect channels.
pub struct Rtt {
    ptr: u32,
    up_channels: Channels<UpChannel>,
    down_channels: Channels<DownChannel>,
}

// Rtt must follow this data layout when reading/writing memory in order to be compatible with the
// official RTT implementation.
//
// struct ControlBlock {
//     char id[16]; // Used to find/validate the control block.
//     // Maximum number of up (target to host) channels in following array
//     unsigned int max_up_channels;
//     // Maximum number of down (host to target) channels in following array.
//     unsigned int max_down_channels;
//     RttChannel up_channels[max_up_channels]; // Array of up (target to host) channels.
//     RttChannel down_channels[max_down_channels]; // array of down (host to target) channels.
// }

impl Rtt {
    const RTT_ID: [u8; 16] = *b"SEGGER RTT\0\0\0\0\0\0";

    // Minimum size of the ControlBlock struct in target memory in bytes with empty arrays
    const MIN_SIZE: usize = Self::O_CHANNEL_ARRAYS;

    // Offsets of fields in target memory in bytes
    const O_ID: usize = 0;
    const O_MAX_UP_CHANNELS: usize = 16;
    const O_MAX_DOWN_CHANNELS: usize = 20;
    const O_CHANNEL_ARRAYS: usize = 24;

    fn from(
        core: &Rc<Core>,
        memory_map: &[MemoryRegion],
        ptr: u32,
        mem: &[u8],
    ) -> Result<Option<Rtt>, Error> {
        // Validate that the control block starts with the ID bytes
        if mem[Self::O_ID..(Self::O_ID + Self::RTT_ID.len())] != Self::RTT_ID {
            return Ok(None);
        }

        let max_up_channels = mem.pread_with::<u32>(Self::O_MAX_UP_CHANNELS, LE).unwrap() as usize;
        let max_down_channels = mem
            .pread_with::<u32>(Self::O_MAX_DOWN_CHANNELS, LE)
            .unwrap() as usize;

        // Validate that the entire control block fits within the region
        if Self::O_CHANNEL_ARRAYS + (max_up_channels + max_down_channels) * Channel::SIZE
            >= mem.len()
        {
            return Ok(None);
        }

        let mut up_channels = BTreeMap::new();
        let mut down_channels = BTreeMap::new();

        for i in 0..max_up_channels {
            let offset = Self::O_CHANNEL_ARRAYS + i * Channel::SIZE;

            if let Some(chan) =
                Channel::from(&core, i, memory_map, ptr + offset as u32, &mem[offset..])?
            {
                up_channels.insert(i, UpChannel(chan));
            }
        }

        for i in 0..max_down_channels {
            let offset =
                Self::O_CHANNEL_ARRAYS + (max_up_channels * Channel::SIZE) + i * Channel::SIZE;

            if let Some(chan) =
                Channel::from(&core, i, memory_map, ptr + offset as u32, &mem[offset..])?
            {
                down_channels.insert(i, DownChannel(chan));
            }
        }

        Ok(Some(Rtt {
            ptr,
            up_channels: Channels(up_channels),
            down_channels: Channels(down_channels),
        }))
    }

    /// Attempts to detect an RTT control block in the core memory and returns an instance if a
    /// valid control block was found.
    ///
    /// `core` can be e.g. an owned `Core` or a shared `Rc<Core>`. The session is only borrowed
    /// temporarily during detection.
    pub fn attach(core: impl Into<Rc<Core>>, session: &Session) -> Result<Rtt, Error> {
        let core = core.into();
        let memory_map: &[MemoryRegion] = &*session.memory_map();

        let mut mem: Vec<u8> = Vec::new();
        let mut instances: Vec<Rtt> = Vec::new();

        'out: for region in memory_map.iter() {
            if let MemoryRegion::Ram(ram) = region {
                let range = &ram.range;

                mem.resize((range.end - range.start) as usize, 0);
                core.read_8(range.start, mem.as_mut())?;

                for offset in 0..(mem.len() - Self::MIN_SIZE) {
                    if let Some(rtt) = Rtt::from(
                        &core,
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

    /// Returns the memory address of the control block in target memory.
    pub fn ptr(&self) -> u32 {
        self.ptr
    }

    /// Gets the detected up channels.
    pub fn up_channels(&mut self) -> &mut Channels<UpChannel> {
        &mut self.up_channels
    }

    /// Gets the detected down channels.
    pub fn down_channels(&mut self) -> &mut Channels<DownChannel> {
        &mut self.down_channels
    }
}

/// List of RTT channels.
pub struct Channels<T: RttChannel>(BTreeMap<usize, T>);

impl<T: RttChannel> Channels<T> {
    /// Returns the number of channels on the list.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` if the list is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns a reference to the channel corresponding to the number.
    pub fn get(&mut self, number: usize) -> Option<&T> {
        self.0.get(&number)
    }

    /// Removes the channel corresponding to the number from the list and returns it.
    pub fn take(&mut self, number: usize) -> Option<T> {
        self.0.remove(&number)
    }

    /// Gets and iterator over the channels on the list, sorted by number.
    pub fn iter(&self) -> ChannelsIter<'_, T> {
        ChannelsIter(self.0.iter())
    }
}

/// An iterator over RTT channels.
///
/// This struct is created by the [`Channels::iter`] method. See its documentation for more.
pub struct ChannelsIter<'a, T: RttChannel>(std::collections::btree_map::Iter<'a, usize, T>);

impl<'a, T: RttChannel> Iterator for ChannelsIter<'a, T> {
    type Item = (usize, &'a T);

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|(k, v)| (*k, v))
    }
}
