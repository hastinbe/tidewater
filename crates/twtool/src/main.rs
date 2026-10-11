//! `twtool`: inspect and convert data from a user-supplied installation.

use std::io::BufRead;
use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tidewater_formats::grp::Grp;
use tidewater_formats::install::Install;
use tidewater_formats::mpq::Archive;
use tidewater_formats::pud::{Era, Pud};
use tidewater_formats::tileset::{Palette, TILE};

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Print `block<TAB>name` for each name (one per line on stdin) that an archive contains, first spelling only.
    MpqProbe { archive: PathBuf },
    /// Extract the named files (one per line on stdin) into a directory, keeping their paths.
    MpqExtract { archive: PathBuf, out: PathBuf },
    /// Dump in-use blocks whose names aren't listed (one `block<TAB>name` per line in `known`) to `out/block_NNNN.<ext>`.
    MpqUnnamed {
        archive: PathBuf,
        known: PathBuf,
        out: PathBuf,
    },
    /// Render a map's terrain to a PNG. `map` is a path on disk or a name inside the install's archives.
    RenderMap {
        #[arg(long)]
        install: PathBuf,
        map: String,
        out: PathBuf,
    },
    /// Render every frame of a GRP sprite sheet into one PNG, using an era's palette.
    RenderGrp {
        #[arg(long)]
        install: PathBuf,
        name: String,
        out: PathBuf,
        #[arg(long, default_value = "forest")]
        era: String,
    },
    /// Draw frame 0 of each GRP named on stdin into one grid, printing `cell<TAB>name`.
    GrpSheet {
        #[arg(long)]
        install: PathBuf,
        out: PathBuf,
        #[arg(long, default_value = "forest")]
        era: String,
        #[arg(long, default_value_t = 8)]
        cols: usize,
    },
    /// Print the unit stats table (`rez\\unitdata.dat`) as TSV.
    Units {
        #[arg(long)]
        install: PathBuf,
    },
    /// Summarise an archive's block table: how many blocks, how many in use.
    MpqInfo { archive: PathBuf },
}

fn stdin_names() -> impl Iterator<Item = String> {
    std::io::stdin()
        .lock()
        .lines()
        .map_while(Result::ok)
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
}

/// A file extension guessed from content.
fn sniff(d: &[u8]) -> &'static str {
    match d {
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'A', b'V', b'E', ..] => "wav",
        [b'S', b'M', b'K', ..] => "smk",
        [b'W', b'A', b'R', b'2', ..] | [b'T', b'Y', b'P', b'E', ..] => "pud",
        [0x0A, _, 1, 8, ..] => "pcx",
        [b'M', b'Z', ..] => "exe",
        [b'B', b'M', ..] => "bmp",
        _ => "bin",
    }
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::MpqProbe { archive } => {
            let a = Archive::open(&archive)?;
            let mut seen = std::collections::HashSet::new();
            for name in stdin_names() {
                if let Some(block) = a.find(&name)
                    && seen.insert(block)
                {
                    println!("{block}\t{name}");
                }
            }
        }
        Cmd::MpqExtract { archive, out } => {
            let mut a = Archive::open(&archive)?;
            let (mut ok, mut failed) = (0, 0);
            for name in stdin_names() {
                match a.read(&name) {
                    Ok(data) => {
                        let path = out.join(name.replace('\\', "/"));
                        std::fs::create_dir_all(path.parent().expect("joined path has a parent"))?;
                        std::fs::write(path, data)?;
                        ok += 1;
                    }
                    Err(e) => {
                        eprintln!("{name}: {e}");
                        failed += 1;
                    }
                }
            }
            eprintln!("extracted {ok}, failed {failed}");
        }
        Cmd::MpqUnnamed { archive, known, out } => {
            let mut a = Archive::open(&archive)?;
            let known: std::collections::HashSet<usize> = std::fs::read_to_string(known)?
                .lines()
                .filter_map(|l| l.split('\t').next()?.parse().ok())
                .collect();
            std::fs::create_dir_all(&out)?;
            for idx in 0..a.blocks().len() {
                if known.contains(&idx) || a.blocks()[idx].flags & tidewater_formats::mpq::flags::EXISTS == 0 {
                    continue;
                }
                let data = match a.detect_key(idx)? {
                    Some(k) => a.read_block_with_key(idx, k),
                    None => a.read_block(idx, None),
                };
                match data {
                    Ok(d) => {
                        let ext = sniff(&d);
                        println!("{idx}\t{}\t{ext}", d.len());
                        std::fs::write(out.join(format!("block_{idx:04}.{ext}")), d)?;
                    }
                    Err(e) => println!("{idx}\terror\t{e}"),
                }
            }
        }
        Cmd::RenderMap { install, map, out } => render_map(&install, &map, &out)?,
        Cmd::RenderGrp {
            install,
            name,
            out,
            era,
        } => render_grp(&install, &name, &out, &era)?,
        Cmd::GrpSheet {
            install,
            out,
            era,
            cols,
        } => grp_sheet(&install, &out, &era, cols)?,
        Cmd::Units { install } => {
            let mut inst = Install::open(&install)?;
            let t = tidewater_formats::udta::UnitTable::from_table(&inst.read("rez\\unitdata.dat")?)?;
            println!("id\thp\tarmor\tdmg\tpierce\trange\tsight\tgold\twood\toil\ttime\tsize\tflags");
            for (i, u) in t.0.iter().enumerate() {
                println!(
                    "{i:#04x}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}x{}\t{:#010x}",
                    u.hit_points,
                    u.armor,
                    u.basic_damage,
                    u.piercing_damage,
                    u.attack_range,
                    u.sight,
                    u.gold,
                    u.lumber,
                    u.oil,
                    u.build_time,
                    u.size.0,
                    u.size.1,
                    u.flags
                );
            }
        }
        Cmd::MpqInfo { archive } => {
            let a = Archive::open(&archive)?;
            let used = a
                .blocks()
                .iter()
                .filter(|b| b.flags & tidewater_formats::mpq::flags::EXISTS != 0)
                .count();
            println!("blocks={} in_use={used}", a.blocks().len());
        }
    }
    Ok(())
}

