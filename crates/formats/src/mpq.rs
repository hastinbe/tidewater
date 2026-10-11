//! MPQ (Mo'PaQ) version 1 archive reader.
//!
//! Format sources (public documentation):
//! - Ladislav Zezula, "The MoPaQ Archive Format", <http://www.zezula.net/en/mpq/mpqformat.html>
//! - Justin Olbrantz, "Inside MoPaQ" (2006).
//!
//! Only the container is implemented here: header, encrypted hash and block tables, sector tables and file
//! decryption. Sector decompression (PKWARE DCL, Huffman, IMA ADPCM, zlib) is delegated to the MIT/Apache-2.0
//! licensed code from the `wow-mpq` and `explode` crates (see [`crate::mpq_codec`]); `wow-mpq`'s own reader can't read
//! every file in these archives.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::OnceLock;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum MpqError {
    #[error("i/o: {0}")]
    Io(#[from] std::io::Error),
    #[error("no MPQ header found")]
    NoHeader,
    #[error("file not found: {0}")]
    NotFound(String),
    #[error("corrupt archive: {0}")]
    Corrupt(String),
    #[error("decompression failed for {name}: {msg}")]
    Decompress { name: String, msg: String },
}

pub type Result<T> = std::result::Result<T, MpqError>;

/// Block flags (Zezula, "Block table").
pub mod flags {
    pub const IMPLODE: u32 = 0x0000_0100;
    pub const COMPRESS: u32 = 0x0000_0200;
    pub const ENCRYPTED: u32 = 0x0001_0000;
    pub const FIX_KEY: u32 = 0x0002_0000;
    pub const SINGLE_UNIT: u32 = 0x0100_0000;
    pub const SECTOR_CRC: u32 = 0x0400_0000;
    pub const EXISTS: u32 = 0x8000_0000;
}

/// Hash types used by [`hash_string`].
#[derive(Clone, Copy)]
pub enum HashType {
    TableOffset = 0,
    NameA = 1,
    NameB = 2,
    FileKey = 3,
}

fn crypt_table() -> &'static [u32; 0x500] {
    static TABLE: OnceLock<[u32; 0x500]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = [0u32; 0x500];
        let mut seed: u32 = 0x0010_0001;
        for i in 0..0x100 {
            let mut idx = i;
            for _ in 0..5 {
                seed = (seed * 125 + 3) % 0x2A_AAAB;
                let hi = (seed & 0xFFFF) << 16;
                seed = (seed * 125 + 3) % 0x2A_AAAB;
                table[idx] = hi | (seed & 0xFFFF);
                idx += 0x100;
            }
        }
        table
    })
}

/// The MPQ string hash. Names are case-insensitive and `/` is treated as `\`.
pub fn hash_string(s: &str, kind: HashType) -> u32 {
    let table = crypt_table();
    let (mut seed1, mut seed2): (u32, u32) = (0x7FED_7FED, 0xEEEE_EEEE);
    for &b in s.as_bytes() {
        let ch = match b.to_ascii_uppercase() {
            b'/' => b'\\',
            c => c,
        } as u32;
        seed1 = table[(kind as usize) * 0x100 + ch as usize] ^ seed1.wrapping_add(seed2);
        seed2 = ch
            .wrapping_add(seed1)
            .wrapping_add(seed2)
            .wrapping_add(seed2 << 5)
            .wrapping_add(3);
    }
    seed1
}

/// Decrypts whole little-endian words in place; trailing bytes (len % 4) are left as they are.
pub fn decrypt(data: &mut [u8], mut key: u32) {
    let table = crypt_table();
    let mut seed: u32 = 0xEEEE_EEEE;
    for chunk in data.chunks_exact_mut(4) {
        seed = seed.wrapping_add(table[0x400 + (key & 0xFF) as usize]);
        let v = u32::from_le_bytes(chunk.try_into().expect("chunk of 4")) ^ key.wrapping_add(seed);
        key = ((!key << 21).wrapping_add(0x1111_1111)) | (key >> 11);
        seed = v.wrapping_add(seed).wrapping_add(seed << 5).wrapping_add(3);
        chunk.copy_from_slice(&v.to_le_bytes());
    }
}

