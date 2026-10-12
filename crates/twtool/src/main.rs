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
    /// Render every frame of a GRP sprite sheet into one PNG, using an era's palette. `name` is a path on disk or a
    /// name inside the install's archives.
    RenderGrp {
        #[arg(long)]
        install: PathBuf,
        name: String,
        out: PathBuf,
        #[arg(long, default_value = "forest")]
        era: String,
    },
    /// Render a PCX image to a PNG with its own palette, or an era's palette if it has none. `name` is a path on disk
    /// or a name inside the install's archives.
    RenderPcx {
        #[arg(long)]
        install: PathBuf,
        name: String,
        out: PathBuf,
        #[arg(long, default_value = "forest")]
        era: String,
    },
    /// Draw frame 0 of each GRP named on stdin (paths on disk or archive names) into one grid, printing `cell<TAB>name`.
    GrpSheet {
        #[arg(long)]
        install: PathBuf,
        out: PathBuf,
        #[arg(long, default_value = "forest")]
        era: String,
        #[arg(long, default_value_t = 8)]
        cols: usize,
    },
    /// Run a map headless: order player `player`'s units to `x,y` (or, with `--fight`, against the nearest enemy, or
    /// with `--harvest`, to gather),
    /// step `ticks` ticks, print where units ended up and the state hash.
    Sim {
        #[arg(long)]
        install: PathBuf,
        map: String,
        #[arg(long, default_value_t = 0)]
        player: u8,
        #[arg(long, default_value_t = 0)]
        x: i32,
        #[arg(long, default_value_t = 0)]
        y: i32,
        #[arg(long, default_value_t = 600)]
        ticks: u32,
        /// Random number generator seed.
        #[arg(long, default_value_t = 0)]
        seed: u32,
        /// Instead of moving, send each unit against the nearest enemy unit.
        #[arg(long)]
        fight: bool,
        /// Instead of moving, send each worker to the nearest gold mine (or else the nearest tree) and each tanker to
        /// the nearest of its side's oil platforms.
        #[arg(long)]
        harvest: bool,
    },
    /// Print the unit stats table (`rez\\unitdata.dat`) as TSV.
    Units {
        #[arg(long)]
        install: PathBuf,
    },
    /// Print the upgrade table as TSV: a map's `UGRD` section if it has its own data, else `rez\\upgrades.dat`.
    /// With a map that has an `ALOW` section, also print its per-player masks.
    Upgrades {
        #[arg(long)]
        install: PathBuf,
        /// A path on disk or a name inside the install's archives.
        map: Option<String>,
    },
    /// Print a string table (`*.tbl`) as `index<TAB>text`, with control bytes escaped. `name` is a path on disk or a
    /// name inside the install's archives.
    Strings {
        #[arg(long)]
        install: PathBuf,
        name: String,
    },
    /// Measure each unit type's move and attack scripts (`rez\\script.bin`) as TSV: move loop and distance, first
    /// strike and attack loop, each at normal speed, hasted and slowed.
    Scripts {
        #[arg(long)]
        install: PathBuf,
    },
    /// Fell trees in a field of one forest shape (forest table row `row`) using each tileset's change tables, and
    /// print the forest table row of every tile after each felling (`..` for cleared ground).
    Fell {
        #[arg(long)]
        install: PathBuf,
        #[arg(long, default_value_t = 24)]
        row: u16,
        /// Tiles to fell in turn, as `x,y` on a 9 x 9 field.
        #[arg(default_values_t = ["4,4".to_string(), "4,3".to_string()])]
        at: Vec<String>,
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
        Cmd::RenderPcx {
            install,
            name,
            out,
            era,
        } => {
            let mut inst = Install::open(&install)?;
            let img = tidewater_formats::pcx::Pcx::parse(&read_file_or_entry(&mut inst, &name)?)?;
            let pal = match img.palette {
                Some(p) => p,
                None => inst.tileset(parse_era(&era)?)?.palette,
            };
            println!("{}x{}", img.width, img.height);
            write_png(&out, img.width as usize, img.height as usize, &img.pixels, &pal, false)?;
        }
        Cmd::GrpSheet {
            install,
            out,
            era,
            cols,
        } => grp_sheet(&install, &out, &era, cols)?,
        Cmd::Sim {
            install,
            map,
            player,
            x,
            y,
            ticks,
            seed,
            fight,
            harvest,
        } => {
            let mode = match (fight, harvest) {
                (true, _) => SimMode::Fight,
                (_, true) => SimMode::Harvest,
                _ => SimMode::Move,
            };
            sim(&install, &map, player, (x, y), ticks, seed, mode)?
        }
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
        Cmd::Scripts { install } => scripts(&install)?,
        Cmd::Fell { install, row, at } => fell(&install, row, &at)?,
        Cmd::Upgrades { install, map } => upgrades(&install, map.as_deref())?,
        Cmd::Strings { install, name } => {
            let mut inst = Install::open(&install)?;
            let t = tidewater_formats::tbl::StringTable::parse(&read_file_or_entry(&mut inst, &name)?)?;
            for i in 0..t.0.len() {
                let text = t.text(i).expect("index in range");
                println!("{i}\t{}", text.escape_debug());
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

/// Reads `name` from disk if it is an existing file, otherwise from the installation's archives.
fn read_file_or_entry(inst: &mut Install, name: &str) -> Result<Vec<u8>> {
    if std::path::Path::new(name).is_file() {
        Ok(std::fs::read(name)?)
    } else {
        Ok(inst.read(name)?)
    }
}

fn upgrades(install: &std::path::Path, map: Option<&str>) -> Result<()> {
    use tidewater_formats::alow::Restrictions;
    use tidewater_formats::ugrd::{NAMES, UpgradeTable};
    let mut inst = Install::open(install)?;
    let pud = map
        .map(|m| Ok::<_, anyhow::Error>(Pud::parse(&read_file_or_entry(&mut inst, m)?)?))
        .transpose()?;
    let from_map = match pud.as_ref().and_then(|p| p.section(b"UGRD")) {
        Some(s) => UpgradeTable::from_ugrd(s)?,
        None => None,
    };
    let table = match from_map {
        Some(t) => {
            eprintln!("upgrades from the map's UGRD section");
            t
        }
        None => UpgradeTable::from_table(&inst.read("rez\\upgrades.dat")?)?,
    };
    println!("id\tname\ttime\tgold\twood\toil\ticon\tgroup\taffect");
    for (i, u) in table.0.iter().enumerate() {
        println!(
            "{i:#04x}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:#010x}",
            NAMES[i], u.time, u.gold, u.lumber, u.oil, u.icon, u.group, u.affect
        );
    }
    if let Some(s) = pud.as_ref().and_then(|p| p.section(b"ALOW")) {
        let r = Restrictions::parse(s)?;
        println!(
            "\nplayer\tunits\tspells_start\tspells_allowed\tspells_researching\tupgrades_allowed\tupgrades_researching"
        );
        for p in 0..r.units.len() {
            println!(
                "{p}\t{:#010x}\t{:#010x}\t{:#010x}\t{:#010x}\t{:#010x}\t{:#010x}",
                r.units[p],
                r.spells_start[p],
                r.spells_allowed[p],
                r.spells_researching[p],
                r.upgrades_allowed[p],
                r.upgrades_researching[p]
            );
        }
    }
    Ok(())
}

fn render_map(install: &std::path::Path, map: &str, out: &std::path::Path) -> Result<()> {
    let mut inst = Install::open(install)?;
    let data = read_file_or_entry(&mut inst, map)?;
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
    let grp = Grp::parse(&read_file_or_entry(&mut inst, name)?)?;
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
        match read_file_or_entry(&mut inst, name.trim()).and_then(|d| Ok(Grp::parse(&d)?)) {
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

/// What `twtool sim` orders the player's units to do.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SimMode {
    Move,
    Fight,
    Harvest,
}

fn sim(
    install: &std::path::Path,
    map: &str,
    player: u8,
    to: (i32, i32),
    ticks: u32,
    seed: u32,
    mode: SimMode,
) -> Result<()> {
    use tidewater_formats::udta::UnitTable;
    use tidewater_sim::{Command, World};
    let mut inst = Install::open(install)?;
    let data = if std::path::Path::new(map).is_file() {
        std::fs::read(map)?
    } else {
        inst.read(map)?
    };
    let pud = Pud::parse(&data)?;
    let ts = inst.tileset(pud.era)?;
    let stats = match pud.section(b"UDTA").map(UnitTable::from_udta).transpose()?.flatten() {
        Some(t) => t,
        None => UnitTable::from_table(&inst.read("rez\\unitdata.dat")?)?,
    };
    let scripts = tidewater_formats::script::Scripts::parse(inst.read("rez\\script.bin")?)?;
    let mut w = World::from_pud(&pud, &ts, stats, std::sync::Arc::new(scripts), seed)?;
    let ai = tidewater_formats::aibin::AiFile::parse(inst.read("rez\\ai.bin")?)?;
    w.start_ai(std::sync::Arc::new(ai));
    // The display maps follow the local player, which the smoke check takes to be player 0.
    w.vision.local = 0;
    let units: Vec<_> = w
        .units
        .iter()
        .filter(|u| u.owner == player && !u.building)
        .map(|u| u.id)
        .collect();
    let before: Vec<_> = units.iter().map(|&id| w.unit(id).map(|u| u.tile)).collect();
    let dist = |a: (i32, i32), b: (i32, i32)| (a.0 - b.0).abs().max((a.1 - b.1).abs());
    let orders: Vec<Command> = if mode == SimMode::Harvest {
        let (mw, mh) = (w.terrain.width as i32, w.terrain.height as i32);
        let sites: Vec<(i32, i32)> = w
            .units
            .iter()
            .filter(|u| u.building)
            .map(|u| u.tile)
            .chain((0..mh).flat_map(|y| (0..mw).map(move |x| (x, y))).filter(|&(x, y)| {
                w.terrain
                    .region(x, y)
                    .is_some_and(tidewater_sim::terrain::region::is_forest)
            }))
            .collect();
        units
            .iter()
            .filter_map(|&id| {
                let me = w.unit(id)?;
                // Mines come first in `sites`, so prefer one that is no further than the nearest tree.
                let tile = sites
                    .iter()
                    .copied()
                    .filter(|&t| w.can_harvest(me, t))
                    .min_by_key(|&t| dist(me.tile, t))?;
                Some(Command::Harvest {
                    player,
                    units: vec![id],
                    target: None,
                    tile,
                })
            })
            .collect()
    } else if mode == SimMode::Fight {
        units
            .iter()
            .filter_map(|&id| {
                let me = w.unit(id)?;
                let foe = w
                    .units
                    .iter()
                    .filter(|t| w.can_attack(me, t.id))
                    .min_by_key(|t| dist(t.tile, me.tile))?;
                Some(Command::Attack {
                    player,
                    units: vec![id],
                    target: foe.id,
                })
            })
            .collect()
    } else {
        vec![Command::Move {
            player,
            units: units.clone(),
            to,
        }]
    };
    w.step(&orders);
    let start = std::time::Instant::now();
    for _ in 1..ticks {
        w.step(&[]);
    }
    let took = start.elapsed();
    for (id, b) in units.iter().zip(before) {
        let Some(u) = w.unit(*id) else {
            println!("unit {:>3}: died", id.0);
            continue;
        };
        println!(
            "unit {:>3} kind {:#04x}: {:?} -> {:?} hp {} {:?}",
            id.0,
            u.kind,
            b.unwrap_or_default(),
            u.tile,
            u.hp,
            u.order
        );
    }
    let kills: Vec<_> = w
        .players
        .iter()
        .map(|p| p.kills)
        .enumerate()
        .filter(|(_, k)| *k > 0)
        .collect();
    println!("kills by player: {kills:?}");
    let p = &w.players[player as usize];
    println!(
        "player {player} gathered gold {} lumber {} oil {}",
        p.gathered[0], p.gathered[1], p.gathered[2]
    );
    println!("tiles reshaped: {}", w.terrain.redraw.len());
    for (p, s) in w.ai.iter().enumerate() {
        if w.players[p].controller == tidewater_sim::ai::controller::COMPUTER {
            let wanted: Vec<String> = (0x13..=0x22).map(|n| s.slots[n].to_string()).collect();
            println!(
                "ai player {p} script {:#04x}: wait {} slots 0x13..0x22 [{}]",
                w.players[p].ai_script,
                s.wait,
                wanted.join(" ")
            );
        }
    }
    println!("explored tiles (player 0): {}", w.vision.explored_tiles());
    println!("{} ticks in {took:?}, state hash {:016x}", w.tick, w.state_hash());
    Ok(())
}

/// One run of a unit's animation: (cycles between the first two events, pixels moved between them, cycle of the first).
fn measure(
    scripts: &tidewater_formats::script::Scripts,
    kind: u8,
    anim: u8,
    haste: i16,
    strike: bool,
) -> Option<String> {
    use tidewater_sim::anim::{Modifiers, Script};
    let pc = scripts.unit(kind, anim)?;
    let mut s = Script::default();
    s.start(anim, pc);
    let (mut events, mut moved) = (vec![], 0);
    for cycle in 1..=2000 {
        let fx = s.update(scripts.bytes(), 2, Modifiers { haste, carrying: false });
        if events.len() == 1 {
            moved += fx.dx;
        }
        if (strike && fx.strikes > 0) || (!strike && fx.step_done) {
            events.push(cycle);
            if events.len() == 2 {
                return Some(if strike {
                    format!("{}/{}", events[0], events[1] - events[0])
                } else {
                    format!("{}/{}px", events[1] - events[0], moved)
                });
            }
        }
    }
    Some("-".into())
}

fn scripts(install: &std::path::Path) -> Result<()> {
    use tidewater_formats::script::{Scripts, anim};
    let mut inst = Install::open(install)?;
    let s = Scripts::parse(inst.read("rez\\script.bin")?)?;
    println!("id\tmove\tmove_haste\tmove_slow\tstrike/loop\thaste\tslow");
    for kind in 0..tidewater_formats::script::UNIT_TYPES as u8 {
        let col = |a, h, st| measure(&s, kind, a, h, st).unwrap_or_default();
        println!(
            "{kind:#04x}\t{}\t{}\t{}\t{}\t{}\t{}",
            col(anim::MOVE, 0, false),
            col(anim::MOVE, 1, false),
            col(anim::MOVE, -1, false),
            col(anim::ATTACK, 0, true),
            col(anim::ATTACK, 1, true),
            col(anim::ATTACK, -1, true)
        );
    }
    Ok(())
}

fn fell(install: &std::path::Path, row: u16, at: &[String]) -> Result<()> {
    use tidewater_sim::terrain::{bits, region};
    let mut inst = Install::open(install)?;
    for era in [Era::Forest, Era::Winter, Era::Wasteland, Era::Swamp] {
        let ts = inst.tileset(era)?;
        let base = 16 + ts.changes.as_ref().map_or(0, |c| c.wall.rows as u16) - 1;
        let n = 81;
        let mut t = tidewater_sim::Terrain::new(
            9,
            9,
            vec![base + row - 1; n],
            vec![bits::OBSTACLE | bits::LAND; n],
            vec![region::FOREST; n],
            std::sync::Arc::new(ts.changes.clone().unwrap_or_default()),
        );
        println!("{}", Install::tileset_name(era));
        for xy in at {
            let (x, y) = xy
                .split_once(',')
                .ok_or_else(|| anyhow::anyhow!("expected x,y, got {xy}"))?;
            t.fell(x.trim().parse()?, y.trim().parse()?, region::LAND_AREA);
            println!("  fell {xy}");
            for y in 0..9 {
                let cells: Vec<String> = (0..9)
                    .map(|x| match t.word(x, y) & bits::OBSTACLE {
                        0 => "..".to_string(),
                        _ => format!("{:2}", t.tile(x, y).unwrap_or(0) + 1 - base),
                    })
                    .collect();
                println!("    {}", cells.join(" "));
            }
        }
    }
    Ok(())
}
