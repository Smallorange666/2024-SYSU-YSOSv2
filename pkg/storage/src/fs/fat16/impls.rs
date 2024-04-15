use core::ops::Add;

use super::*;

impl Fat16Impl {
    pub fn new(inner: impl BlockDevice<Block512>) -> Self {
        let mut block = Block::default();
        let block_size = Block512::size();

        inner.read_block(0, &mut block).unwrap();
        let bpb = Fat16Bpb::new(block.as_ref()).unwrap();

        trace!("Loading Fat16 Volume: {:#?}", bpb);

        // HINT: FirstDataSector = BPB_ResvdSecCnt + (BPB_NumFATs * FATSz) + RootDirSectors;
        let fat_start = bpb.reserved_sector_count() as usize;
        let root_dir_size = bpb.root_entries_count() as usize * 2;
        let first_root_dir_sector =
            fat_start + (bpb.fat_count() as usize * bpb.sectors_per_fat() as usize);
        let first_data_sector = first_root_dir_sector + root_dir_size;

        Self {
            bpb,
            inner: Box::new(inner),
            fat_start,
            first_data_sector,
            first_root_dir_sector,
        }
    }

    pub fn cluster_to_first_sector(&self, cluster: &Cluster) -> usize {
        match *cluster {
            Cluster::ROOT_DIR => self.first_root_dir_sector,
            Cluster(c) => {
                // FIXME: calculate the first sector of the cluster
                // HINT: FirstSectorofCluster = ((N – 2) * BPB_SecPerClus) + FirstDataSector;
                ((c - 2) * self.bpb.sectors_per_cluster() as u32 + self.first_data_sector as u32)
                    as usize
            }
        }
    }

    // FIXME: YOU NEED TO IMPLEMENT THE FILE SYSTEM OPERATIONS HERE
    //      - calculate the sectors and the clusters
    //      - read the FAT and get cluster chain
    //      - traverse the cluster chain and read the data
    //      - parse the directory entries
    //      - ...
    //      - finally, implement the FileSystem trait for Fat16 with `self.handle`
    pub fn get_next_cluster(&self, cluster: &Cluster) -> Result<Cluster> {
        let mut current = cluster;
        let mut block = Block::default();
        self.inner.read_block(self.fat_start, &mut block).unwrap();
        let tem = (cluster.0 * 2) as usize;
        let next = u16::from_le_bytes(block[tem..tem + 2].try_into().unwrap()) as u32;
        match Cluster(next) {
            Cluster::EMPTY => Ok(Cluster::EMPTY),
            Cluster::BAD => Err(FsError::BadCluster),
            Cluster(c) => {
                if c >= 0x0002 && c < 0xFFF6 {
                    Ok(Cluster(c))
                } else if c >= 0xFFF8 && c <= 0xFFFF {
                    Ok(Cluster::END_OF_FILE)
                } else {
                    Ok(Cluster::INVALID)
                }
            }
        }
    }

    pub fn get_dir_entry_by_name(&self, dir: &Directory, name: &str) -> Result<DirEntry> {
        let mut block = Block::default();
        let mut cluster = dir.cluster;
        loop {
            let first_sector = self.cluster_to_first_sector(&cluster);
            for i in 0..self.bpb.sectors_per_cluster() as usize {
                self.inner
                    .read_block(first_sector + i as usize, &mut block)
                    .unwrap();
                let mut offset = 0;
                for j in 0..BLOCK_SIZE {
                    let entry = DirEntry::parse(&block[offset..offset + DirEntry::LEN]).unwrap();
                    if entry.is_valid() && entry.filename() == name {
                        return Ok(entry);
                    }

                    offset += DirEntry::LEN;
                }
                cluster = self.get_next_cluster(&cluster)?;
            }
            if cluster == Cluster::END_OF_FILE {
                break;
            }
        }
        Err(FsError::FileNotFound)
    }

    pub fn get_all_file_under_dir(
        &self,
        dir: &Directory,
    ) -> Result<Box<dyn Iterator<Item = Metadata> + Send>> {
        let mut block = Block::default();
        let mut cluster = dir.cluster;
        let mut entries = Vec::new();
        loop {
            let first_sector = self.cluster_to_first_sector(&cluster);
            for i in 0..self.bpb.sectors_per_cluster() as usize {
                self.inner
                    .read_block(first_sector + i as usize, &mut block)
                    .unwrap();
                let mut offset = 0;
                for j in 0..BLOCK_SIZE {
                    let entry = DirEntry::parse(&block[offset..offset + DirEntry::LEN]).unwrap();
                    if entry.is_valid() {
                        entries.push(entry);
                    }
                    offset += DirEntry::LEN;
                }
                cluster = self.get_next_cluster(&cluster)?;
            }
            if cluster == Cluster::END_OF_FILE {
                return Ok(Box::new(
                    entries.into_iter().map(|entry| Metadata::from(&entry)),
                ));
            }
        }
    }

    pub fn parse_path(&self, path: &str) -> Vec<&str> {
        let mut parts = path.split('/').filter(|s| !s.is_empty());
        let mut result = Vec::new();
        while let Some(part) = parts.next() {
            result.push(part);
        }
        result
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
        // FIXME: read dir and return an iterator for all entries
        let parts = self.handle.parse_path(path);
        let mut dir = self.handle.open_root_dir();
        for i in 0..parts.len() {
            let part = parts[i];
            let entry = self.handle.get_dir_entry_by_name(&dir, part)?;
            if i == parts.len() - 1 {
                if entry.is_directory() {
                    return Ok(Box::new(self.handle.get_all_file_under_dir(&dir)?));
                } else {
                    return Err(FsError::NotADirectory);
                }
            } else {
                dir = Directory::from_entry(entry);
            }
        }

        Err(FsError::FileNotFound)
    }

    fn open_file(&self, path: &str) -> Result<FileHandle> {
        // FIXME: open file and return a file handle
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
        // FIXME: read metadata of the file / dir
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
        // FIXME: check if the file / dir exists
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
