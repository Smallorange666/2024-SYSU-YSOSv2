use core::{cmp::min, f32::INFINITY, ops::Add};

use super::*;

impl Fat16Impl {
    pub fn new(inner: impl BlockDevice<Block512>) -> Self {
        let mut block = Block::default();

        inner.read_block(0, &mut block).unwrap();
        let bpb = Fat16Bpb::new(block.as_ref()).unwrap();

        trace!("Loading Fat16 Volume: {:#?}", bpb);

        let fat_start = bpb.reserved_sector_count() as usize;
        let root_dir_sector_num = bpb.root_entries_count() as usize * 32 / BLOCK_SIZE;
        let first_root_dir_sector =
            fat_start + (bpb.fat_count() as usize * bpb.sectors_per_fat() as usize);
        let first_data_sector = first_root_dir_sector + root_dir_sector_num;

        Self {
            bpb,
            inner: Box::new(inner),
            fat_start,
            first_data_sector,
            first_root_dir_sector,
        }
    }

    // calculate the first sector of the cluster
    pub fn cluster_to_first_sector(&self, cluster: &Cluster) -> usize {
        match *cluster {
            Cluster::ROOT_DIR => self.first_root_dir_sector,
            Cluster(c) => {
                // calculate the first sector of the cluster
                ((c - 2) * self.bpb.sectors_per_cluster() as u32 + self.first_data_sector as u32)
                    as usize
            }
        }
    }

    // read the FAT and get next
    pub fn get_next_cluster(&self, cluster: &Cluster) -> Result<Cluster> {
        if *cluster == Cluster::ROOT_DIR {
            Ok(Cluster::END_OF_FILE)
        } else {
            let mut block = Block::default();
            self.inner.read_block(self.fat_start, &mut block).unwrap();
            let tem = (cluster.0 * 2) as usize;
            let next = u16::from_le_bytes(block[tem..tem + 2].try_into().unwrap()) as u32;
            match Cluster(next) {
                Cluster::EMPTY => Ok(Cluster::EMPTY),
                Cluster::BAD => Err(FsError::BadCluster),
                Cluster(c) => {
                    if (0x0000_0002..0x0000_FFF6).contains(&c) {
                        Ok(Cluster(c))
                    } else if c >= 0xFFFF_FFF8 {
                        Ok(Cluster::END_OF_FILE)
                    } else {
                        Ok(Cluster::INVALID)
                    }
                }
            }
        }
    }

    // traverse all dir entries in the dir
    pub fn traverse_dir_entries<F>(&self, dir: &Directory, mut process_entry: F) -> Result<()>
    where
        F: FnMut(DirEntry) -> Result<()>,
    {
        let mut block = Block::default();
        let mut cluster = dir.cluster;
        let entry_per_block = BLOCK_SIZE / DirEntry::LEN;
        loop {
            let mut now_sector = self.cluster_to_first_sector(&cluster);
            let mut entry_num = {
                if cluster == Cluster::ROOT_DIR {
                    self.bpb.root_entries_count() as usize
                } else {
                    entry_per_block * self.bpb.sectors_per_cluster() as usize
                }
            };

            while entry_num > 0 {
                self.inner.read_block(now_sector, &mut block).unwrap();
                let mut offset = 0;
                for _ in 0..min(entry_num, entry_per_block) {
                    let entry = DirEntry::parse(&block[offset..offset + DirEntry::LEN]).unwrap();
                    if entry.is_valid() {
                        process_entry(entry)?;
                    } else {
                        return Ok(());
                    }
                    entry_num -= 1;
                    offset += DirEntry::LEN;
                }

                now_sector += 1;
            }

            cluster = self.get_next_cluster(&cluster)?;
            if cluster == Cluster::END_OF_FILE {
                return Ok(());
            }
        }
    }

    pub fn get_dir_entry_by_name(&self, dir: &Directory, name: &str) -> Result<DirEntry> {
        let parse_name = ShortFileName::parse(name)?;
        let mut result = None;
        self.traverse_dir_entries(dir, |entry| {
            if entry.filename.matches(&parse_name) {
                result = Some(entry);
            }
            Ok(())
        })?;
        result.ok_or(FsError::FileNotFound)
    }

    pub fn get_all_file_under_dir(
        &self,
        dir: &Directory,
    ) -> Result<Box<dyn Iterator<Item = Metadata> + Send>> {
        let mut entries = Vec::new();
        self.traverse_dir_entries(dir, |entry| {
            if entry.is_displayable() {
                entries.push(entry);
            }
            Ok(())
        })?;
        Ok(Box::new(
            entries.into_iter().map(|entry| Metadata::from(&entry)),
        ))
    }

    pub fn parse_path<'a>(&self, path: &'a str) -> Vec<&'a str> {
        path.split('/').filter(|s| !s.is_empty()).collect()
    }

    pub fn open_root_dir(&self) -> Directory {
        Directory {
            cluster: Cluster::ROOT_DIR,
            entry: None,
        }
    }
}

impl FileSystem for Fat16 {
    fn read_dir(&self, path: &str) -> Result<Box<dyn Iterator<Item = Metadata> + Send>> {
        // read dir and return an iterator for all entries
        let parts = self.handle.parse_path(path);
        let mut dir = self.handle.open_root_dir();
        let mut entry: DirEntry;
        for part in parts {
            entry = self.handle.get_dir_entry_by_name(&dir, part)?;
            if entry.is_directory() {
                dir = Directory::from_entry(entry);
            } else {
                return Err(FsError::NotADirectory);
            }
        }

        Ok(Box::new(self.handle.get_all_file_under_dir(&dir)?))
    }

    fn open_file(&self, path: &str) -> Result<FileHandle> {
        // open file and return a file handle
        let parts = self.handle.parse_path(path);
        let mut dir = self.handle.open_root_dir();

        for i in 0..parts.len() {
            let part = parts[i];
            let entry = self.handle.get_dir_entry_by_name(&dir, part)?;
            if i == parts.len() - 1 {
                if entry.is_directory() {
                    return Err(FsError::NotAFile);
                } else {
                    return Ok(FileHandle::new(
                        Metadata::from(&entry),
                        Box::new(File::new(self.handle.clone(), entry)),
                    ));
                }
            } else {
                dir = Directory::from_entry(entry);
            }
        }

        Err(FsError::FileNotFound)
    }

    fn metadata(&self, path: &str) -> Result<Metadata> {
        // read metadata of the file / dir
        let parts = self.handle.parse_path(path);
        let mut dir = self.handle.open_root_dir();

        for i in 0..parts.len() {
            let part = parts[i];
            let entry = self.handle.get_dir_entry_by_name(&dir, part)?;
            if i == parts.len() - 1 {
                return Ok(Metadata::from(&entry));
            } else {
                dir = Directory::from_entry(entry);
            }
        }

        Err(FsError::FileNotFound)
    }

    fn exists(&self, path: &str) -> Result<bool> {
        // check if the file / dir exists
        let parts = self.handle.parse_path(path);
        let mut dir = self.handle.open_root_dir();

        for i in 0..parts.len() {
            let part = parts[i];
            let entry = self.handle.get_dir_entry_by_name(&dir, part)?;
            if i == parts.len() - 1 {
                return Ok(true);
            } else {
                dir = Directory::from_entry(entry);
            }
        }

        Err(FsError::FileNotFound)
    }
}