fn write_png(path: &std::path::Path, w: usize, h: usize, idx: &[u8], pal: &Palette, transparent0: bool) -> Result<()> {
    let mut rgba = Vec::with_capacity(w * h * 4);
    for &i in idx {
        let c = pal.0[i as usize];
        rgba.extend([c[0], c[1], c[2], if transparent0 && i == 0 { 0 } else { 255 }]);
    }
    let mut enc = png::Encoder::new(
        std::io::BufWriter::new(std::fs::File::create(path)?),
        w as u32,
        h as u32,
    );
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()?.write_image_data(&rgba)?;
    Ok(())
}

fn parse_era(s: &str) -> Result<Era> {
    Ok(match s.to_ascii_lowercase().as_str() {
        "forest" => Era::Forest,
        "winter" => Era::Winter,
        "wasteland" => Era::Wasteland,
        "swamp" => Era::Swamp,
        _ => anyhow::bail!("unknown era {s}"),
    })
}

fn render_map(install: &std::path::Path, map: &str, out: &std::path::Path) -> Result<()> {
    let mut inst = Install::open(install)?;
    let data = if std::path::Path::new(map).is_file() {
        std::fs::read(map)?
    } else {
        inst.read(map)?
    };
    let pud = Pud::parse(&data)?;
    let ts = inst.tileset(pud.era)?;
    let (w, h) = (pud.width as usize, pud.height as usize);
    let stride = w * TILE;
    let (px, missing) = ts.draw_map(&pud.tiles, w, h);
    println!(
        "{}x{} {:?} tiles, {} units, {missing} undrawable tiles, description {:?}",
        w,
        h,
        pud.era,
        pud.units.len(),
        pud.description()
    );
    write_png(out, stride, h * TILE, &px, &ts.palette, false)
}

fn render_grp(install: &std::path::Path, name: &str, out: &std::path::Path, era: &str) -> Result<()> {
    let mut inst = Install::open(install)?;
    let grp = Grp::parse(&inst.read(name)?)?;
    let pal = inst.tileset(parse_era(era)?)?.palette;
    let (cw, ch) = (grp.width as usize, grp.height as usize);
    let cols = 10usize.min(grp.frames.len().max(1));
    let rows = grp.frames.len().div_ceil(cols);
    let stride = cols * cw;
    let mut px = vec![0u8; stride * rows * ch];
    for (i, f) in grp.frames.iter().enumerate() {
        let (ox, oy) = ((i % cols) * cw + f.x as usize, (i / cols) * ch + f.y as usize);
        for y in 0..f.height as usize {
            for x in 0..f.width as usize {
                let v = f.pixels[y * f.width as usize + x];
                if v != 0 && ox + x < stride && oy + y < rows * ch {
                    px[(oy + y) * stride + ox + x] = v;
                }
            }
        }
    }
    println!("{} frames of {}x{}", grp.frames.len(), cw, ch);
    write_png(out, stride, rows * ch, &px, &pal, true)
}

fn grp_sheet(install: &std::path::Path, out: &std::path::Path, era: &str, cols: usize) -> Result<()> {
    let mut inst = Install::open(install)?;
    let pal = inst.tileset(parse_era(era)?)?.palette;
    let mut grps = Vec::new();
    for name in std::io::stdin().lines() {
        let name = name?;
        match inst
            .read(name.trim())
            .map_err(anyhow::Error::from)
            .and_then(|d| Ok(Grp::parse(&d)?))
        {
            Ok(g) if !g.frames.is_empty() => grps.push((name, g)),
            Ok(_) => eprintln!("{name}: no frames"),
            Err(e) => eprintln!("{name}: {e}"),
        }
    }
    let cell = grps
        .iter()
        .map(|(_, g)| g.width.max(g.height) as usize)
        .max()
        .unwrap_or(1)
        .min(160);
    let rows = grps.len().div_ceil(cols);
    let stride = cols * cell;
    let mut px = vec![0u8; stride * rows * cell];
    for (i, (name, g)) in grps.iter().enumerate() {
        println!("{i}\t{name}");
        let f = &g.frames[0];
        let (ox, oy) = ((i % cols) * cell + f.x as usize, (i / cols) * cell + f.y as usize);
        for y in 0..(f.height as usize).min(cell.saturating_sub(f.y as usize)) {
            for x in 0..(f.width as usize).min(cell.saturating_sub(f.x as usize)) {
                let v = f.pixels[y * f.width as usize + x];
                if v != 0 {
                    px[(oy + y) * stride + ox + x] = v;
                }
            }
        }
    }
    write_png(out, stride, rows * cell, &px, &pal, false)
}
