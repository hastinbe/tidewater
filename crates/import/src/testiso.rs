//! Test helpers: a tiny ISO 9660 writer with Joliet, the raw-sector framings and a Nero footer, so the disc reader's
//! tests build synthetic images with our own code (no disc data anywhere). Formats: ECMA-119, the Joliet specification,
//! ECMA-130 sector layouts and the public descriptions of the NRG footer (see `disc.rs`).

/// How [`iso_image`] writes the volume.
#[derive(Clone, Copy)]
pub struct IsoOptions {
    /// Add a Joliet supplementary volume descriptor and tree (UCS-2 names as given).
    pub joliet: bool,
    /// Add the ISO 9660 tree (upper-case names with `;1`; names too long for 8.3 are mangled).
    pub iso: bool,
}

impl Default for IsoOptions {
    fn default() -> Self {
        Self {
            joliet: true,
            iso: true,
        }
    }
}

enum Node {
    Dir(Vec<(String, Node)>),
    File(usize),
}

fn insert(children: &mut Vec<(String, Node)>, parts: &[&str], file: usize) {
    if let [name] = parts {
        children.push(((*name).to_string(), Node::File(file)));
        return;
    }
    let at = match children.iter().position(|(n, _)| n == parts[0]) {
        Some(i) => i,
        None => {
            children.push((parts[0].to_string(), Node::Dir(Vec::new())));
            children.len() - 1
        }
    };
    if let Node::Dir(c) = &mut children[at].1 {
        insert(c, &parts[1..], file);
    }
}

pub(crate) fn both32(v: u32) -> [u8; 8] {
    let mut b = [0u8; 8];
    b[..4].copy_from_slice(&v.to_le_bytes());
    b[4..].copy_from_slice(&v.to_be_bytes());
    b
}

pub(crate) fn record(name: &[u8], extent: u32, size: u32, dir: bool) -> Vec<u8> {
    let mut len = 33 + name.len();
    len += len % 2;
    let mut r = vec![0u8; len];
    r[0] = len as u8;
    r[2..10].copy_from_slice(&both32(extent));
    r[10..18].copy_from_slice(&both32(size));
    r[18..25].copy_from_slice(&[126, 1, 1, 0, 0, 0, 0]);
    r[25] = if dir { 2 } else { 0 };
    r[28..30].copy_from_slice(&1u16.to_le_bytes());
    r[30..32].copy_from_slice(&1u16.to_be_bytes());
    r[32] = name.len() as u8;
    r[33..33 + name.len()].copy_from_slice(name);
    r
}

fn iso_name(name: &str, is_dir: bool, n: usize) -> Vec<u8> {
    let upper = name.to_ascii_uppercase();
    let (stem, ext) = upper.rsplit_once('.').unwrap_or((&upper, ""));
    let fits = stem.len() <= 8 && ext.len() <= 3 && !upper.contains(' ');
    let text = if is_dir {
        stem.chars().take(8).collect::<String>()
    } else if fits {
        format!("{upper};1")
    } else {
        format!("F{n:07}.DAT;1")
    };
    text.into_bytes()
}

fn ucs2(name: &str) -> Vec<u8> {
    name.encode_utf16().flat_map(u16::to_be_bytes).collect()
}

/// A directory's bytes: records packed so none crosses a sector, padded to whole sectors.
fn dir_bytes(entries: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    for e in entries {
        if out.len() % 2048 + e.len() > 2048 {
            out.resize(out.len().div_ceil(2048) * 2048, 0);
        }
        out.extend_from_slice(e);
    }
    out.resize(out.len().div_ceil(2048).max(1) * 2048, 0);
    out
}

struct Vol<'a> {
    files: &'a [(&'a str, &'a [u8])],
    file_extent: Vec<u32>,
    /// Directory contents by extent, filled as the tree is laid out.
    out: Vec<(u32, Vec<u8>)>,
    next: u32,
    joliet: bool,
    counter: usize,
}

impl Vol<'_> {
    fn entries(&mut self, children: &[(String, Node)], extents: &[(u32, u32)]) -> Vec<Vec<u8>> {
        let mut entries = vec![record(&[0], 0, 0, true), record(&[1], 0, 0, true)];
        for (i, (name, node)) in children.iter().enumerate() {
            let is_dir = matches!(node, Node::Dir(_));
            self.counter += 1;
            let nm = if self.joliet {
                ucs2(name)
            } else {
                iso_name(name, is_dir, self.counter)
            };
            let (ext, size) = extents.get(i).copied().unwrap_or((0, 0));
            entries.push(record(&nm, ext, size, is_dir));
        }
        entries
    }

    /// Lays out `children` as a directory (and its subdirectories); returns its extent and byte size.
    fn lay(&mut self, children: &[(String, Node)], parent: Option<(u32, u32)>) -> (u32, u32) {
        // The size first: records don't depend on the values of the extents.
        let measured = self.entries(children, &[]);
        let size = dir_bytes(&measured).len() as u32;
        let my_extent = self.next;
        self.next += size / 2048;
        let mut child_extents = Vec::new();
        for (_, node) in children {
            match node {
                Node::Dir(c) => child_extents.push(self.lay(c, Some((my_extent, size)))),
                Node::File(i) => child_extents.push((self.file_extent[*i], self.files[*i].1.len() as u32)),
            }
        }
        let mut entries = self.entries(children, &child_extents);
        let (pe, ps) = parent.unwrap_or((my_extent, size));
        entries[0] = record(&[0], my_extent, size, true);
        entries[1] = record(&[1], pe, ps, true);
        self.out.push((my_extent, dir_bytes(&entries)));
        (my_extent, size)
    }
}

