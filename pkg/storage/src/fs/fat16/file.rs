//! File
//!
//! reference: <https://wiki.osdev.org/FAT#Directories_on_FAT12.2F16.2F32>

use core::cmp::min;

use super::*;

#[derive(Debug, Clone)]
pub struct File {
    /// The current offset in the file
    offset: usize,
    /// The current cluster of this file
    current_cluster: Cluster,
    /// DirEntry of this file
    entry: DirEntry,
    /// The file system handle that contains this file
    handle: Fat16Handle,
}

impl File {
    pub fn new(handle: Fat16Handle, entry: DirEntry) -> Self {
        Self {
            offset: 0,
            current_cluster: entry.cluster,
            entry,
            handle,
        }
    }

    pub fn length(&self) -> usize {
        self.entry.size as usize
    }
}

impl Read for File {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        // read file content from disk
        let bps = self.handle.bpb.bytes_per_sector() as usize;
        let spc = self.handle.bpb.sectors_per_cluster() as usize;
        let mut read_bytes = 0;
        let mut sector_offset = self.offset / bps;
        let mut byte_offset = self.offset % bps;
        let mut block = Block::default();
        loop {
            self.handle.inner.read_block(
                self.handle.cluster_to_first_sector(&self.current_cluster) + sector_offset,
                &mut block,
            )?;

            let bytes_to_read = min(
                min(buf.len() - read_bytes, bps - byte_offset),
                self.entry.size as usize - self.offset,
            );

            buf[read_bytes..read_bytes + bytes_to_read]
                .copy_from_slice(&block[byte_offset..byte_offset + bytes_to_read]);

            read_bytes += bytes_to_read;
            self.offset += bytes_to_read;
            if read_bytes == buf.len() || self.offset == self.entry.size as usize {
                break;
            } else {
                byte_offset = 0;
                sector_offset += 1;
                if sector_offset == spc {
                    sector_offset = 0;
                    self.current_cluster = self.handle.get_next_cluster(&self.current_cluster)?;
                }
            }
        }
        return Ok(read_bytes);
    }
}

// NOTE: `Seek` trait is not required for this lab
impl Seek for File {
    fn seek(&mut self, _pos: SeekFrom) -> Result<usize> {
        unimplemented!()
    }
}

// NOTE: `Write` trait is not required for this lab
impl Write for File {
    fn write(&mut self, _buf: &[u8]) -> Result<usize> {
        unimplemented!()
    }

    fn flush(&mut self) -> Result<()> {
        unimplemented!()
    }
}