#[derive(Clone, Copy, Debug)]
struct HashEntry {
    name_a: u32,
    name_b: u32,
    block: u32,
}

/// One entry of the block table: where a file's data is and how it's stored.
#[derive(Clone, Copy, Debug)]
pub struct BlockEntry {
    pub offset: u32,
    pub packed_size: u32,
    pub size: u32,
    pub flags: u32,
}

const HASH_EMPTY: u32 = 0xFFFF_FFFF;
const HASH_DELETED: u32 = 0xFFFF_FFFE;

/// An open MPQ archive. Reads go straight to the file; nothing but the tables is cached.
pub struct Archive {
    file: File,
    base: u64,
    sector_size: usize,
    hashes: Vec<HashEntry>,
    blocks: Vec<BlockEntry>,
    forced_key: Option<u32>,
}

fn words(buf: &[u8]) -> impl Iterator<Item = u32> + '_ {
    buf.chunks_exact(4)
        .map(|c| u32::from_le_bytes(c.try_into().expect("chunk of 4")))
}

impl Archive {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let mut file = File::open(path)?;
        let len = file.metadata()?.len();
        // The header sits at offset 0 or at a later 512-byte boundary (e.g. after an installer stub).
        let mut base = 0u64;
        let mut hdr = [0u8; 32];
        loop {
            if base + 32 > len {
                return Err(MpqError::NoHeader);
            }
            file.seek(SeekFrom::Start(base))?;
            file.read_exact(&mut hdr)?;
            if &hdr[0..4] == b"MPQ\x1A" {
                break;
            }
            base += 512;
        }
        let w: Vec<u32> = words(&hdr).collect();
        let shift = u16::from_le_bytes([hdr[14], hdr[15]]);
        let (hash_pos, block_pos, hash_n, block_n) = (w[4], w[5], w[6] as usize, w[7] as usize);
        if !hash_n.is_power_of_two() {
            return Err(MpqError::Corrupt(format!("hash table size {hash_n}")));
        }
        let mut read_table = |pos: u32, n: usize, key: &str| -> Result<Vec<u8>> {
            let mut buf = vec![0u8; n * 16];
            file.seek(SeekFrom::Start(base + pos as u64))?;
            file.read_exact(&mut buf)?;
            decrypt(&mut buf, hash_string(key, HashType::FileKey));
            Ok(buf)
        };
        let hbuf = read_table(hash_pos, hash_n, "(hash table)")?;
        let bbuf = read_table(block_pos, block_n, "(block table)")?;
        let hw: Vec<u32> = words(&hbuf).collect();
        let hashes = hw
            .chunks_exact(4)
            .map(|e| HashEntry {
                name_a: e[0],
                name_b: e[1],
                block: e[3],
            })
            .collect();
        let bw: Vec<u32> = words(&bbuf).collect();
        let blocks = bw
            .chunks_exact(4)
            .map(|e| BlockEntry {
                offset: e[0],
                packed_size: e[1],
                size: e[2],
                flags: e[3],
            })
            .collect();
        Ok(Self {
            file,
            base,
            sector_size: 512usize << shift,
            hashes,
            blocks,
            forced_key: None,
        })
    }

    pub fn blocks(&self) -> &[BlockEntry] {
        &self.blocks
    }

    /// The block index for `name`, if the archive has it (any locale).
    pub fn find(&self, name: &str) -> Option<usize> {
        let n = self.hashes.len();
        let start = hash_string(name, HashType::TableOffset) as usize & (n - 1);
        let (a, b) = (hash_string(name, HashType::NameA), hash_string(name, HashType::NameB));
        for i in 0..n {
            let e = self.hashes[(start + i) & (n - 1)];
            if e.block == HASH_EMPTY {
                return None;
            }
            if e.block != HASH_DELETED && e.name_a == a && e.name_b == b {
                return Some(e.block as usize);
            }
        }
        None
    }

    pub fn contains(&self, name: &str) -> bool {
        self.find(name).is_some()
    }

    /// Reads and decompresses a whole file.
    pub fn read(&mut self, name: &str) -> Result<Vec<u8>> {
        let idx = self.find(name).ok_or_else(|| MpqError::NotFound(name.into()))?;
        let base_name = name.rsplit(['\\', '/']).next().unwrap_or(name);
        self.read_block(idx, Some(base_name)).map_err(|e| match e {
            MpqError::Decompress { msg, .. } => MpqError::Decompress { name: name.into(), msg },
            e => e,
        })
    }

    /// Recovers the file key of an encrypted, multi-sector, compressed block from its sector table, whose first entry
    /// is known: it's the table's own length. Lets unnamed files be read. Method from StormLib
    /// (`DetectFileKeyBySectorSize`, MIT).
    pub fn detect_key(&mut self, idx: usize) -> Result<Option<u32>> {
        let b = self.blocks[idx];
        let packed = b.flags & (flags::IMPLODE | flags::COMPRESS) != 0;
        if b.flags & flags::ENCRYPTED == 0 || b.flags & flags::SINGLE_UNIT != 0 || !packed || b.packed_size < 8 {
            return Ok(None);
        }
        let sectors = (b.size as usize).div_ceil(self.sector_size);
        let first = ((sectors + 1) * 4) as u32;
        let mut head = [0u8; 8];
        self.file.seek(SeekFrom::Start(self.base + b.offset as u64))?;
        self.file.read_exact(&mut head)?;
        let enc0 = u32::from_le_bytes(head[0..4].try_into().expect("4 bytes"));
        let table = crypt_table();
        for low in 0..0x100u32 {
            // decrypt(): plain = enc ^ (key + seed), seed = 0xEEEEEEEE + table[0x400 + (key & 0xFF)].
            let key = (enc0 ^ first)
                .wrapping_sub(0xEEEE_EEEE)
                .wrapping_sub(table[0x400 + low as usize]);
            if key & 0xFF != low {
                continue;
            }
            let mut check = head;
            decrypt(&mut check, key);
            let second = u32::from_le_bytes(check[4..8].try_into().expect("4 bytes"));
            if u32::from_le_bytes(check[0..4].try_into().expect("4 bytes")) == first
                && second > first
                && second - first <= self.sector_size as u32 + 1
            {
                return Ok(Some(key.wrapping_add(1)));
            }
        }
        Ok(None)
    }

    /// Reads block `idx` decrypting with an explicit file key (see [`Self::detect_key`]).
    pub fn read_block_with_key(&mut self, idx: usize, key: u32) -> Result<Vec<u8>> {
        self.forced_key = Some(key);
        let r = self.read_block(idx, None);
        self.forced_key = None;
        r
    }

    /// Reads block `idx`. `base_name` (the name after the last backslash) is needed for encrypted files.
    pub fn read_block(&mut self, idx: usize, base_name: Option<&str>) -> Result<Vec<u8>> {
        let b = *self
            .blocks
            .get(idx)
            .ok_or_else(|| MpqError::Corrupt(format!("block index {idx}")))?;
        if b.flags & flags::EXISTS == 0 {
            return Err(MpqError::Corrupt(format!("block {idx} is not in use")));
        }
        let key = if let Some(k) = self.forced_key {
            Some(k)
        } else if b.flags & flags::ENCRYPTED != 0 {
            let name = base_name.ok_or_else(|| MpqError::Corrupt("encrypted file needs a name".into()))?;
            let mut k = hash_string(name, HashType::FileKey);
            if b.flags & flags::FIX_KEY != 0 {
                k = k.wrapping_add(b.offset) ^ b.size;
            }
            Some(k)
        } else {
            None
        };
        let mut raw = vec![0u8; b.packed_size as usize];
        self.file.seek(SeekFrom::Start(self.base + b.offset as u64))?;
        self.file.read_exact(&mut raw)?;
        let size = b.size as usize;
        let packed = b.flags & (flags::IMPLODE | flags::COMPRESS) != 0;

        if b.flags & flags::SINGLE_UNIT != 0 || !packed {
            // Uncompressed multi-sector files are still encrypted per sector.
            if !packed {
                if let Some(k) = key {
                    for (i, chunk) in raw.chunks_mut(self.sector_size).enumerate() {
                        decrypt(chunk, k.wrapping_add(i as u32));
                    }
                }
                raw.truncate(size);
                return Ok(raw);
            }
            if let Some(k) = key {
                decrypt(&mut raw, k);
            }
            return self.unpack(&raw, size, b.flags);
        }

        let sectors = size.div_ceil(self.sector_size);
        let table_len = (sectors + 1) * 4;
        if raw.len() < table_len {
            return Err(MpqError::Corrupt("sector table past end of file".into()));
        }
        let mut table = raw[..table_len].to_vec();
        if let Some(k) = key {
            decrypt(&mut table, k.wrapping_sub(1));
        }
        let offs: Vec<usize> = words(&table).map(|v| v as usize).collect();
        let mut out = Vec::with_capacity(size);
        for s in 0..sectors {
            let (start, end) = (offs[s], offs[s + 1]);
            if start > end || end > raw.len() {
                return Err(MpqError::Corrupt(format!("sector {s} bounds {start}..{end}")));
            }
            let mut sector = raw[start..end].to_vec();
            if let Some(k) = key {
                decrypt(&mut sector, k.wrapping_add(s as u32));
            }
            let want = (size - s * self.sector_size).min(self.sector_size);
            out.extend(self.unpack(&sector, want, b.flags)?);
        }
        Ok(out)
    }

    fn unpack(&self, data: &[u8], size: usize, block_flags: u32) -> Result<Vec<u8>> {
        if data.len() >= size {
            return Ok(data[..size].to_vec());
        }
        let (method, rest) = if block_flags & flags::IMPLODE != 0 {
            // PKWARE DCL with no method byte.
            (0x08, data)
        } else {
            let (&m, rest) = data
                .split_first()
                .ok_or_else(|| MpqError::Corrupt("empty sector".into()))?;
            (m, rest)
        };
        let mut out = crate::mpq_codec::decompress(method, rest, size).map_err(|e| MpqError::Decompress {
            name: String::new(),
            msg: e.to_string(),
        })?;
        if out.len() < size {
            return Err(MpqError::Decompress {
                name: String::new(),
                msg: format!("short sector: {} of {size}", out.len()),
            });
        }
        out.truncate(size);
        Ok(out)
    }
}

