//! Attacks, missiles and death. Spec: `docs/specs/combat/damage.md`, "How an attack delivers damage", "Applying
//! damage" and "Splash on impact". The attack order's chase-and-face behaviour, the range measure, missile flight
//! speed and the splash scan order are placeholders of our own until their specs land.

use tidewater_formats::script::anim;

use crate::TILE_PX;
use crate::combat::{self, missile};
use crate::terrain::Tile;
use crate::world::{Layer, Order, Unit, UnitId, Walk, World, centre, facing};

/// Placeholder flight speed in pixels per cycle (an open question in the combat spec).
pub const MISSILE_SPEED: i32 = 5;

/// The neutral player; it neither attacks nor is attacked.
const NEUTRAL: u8 = 15;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Missile {
    /// UDTA missile weapon value.
    pub kind: u8,
    pub attacker: UnitId,
    /// The unit aimed at, which a non-splash missile damages on impact wherever it is.
    pub target: Option<UnitId>,
    pub from: (i32, i32),
    pub to: (i32, i32),
    /// Current position in map pixels.
    pub pos: (i32, i32),
    pub flight: u16,
    pub elapsed: u16,
    /// Rolled damage, or the stored maximum for a splash missile.
    pub damage: u8,
    pub splash: bool,
    /// A small-cannon shot at an air unit also splashes air units.
    pub hits_air: bool,
}

/// Tiles between two footprints along the larger axis: 1 for neighbours, diagonal included.
fn gap(a: &Unit, b: &Unit) -> i32 {
    let axis = |a0: i32, aw: u8, b0: i32, bw: u8| (b0 - (a0 + aw as i32 - 1)).max(a0 - (b0 + bw as i32 - 1)).max(0);
    axis(a.tile.0, a.size.0, b.tile.0, b.size.0).max(axis(a.tile.1, a.size.1, b.tile.1, b.size.1))
}

impl World {
    /// Whether `u` may be ordered to attack `target`: an enemy it can reach with its weapon.
    pub fn can_attack(&self, u: &Unit, target: UnitId) -> bool {
        let Some(t) = self.unit(target) else { return false };
        let s = self.stats_of(u.kind);
        t.id != u.id
            && t.alive()
            && t.owner != u.owner
            && t.owner != NEUTRAL
            && s.flags & tidewater_formats::udta::flags::CAN_ATTACK != 0
            && (t.layer != Layer::Air || s.missile != missile::NONE)
    }

    pub(crate) fn attack_order(&mut self, i: usize, target: UnitId) {
        let playing = self.units[i].script.anim;
        let target_ok = self.unit(target).is_some_and(|t| t.alive());
        if !target_ok || !self.can_attack(&self.units[i], target) {
            self.go_idle(i);
            self.start_anim(i, anim::STILL);
            return;
        }
        let u = &self.units[i];
        let t = self.unit(target).expect("checked above");
        let up = self.players[u.owner as usize].upgrades;
        let reach = combat::range(u.kind, &self.stats_of(u.kind), &up).max(1) as i32;
        if gap(u, t) <= reach {
            let (dx, dy) = (t.pos.0 - u.pos.0, t.pos.1 - u.pos.1);
            let f = facing(dx, dy);
            if playing != anim::ATTACK {
                self.units[i].facing = f;
                self.start_anim(i, anim::ATTACK);
            }
            return;
        }
        let goal = t.tile;
        if self.walk(i, goal) == Walk::Arrived {
            // Nowhere closer to go: give up.
            self.go_idle(i);
            self.start_anim(i, anim::STILL);
        }
    }