/// A 2048-byte-sector ISO 9660 image holding `files` (paths with `/`). File data comes after the directories.
pub fn iso_image(files: &[(&str, &[u8])], opts: IsoOptions) -> Vec<u8> {
    let mut root = Vec::new();
    for (i, (path, _)) in files.iter().enumerate() {
        insert(&mut root, &path.split('/').collect::<Vec<_>>(), i);
    }
    let trees: Vec<bool> = [opts.iso.then_some(false), opts.joliet.then_some(true)]
        .into_iter()
        .flatten()
        .collect();
    let descriptors = trees.len() as u32 + 1; // plus the terminator
    let new_vol = |file_extent: Vec<u32>| Vol {
        files,
        file_extent,
        out: Vec::new(),
        next: 16 + descriptors,
        joliet: false,
        counter: 0,
    };
    // Pass 1 counts the directory sectors so file data can start after them.
    let mut probe = new_vol(vec![0; files.len()]);
    for &j in &trees {
        probe.joliet = j;
        probe.lay(&root, None);
    }
    let mut next = probe.next;
    let file_extent: Vec<u32> = files
        .iter()
        .map(|(_, d)| {
            let e = next;
            next += (d.len() as u32).div_ceil(2048);
            e
        })
        .collect();
    let mut vol = new_vol(file_extent.clone());
    let mut roots = Vec::new();
    for &j in &trees {
        vol.joliet = j;
        roots.push(vol.lay(&root, None));
    }
    let mut img = vec![0u8; next as usize * 2048];
    for (n, &j) in trees.iter().enumerate() {
        let at = (16 + n) * 2048;
        let d = &mut img[at..at + 2048];
        d[0] = if j { 2 } else { 1 };
        d[1..6].copy_from_slice(b"CD001");
        d[6] = 1;
        if j {
            d[88..91].copy_from_slice(b"%/E");
        }
        d[80..88].copy_from_slice(&both32(next));
        d[128..132].copy_from_slice(&[0, 8, 8, 0]);
        let (e, s) = roots[n];
        d[156..190].copy_from_slice(&record(&[0], e, s, true)[..34]);
    }
    let term = (16 + trees.len()) * 2048;
    img[term] = 255;
    img[term + 1..term + 6].copy_from_slice(b"CD001");
    img[term + 6] = 1;
    for (e, bytes) in &vol.out {
        let at = *e as usize * 2048;
        img[at..at + bytes.len()].copy_from_slice(bytes);
    }
    for ((_, data), e) in files.iter().zip(&file_extent) {
        let at = *e as usize * 2048;
        img[at..at + data.len()].copy_from_slice(data);
    }
    img
}

/// How raw sectors are framed around the 2048 data bytes.
#[derive(Clone, Copy)]
pub enum Framing {
    /// 2352 bytes, mode 1 (data at offset 16).
    Mode1,
    /// 2352 bytes, mode 2 form 1 (data at offset 24).
    Mode2Form1,
    /// 2448 bytes: mode 1 plus 96 bytes of sub-channel data.
    Mode1Sub,
}

/// Wraps cooked sectors into raw CD sectors (sync, header, optional subheader, data, zeroed error codes).
pub fn raw_sectors(iso: &[u8], framing: Framing) -> Vec<u8> {
    let mut out = Vec::new();
    for (n, sector) in iso.chunks(2048).enumerate() {
        let mut raw = vec![0u8; 2352];
        raw[1..11].fill(0xFF);
        raw[12..15].copy_from_slice(&[0, 2, (n % 75) as u8]);
        let (mode, at) = match framing {
            Framing::Mode1 | Framing::Mode1Sub => (1, 16),
            Framing::Mode2Form1 => (2, 24),
        };
        raw[15] = mode;
        if mode == 2 {
            raw[16..24].copy_from_slice(&[0, 0, 8, 0, 0, 0, 8, 0]);
        }
        raw[at..at + sector.len()].copy_from_slice(sector);
        out.extend_from_slice(&raw);
        if matches!(framing, Framing::Mode1Sub) {
            out.extend_from_slice(&[0x55; 96]);
        }
    }
    out
}

/// A Nero image: `junk` bytes, the cooked sectors, then a version 1 (`v2` false) or version 2 footer.
pub fn nrg_image(iso: &[u8], junk: usize, v2: bool) -> Vec<u8> {
    let mut out = vec![0xEEu8; junk];
    out.extend_from_slice(iso);
    let table = out.len() as u64;
    let mut chunk = |id: &[u8; 4], body: Vec<u8>| {
        out.extend_from_slice(id);
        out.extend_from_slice(&(body.len() as u32).to_be_bytes());
        out.extend_from_slice(&body);
    };
    chunk(b"CUES", vec![0; 16]);
    let mut e = Vec::new();
    if v2 {
        e.extend_from_slice(&(junk as u64).to_be_bytes());
        e.extend_from_slice(&(iso.len() as u64).to_be_bytes());
        e.extend_from_slice(&[0; 16]);
        chunk(b"ETN2", e);
    } else {
        e.extend_from_slice(&(junk as u32).to_be_bytes());
        e.extend_from_slice(&(iso.len() as u32).to_be_bytes());
        e.extend_from_slice(&[0; 12]);
        chunk(b"ETNF", e);
    }
    chunk(b"END!", Vec::new());
    if v2 {
        out.extend_from_slice(b"NER5");
        out.extend_from_slice(&table.to_be_bytes());
    } else {
        out.extend_from_slice(b"NERO");
        out.extend_from_slice(&(table as u32).to_be_bytes());
    }
    out
}