/// Opens several archives as one, later archives overriding earlier ones (patch order).
pub struct ArchiveSet {
    archives: Vec<Archive>,
    cache: HashMap<String, usize>,
}

impl ArchiveSet {
    /// `archives` in priority order, lowest first.
    pub fn new(archives: Vec<Archive>) -> Self {
        Self {
            archives,
            cache: HashMap::new(),
        }
    }

    pub fn contains(&self, name: &str) -> bool {
        self.archives.iter().any(|a| a.contains(name))
    }

    pub fn read(&mut self, name: &str) -> Result<Vec<u8>> {
        let key = name.to_ascii_lowercase();
        let idx = match self.cache.get(&key) {
            Some(&i) => i,
            None => {
                let i = (0..self.archives.len())
                    .rev()
                    .find(|&i| self.archives[i].contains(name))
                    .ok_or_else(|| MpqError::NotFound(name.into()))?;
                self.cache.insert(key, i);
                i
            }
        };
        self.archives[idx].read(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Well-known values: the keys of the hash and block tables (Zezula, "Hash table").
    #[test]
    fn table_keys() {
        assert_eq!(hash_string("(hash table)", HashType::FileKey), 0xC3AF_3770);
        assert_eq!(hash_string("(block table)", HashType::FileKey), 0xEC83_B3A3);
    }

    #[test]
    fn hash_is_case_and_slash_insensitive() {
        for t in [HashType::TableOffset, HashType::NameA, HashType::NameB] {
            assert_eq!(hash_string("rez\\unitdata.dat", t), hash_string("REZ/UnitData.dat", t));
        }
    }

    #[test]
    fn decrypt_leaves_tail_bytes() {
        let mut d = [1u8, 2, 3, 4, 5, 6];
        decrypt(&mut d, 0x1234_5678);
        assert_eq!(&d[4..], &[5, 6]);
    }
}