    /// The strike point of the attack script: a melee hit or a missile launch.
    pub(crate) fn strike(&mut self, i: usize) {
        let Order::Attack { target } = self.units[i].order else {
            return;
        };
        let Some(ti) = self.index(target).filter(|&ti| self.units[ti].alive()) else {
            return;
        };
        let (a, t) = (&self.units[i], &self.units[ti]);
        let s = self.stats_of(a.kind);
        let (basic, pierce) = combat::attack(a.kind, &s, &self.players[a.owner as usize].upgrades, a.bloodlust > 0);
        let armour = combat::armour(&self.stats_of(t.kind), &self.players[t.owner as usize].upgrades);
        let (attacker, from, to, hits_air) = (a.id, a.pos, t.pos, t.layer == Layer::Air);
        if s.missile == missile::NONE {
            let d = combat::roll(&mut self.rng, basic, pierce, armour);
            self.damage(ti, d, attacker);
            return;
        }
        let min_flight = combat::splash(s.missile);
        let damage = match min_flight {
            Some(_) => (basic + pierce).min(255) as u8,
            None => combat::roll(&mut self.rng, basic, pierce, armour),
        };
        let mut to = to;
        if s.flags & tidewater_formats::udta::flags::TOWER == 0 {
            to.0 += (self.rng.draw() & 7) as i32 - 3;
            to.1 += (self.rng.draw() & 7) as i32 - 3;
        }
        let length = (to.0 - from.0).abs().max((to.1 - from.1).abs());
        let length = length.max(min_flight.unwrap_or(0) as i32 * TILE_PX);
        self.missiles.push(Missile {
            kind: s.missile,
            attacker,
            target: Some(target),
            from,
            to,
            pos: from,
            flight: ((length + MISSILE_SPEED - 1) / MISSILE_SPEED).max(1) as u16,
            elapsed: 0,
            damage,
            splash: min_flight.is_some(),
            hits_air: s.missile == missile::SMALL_CANNON && hits_air,
        });
    }

    /// Applies damage from `attacker` to the unit at index `ti`.
    pub(crate) fn damage(&mut self, ti: usize, d: u8, attacker: UnitId) {
        let t = &mut self.units[ti];
        if t.dying {
            return;
        }
        t.attacked_by = Some(attacker);
        if t.unholy_armor > 0 || d == 0 {
            return;
        }
        if t.hp > d as u16 {
            t.hp -= d as u16;
            return;
        }
        if let Some(a) = self.unit(attacker) {
            let owner = a.owner as usize;
            self.players[owner].kills += 1;
        }
        self.kill(ti);
    }

    fn kill(&mut self, ti: usize) {
        let unit = self.units[ti].clone();
        if let Some(from) = unit.from {
            self.mark(&unit, from, None);
        }
        self.mark(&unit, unit.tile, None);
        let u = &mut self.units[ti];
        u.hp = 0;
        u.dying = true;
        u.from = None;
        u.order = Order::Idle;
        u.pos = centre(u.tile, u.size);
        self.start_anim(ti, anim::DEATH);
    }

    /// Moves every missile one cycle and lands those that arrive.
    pub(crate) fn advance_missiles(&mut self) {
        let mut landed = Vec::new();
        for (n, m) in self.missiles.iter_mut().enumerate() {
            m.elapsed += 1;
            let k = m.elapsed.min(m.flight) as i32;
            let f = m.flight as i32;
            m.pos = (
                m.from.0 + (m.to.0 - m.from.0) * k / f,
                m.from.1 + (m.to.1 - m.from.1) * k / f,
            );
            if m.elapsed >= m.flight {
                landed.push(n);
            }
        }
        for n in landed.iter().rev() {
            let m = self.missiles.remove(*n);
            self.land(m);
        }
    }

    fn land(&mut self, m: Missile) {
        if !m.splash {
            if let Some(ti) = m.target.and_then(|t| self.index(t)) {
                self.damage(ti, m.damage, m.attacker);
            }
            return;
        }
        let tile = (m.to.0.div_euclid(TILE_PX), m.to.1.div_euclid(TILE_PX));
        self.hit_wall(tile, m.damage);
        for (dx, dy) in [(0, -1), (1, 0), (0, 1), (-1, 0)] {
            self.hit_wall((tile.0 + dx, tile.1 + dy), m.damage / 4);
        }
        let mut layers = vec![Layer::Land, Layer::Sea];
        if m.hits_air {
            layers.push(Layer::Air);
        }
        let mut hit: Vec<UnitId> = Vec::new();
        let consider = |w: &World, x: i32, y: i32, hit: &mut Vec<UnitId>| {
            for &l in &layers {
                if let Some(id) = w.occupant(l, x, y)
                    && id != m.attacker
                    && !hit.contains(&id)
                {
                    hit.push(id);
                }
            }
        };
        consider(self, tile.0, tile.1, &mut hit);
        for y in tile.1 - 3..=tile.1 + 3 {
            for x in tile.0 - 3..=tile.0 + 3 {
                consider(self, x, y, &mut hit);
            }
        }
        for id in hit {
            let Some(ti) = self.index(id) else { continue };
            let t = &self.units[ti];
            if t.dying {
                continue;
            }
            let d = (t.pos.0 - m.to.0).abs().max((t.pos.1 - m.to.1).abs());
            let armour = combat::armour(&self.stats_of(t.kind), &self.players[t.owner as usize].upgrades);
            if let Some(dmg) = combat::splash_roll(&mut self.rng, m.damage, d, armour) {
                self.damage(ti, dmg, m.attacker);
            }
        }
    }

    fn hit_wall(&mut self, tile: (i32, i32), damage: u8) {
        let (x, y) = tile;
        if !self.terrain.contains(x, y) || !self.terrain.get(x, y).has(Tile::WALL) {
            return;
        }
        let n = (y * self.terrain.width as i32 + x) as usize;
        let (c, destroyed) = combat::wall_hit(self.wall_damage[n], damage);
        self.wall_damage[n] = c;
        if destroyed {
            self.terrain.destroy_wall(x, y);
            self.wall_damage[n] = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::world::tests::{run, world};
    use crate::{Command, Layer, Order, UnitId};

    fn attack(w: &mut crate::World, player: u8, units: Vec<UnitId>, target: UnitId) {
        w.step(&[Command::Attack { player, units, target }]);
    }

    #[test]
    fn melee_fight_to_the_death() {
        let mut w = world();
        let foe = w.spawn(0, 1, (1, 0), 0);
        attack(&mut w, 0, vec![UnitId(1)], foe);
        run(&mut w, 40);
        let hp = w.unit(foe).unwrap().hp;
        assert!((60 - 2 * 8..60).contains(&hp), "one or two hits of 4 to 8, got {hp}");
        run(&mut w, 600);
        assert!(w.unit(foe).is_none(), "removed after its death animation");
        assert_eq!(w.occupant(Layer::Land, 1, 0), None);
        assert_eq!(w.players[0].kills, 1);
        assert_eq!(w.unit(UnitId(1)).unwrap().order, Order::Idle);
    }

    #[test]
    fn chases_out_of_range_targets() {
        let mut w = world();
        let foe = w.spawn(0, 1, (0, 5), 0);
        attack(&mut w, 0, vec![UnitId(1)], foe);
        run(&mut w, 100);
        let me = w.unit(UnitId(1)).unwrap();
        assert!(me.tile.1 >= 4, "walked next to the target, at {:?}", me.tile);
        assert!(w.unit(foe).unwrap().hp < 60);
    }

    #[test]
    fn arrows_fly_and_hit() {
        let mut w = world();
        let archer = w.spawn(8, 0, (0, 7), 0);
        let foe = w.spawn(0, 1, (3, 7), 0);
        attack(&mut w, 0, vec![archer], foe);
        let mut saw_missile = false;
        for _ in 0..40 {
            w.step(&[]);
            saw_missile |= !w.missiles.is_empty();
        }
        assert!(saw_missile);
        assert_eq!(w.unit(archer).unwrap().tile, (0, 7), "in range, so it stays put");
        assert!(w.unit(foe).unwrap().hp < 60);
    }

    #[test]
    fn splash_hits_friends_but_not_the_thrower() {
        let mut w = world();
        let cat = w.spawn(5, 0, (0, 7), 0);
        let foe = w.spawn(0, 1, (5, 7), 0);
        let friend = w.spawn(0, 0, (6, 7), 0);
        attack(&mut w, 0, vec![cat], foe);
        // Strike on cycle 13, then 160 px of flight at 5 px a cycle.
        run(&mut w, 13 + 32 + 2);
        assert!(w.unit(foe).is_none_or(|u| u.hp < 60));
        assert!(w.unit(friend).is_none_or(|u| u.hp < 60), "friendly fire");
        assert_eq!(w.unit(cat).unwrap().hp, 110);
    }

    #[test]
    fn same_battle_same_hash() {
        let go = || {
            let mut w = world();
            let foe = w.spawn(8, 1, (2, 2), 0);
            attack(&mut w, 0, vec![UnitId(1)], foe);
            run(&mut w, 300);
            w.state_hash()
        };
        assert_eq!(go(), go());
    }
}
