//! Any entity's motion for native server mods (docs/native-mods.md, "Entities", API 60):
//! read the entities of the level in place by id or in a box, and set one's motion
//! (`Entity.setDeltaMovement` with `hurtMarked`: a player's client is told), its fall
//! (`Entity.resetFallDistance`) or its yaw (`Entity.setYRot`). Players, mobs, item entities,
//! minecarts, boats and armor stands; on a server only, for the level in place, which each
//! names by its `dimension` as `native_set_block` does.
use super::Game;
use horizon_core::{
    geometry::{Aabb, Vec3},
    position::BlockPos,
};
use horizon_protocol::packets::play::{self, Clientbound as C};

/// An entity as a native mod reads it.
#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NativeEntity {
    /// The entity id, as the other helpers and hooks take it.
    pub(crate) id: u32,
    /// `minecraft:player`, `minecraft:zombie`, `minecraft:item`, `minecraft:minecart`...
    pub(crate) kind: String,
    /// Its feet.
    pub(crate) pos: [f64; 3],
    /// Blocks a tick. A player's is the server's copy: their client moves them. A rider's is 0.
    pub(crate) velocity: [f64; 3],
    /// Degrees; 0 faces south.
    pub(crate) yaw: f32,
    pub(crate) on_ground: bool,
    /// `Entity.isShiftKeyDown`: a player holding sneak. API 64.
    pub(crate) sneaking: bool,
    /// `Abilities.flying`: a player flying in creative or spectator. API 64.
    pub(crate) flying: bool,
    /// The vehicle it rides.
    pub(crate) vehicle: Option<u32>,
    /// Its box.
    pub(crate) min: [f64; 3],
    pub(crate) max: [f64; 3],
}

/// Where an entity id is.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Found {
    Player(usize),
    Mob(usize),
    Item(usize),
    Cart(usize),
    Boat(usize),
    Stand(usize),
}

thread_local! {
    /// Where `native_find` last found an entity.
    static FIND_HINT: std::cell::Cell<Option<Found>> = const { std::cell::Cell::new(None) };
}

/// The blocks a box is in, less 1.0E-5 on each side (`Entity.checkInsideBlocks`).
fn cells(min: [f64; 3], max: [f64; 3]) -> impl Iterator<Item = [i32; 3]> {
    let e = 1.0e-5;
    let lo = min.map(|v| (v + e).floor() as i32);
    let hi = max.map(|v| (v - e).floor() as i32);
    (lo[0]..=hi[0]).flat_map(move |x| {
        (lo[1]..=hi[1]).flat_map(move |y| (lo[2]..=hi[2]).map(move |z| [x, y, z]))
    })
}

fn finite(v: [f64; 3]) -> bool {
    v.into_iter().all(f64::is_finite)
}

fn corners(b: Aabb) -> ([f64; 3], [f64; 3]) {
    let (lo, hi) = (b.min_position(), b.max_position());
    ([lo.x, lo.y, lo.z], [hi.x, hi.y, hi.z])
}

/// Whether the boxes touch (`AABB.intersects`).
fn touches(min: [f64; 3], max: [f64; 3], lo: [f64; 3], hi: [f64; 3]) -> bool {
    (0..3).all(|k| lo[k] < max[k] && hi[k] > min[k])
}

/// `PlayerRotation`'s relative turn from the server's yaw to `yaw`, the shortest way, so the
/// player's own mouse movement on its way to the server is kept.
fn turn(from: f32, to: f32) -> f32 {
    (to - from + 180.0).rem_euclid(360.0) - 180.0
}

// Native mods use these; nothing in Horizon does.
#[allow(dead_code)]
impl Game<'_> {
    /// Whether this `Game` is a server and the level in place is `dimension`.
    pub(super) fn native_server_level(&self, dimension: &str) -> bool {
        (self.serving.is_some() || self.options.dedicated.is_some()) && dimension == self.dim.id()
    }

    /// Where entity `id` is: first where the last one was found and the slot after it (a mod
    /// walking the entities in turn, as turntables do, finds each at once), else by a search.
    fn native_find(&self, id: u32) -> Option<Found> {
        if let Some(found) = FIND_HINT
            .get()
            .and_then(|hint| self.native_find_near(hint, id))
        {
            FIND_HINT.set(Some(found));
            return Some(found);
        }
        let found = self.native_find_all(id);
        FIND_HINT.set(found);
        found
    }

    /// `native_find`'s guess: the entity at `hint`, or the one after it, when it is `id`.
    fn native_find_near(&self, hint: Found, id: u32) -> Option<Found> {
        let player = |i: usize| {
            let host = self.lan.as_ref()?;
            let g = host.hub.guests.get(i)?;
            (g.entity_id as u32 == id && host.hub.here(g) && g.loaded && !g.dead)
                .then_some(Found::Player(i))
        };
        let at = |found: Found| match found {
            Found::Player(i) => player(i),
            Found::Mob(i) => self
                .mobs
                .get(i)
                .is_some_and(|m| m.id == id && !m.removed)
                .then_some(found),
            Found::Item(i) => self
                .dropped
                .get(i)
                .is_some_and(|e| e.id == id)
                .then_some(found),
            Found::Cart(i) => self
                .carts
                .get(i)
                .is_some_and(|c| c.id == id)
                .then_some(found),
            Found::Boat(i) => self
                .boats
                .get(i)
                .is_some_and(|b| b.id == id)
                .then_some(found),
            Found::Stand(i) => self
                .armor_stands
                .get(i)
                .is_some_and(|a| a.id == id)
                .then_some(found),
        };
        let next = match hint {
            Found::Player(i) => Found::Player(i + 1),
            Found::Mob(i) => Found::Mob(i + 1),
            Found::Item(i) => Found::Item(i + 1),
            Found::Cart(i) => Found::Cart(i + 1),
            Found::Boat(i) => Found::Boat(i + 1),
            Found::Stand(i) => Found::Stand(i + 1),
        };
        // A player shares no id with another entity, so a guess that is a player stands; any
        // other guess stands only when no player has the id either, as the search would find.
        let found = at(hint).or_else(|| at(next))?;
        if !matches!(found, Found::Player(_))
            && self.lan.as_ref().is_some_and(|host| {
                host.hub
                    .guests
                    .iter()
                    .any(|g| g.entity_id as u32 == id && host.hub.here(g) && g.loaded && !g.dead)
            })
        {
            return None;
        }
        Some(found)
    }

    fn native_find_all(&self, id: u32) -> Option<Found> {
        if let Some(host) = self.lan.as_ref()
            && let Some(i) =
                host.hub.guests.iter().position(|g| {
                    g.entity_id as u32 == id && host.hub.here(g) && g.loaded && !g.dead
                })
        {
            return Some(Found::Player(i));
        }
        let mob = self.mobs.iter().position(|m| m.id == id && !m.removed);
        mob.map(Found::Mob)
            .or_else(|| {
                self.dropped
                    .iter()
                    .position(|e| e.id == id)
                    .map(Found::Item)
            })
            .or_else(|| self.carts.iter().position(|c| c.id == id).map(Found::Cart))
            .or_else(|| self.boats.iter().position(|b| b.id == id).map(Found::Boat))
            .or_else(|| {
                self.armor_stands
                    .iter()
                    .position(|a| a.id == id)
                    .map(Found::Stand)
            })
    }

    fn native_entity_at(&self, found: Found) -> Option<NativeEntity> {
        let v = |v: Vec3| [v.x, v.y, v.z];
        let entity = |id: u32, kind: &str, pos: Vec3, velocity: Vec3, yaw: f32, bounds: Aabb| {
            let (min, max) = corners(bounds);
            NativeEntity {
                id,
                kind: kind.to_owned(),
                pos: v(pos),
                velocity: v(velocity),
                yaw,
                on_ground: false,
                sneaking: false,
                flying: false,
                vehicle: None,
                min,
                max,
            }
        };
        Some(match found {
            Found::Player(i) => {
                let g = &self.lan.as_ref()?.hub.guests[i];
                let (min, max) = corners(g.bounds());
                NativeEntity {
                    id: g.entity_id as u32,
                    kind: "minecraft:player".to_owned(),
                    pos: g.position,
                    // `Entity.rideTick`: a passenger doesn't move on its own.
                    velocity: g
                        .motion
                        .as_ref()
                        .filter(|_| g.vehicle.is_none())
                        .map_or([0.0; 3], |m| v(m.velocity)),
                    yaw: g.yaw,
                    on_ground: g.on_ground,
                    sneaking: g.sneaking,
                    flying: g.flying,
                    vehicle: g.vehicle,
                    min,
                    max,
                }
            }
            Found::Mob(i) => {
                let m = &self.mobs[i];
                let kind = horizon_client::command::args::normalize(&m.kind.id);
                let mut e = entity(m.id, &kind, m.position, m.velocity, m.yaw, m.bounds());
                e.on_ground = m.on_ground;
                e.vehicle = m.extra.mount;
                e
            }
            Found::Item(i) => {
                let d = &self.dropped[i];
                let mut e = entity(
                    d.id,
                    "minecraft:item",
                    d.position,
                    d.velocity,
                    0.0,
                    d.bounds(),
                );
                e.on_ground = d.on_ground;
                e
            }
            Found::Cart(i) => {
                let c = &self.carts[i];
                let mut e = entity(c.id, c.kind.id(), c.position, c.velocity, c.yaw, c.bounds());
                e.on_ground = c.on_ground();
                e
            }
            Found::Boat(i) => {
                let b = &self.boats[i];
                let mut e = entity(
                    b.id,
                    &b.entity_type(),
                    b.position,
                    b.velocity,
                    b.yaw,
                    b.bounds(),
                );
                e.on_ground = b.on_ground;
                e
            }
            Found::Stand(i) => {
                let a = &self.armor_stands[i];
                let mut e = entity(
                    a.id,
                    "minecraft:armor_stand",
                    a.position,
                    a.velocity,
                    a.yaw,
                    a.bounds(),
                );
                e.on_ground = a.on_ground;
                e
            }
        })
    }

    /// The entity `id` in the level in place, `dimension`: a player, mob, item entity,
    /// minecart, boat or armor stand. None for another dimension or off a server. API 60.
    pub(super) fn native_entity(&self, dimension: &str, id: u32) -> Option<NativeEntity> {
        if !self.native_server_level(dimension) {
            return None;
        }
        self.native_entity_at(self.native_find(id)?)
    }

    /// The entities of the level in place, `dimension`, whose box touches `min`..`max`
    /// (`Level.getEntities(null, box)`): players, mobs, item entities, minecarts, boats and
    /// armor stands, in that order. API 60.
    pub(super) fn native_entities_in(
        &self,
        dimension: &str,
        min: [f64; 3],
        max: [f64; 3],
    ) -> Vec<NativeEntity> {
        if !self.native_server_level(dimension) || !finite(min) || !finite(max) {
            return Vec::new();
        }
        self.native_all_entities(false)
            .into_iter()
            .filter(|e| touches(min, max, e.min, e.max))
            .collect()
    }

    /// Every entity of the level in place the helpers reach; without spectators when
    /// `affected` (`Entity.isAffectedByBlocks`: a spectator has no physics).
    fn native_all_entities(&self, affected: bool) -> Vec<NativeEntity> {
        let players = self.lan.as_ref().map_or(0, |h| h.hub.guests.len());
        let all = (0..players)
            .map(Found::Player)
            .chain((0..self.mobs.len()).map(Found::Mob))
            .chain((0..self.dropped.len()).map(Found::Item))
            .chain((0..self.carts.len()).map(Found::Cart))
            .chain((0..self.boats.len()).map(Found::Boat))
            .chain((0..self.armor_stands.len()).map(Found::Stand));
        all.filter(|f| match *f {
            Found::Player(i) => self.lan.as_ref().is_some_and(|h| {
                let g = &h.hub.guests[i];
                h.hub.here(g) && g.loaded && !g.dead && !(affected && g.spectator())
            }),
            Found::Mob(i) => !self.mobs[i].removed,
            _ => true,
        })
        .filter_map(|f| self.native_entity_at(f))
        .collect()
    }

    /// `Entity.checkInsideBlocks` → `BlockBehaviour.entityInside` for content blocks with
    /// `properties.entity_inside`: each entity, once a server tick, for each such block its box
    /// (less 1.0E-5 on each side) is in. Nothing is searched when no block has the property.
    pub(super) fn native_entities_inside(&mut self) {
        if !self.res.any_entity_inside()
            || (self.serving.is_none() && self.options.dedicated.is_none())
        {
            return;
        }
        let mut inside = Vec::new();
        for e in self.native_all_entities(true) {
            for [x, y, z] in cells(e.min, e.max) {
                let state = self.world.block(x, y, z);
                if self.res.facts().get(state).entity_inside {
                    inside.push((BlockPos::new(x, y, z), state, e.clone()));
                }
            }
        }
        let dimension = self.dim.id();
        for (pos, state, entity) in inside {
            self.native_entity_inside(dimension, pos, state, &entity);
        }
    }

    /// `Entity.setDeltaMovement` with `hurtMarked`: entity `id`'s motion becomes `velocity`
    /// (blocks a tick). A player's client is sent it (`ClientboundSetEntityMotionPacket`), as a
    /// knockback is, since a player moves on their own client. False for no such entity,
    /// another dimension, a velocity that isn't finite, or a player inside their own hook (whose
    /// state is swapped in). API 60.
    pub(super) fn native_set_delta_movement(
        &mut self,
        dimension: &str,
        id: u32,
        velocity: [f64; 3],
    ) -> bool {
        if !self.native_server_level(dimension) || !finite(velocity) {
            return false;
        }
        let v = Vec3::new(velocity[0], velocity[1], velocity[2]);
        match self.native_find(id) {
            Some(Found::Player(i)) => {
                let Some(host) = self.lan.as_mut() else {
                    return false;
                };
                let g = &mut host.hub.guests[i];
                if let Some(m) = g.motion.as_mut() {
                    m.velocity = v;
                }
                g.send(&C::SetEntityMotion(play::SetEntityMotion {
                    id: g.entity_id,
                    velocity,
                }));
            }
            Some(Found::Mob(i)) => {
                let m = &mut self.mobs[i];
                m.velocity = v;
                m.needs_sync = true;
            }
            Some(Found::Item(i)) => self.dropped[i].velocity = v,
            Some(Found::Cart(i)) => self.carts[i].velocity = v,
            Some(Found::Boat(i)) => self.boats[i].velocity = v,
            Some(Found::Stand(i)) => self.armor_stands[i].velocity = v,
            None => return false,
        }
        true
    }

    /// `Entity.resetFallDistance`: entity `id` has fallen nowhere yet, so landing doesn't hurt
    /// it. False for no such entity or another dimension. API 60.
    pub(super) fn native_reset_fall_distance(&mut self, dimension: &str, id: u32) -> bool {
        if !self.native_server_level(dimension) {
            return false;
        }
        match self.native_find(id) {
            Some(Found::Player(i)) => {
                let Some(host) = self.lan.as_mut() else {
                    return false;
                };
                if let Some(m) = host.hub.guests[i].motion.as_mut() {
                    m.fall_distance = 0.0;
                }
            }
            Some(Found::Mob(i)) => self.mobs[i].fall_distance = 0.0,
            Some(Found::Cart(i)) => self.carts[i].fall_distance = 0.0,
            Some(_) => {}
            None => return false,
        }
        true
    }

    /// `Entity.setYRot` (and `LivingEntity.setYHeadRot` for a mob): entity `id` faces `yaw`
    /// degrees (0 is south). A player's client is turned by the difference from the yaw the
    /// server last heard (`ClientboundPlayerRotationPacket`, relative), so their own mouse
    /// movement on its way is kept. A mob's AI may turn it again. False for no such entity,
    /// another dimension or a yaw that isn't finite. API 60.
    pub(super) fn native_set_yaw(&mut self, dimension: &str, id: u32, yaw: f32) -> bool {
        if !self.native_server_level(dimension) || !yaw.is_finite() {
            return false;
        }
        match self.native_find(id) {
            Some(Found::Player(i)) => {
                let Some(host) = self.lan.as_mut() else {
                    return false;
                };
                let g = &mut host.hub.guests[i];
                let delta = turn(g.yaw, yaw);
                g.yaw += delta;
                g.send(&C::PlayerRotation(play::PlayerRotation {
                    yaw: delta,
                    relative_yaw: true,
                    pitch: 0.0,
                    relative_pitch: true,
                }));
            }
            Some(Found::Mob(i)) => {
                let m = &mut self.mobs[i];
                m.yaw = yaw;
                m.head_yaw = yaw;
                m.needs_sync = true;
            }
            Some(Found::Cart(i)) => self.carts[i].yaw = yaw,
            Some(Found::Boat(i)) => self.boats[i].yaw = yaw,
            Some(Found::Stand(i)) => self.armor_stands[i].yaw = yaw,
            Some(Found::Item(_)) => {}
            None => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn finding_an_entity_tries_the_last_place_first_and_checks_its_id() {
        let source = include_str!("native_entities.rs");
        let code = source.split("#[cfg(test)]").next().unwrap();
        let find = &code[code.find("fn native_find(&self").unwrap()..];
        let find = &find[..find.find("\n    }\n").unwrap()];
        let near = find.find("self.native_find_near(hint, id)").unwrap();
        assert!(near < find.find("self.native_find_all(id)").unwrap());
        // Every guess is the entity with that id (a mob not removed), the one after it the
        // second guess, and a player with the id wins as in the search.
        let guess = &code[code.find("fn native_find_near(").unwrap()..];
        let guess = &guess[..guess.find("\n    }\n").unwrap()];
        for check in [
            "m.id == id && !m.removed",
            "e.id == id",
            "c.id == id",
            "b.id == id",
            "a.id == id",
        ] {
            assert!(guess.contains(check), "{check}");
        }
        assert!(guess.contains("at(hint).or_else(|| at(next))?"));
        assert!(guess.contains("if !matches!(found, Found::Player(_))"));
    }

    use super::{cells, touches, turn};

    #[test]
    fn a_box_is_inside_the_blocks_it_reaches_past_the_margin() {
        // A player standing on a block: their feet touch its top, but aren't inside it.
        let got: Vec<_> = cells([0.2, 64.0, 0.2], [0.8, 65.8, 0.8]).collect();
        assert_eq!(got, vec![[0, 64, 0], [0, 65, 0]]);
        let got: Vec<_> = cells([0.7, 64.5, 0.2], [1.3, 64.9, 0.8]).collect();
        assert_eq!(got, vec![[0, 64, 0], [1, 64, 0]]);
    }

    #[test]
    fn the_inside_hook_fires_from_the_block_tick_and_only_with_flagged_blocks() {
        let game = include_str!("../game.rs");
        let blocks = &game[game.find("fn tick_blocks(").unwrap()..];
        let blocks = &blocks[..blocks.find("\n    fn ").unwrap()];
        assert!(blocks.contains("self.native_entities_inside();"));
        assert!(game.contains("entity_inside: facts.states.iter().any(|s| s.entity_inside)"));
        let source = include_str!("native_entities.rs");
        let inside = &source[source.find("fn native_entities_inside(").unwrap()..];
        let gate = inside.find("!self.res.any_entity_inside()").unwrap();
        let search = inside.find("self.native_all_entities(true)").unwrap();
        let hook = inside
            .find("self.native_entity_inside(dimension, pos, state, &entity)")
            .unwrap();
        assert!(gate < search && search < hook);
        assert!(inside.contains("if self.res.facts().get(state).entity_inside"));
    }

    #[test]
    fn a_player_is_turned_the_short_way_round() {
        assert_eq!(turn(10.0, 30.0), 20.0);
        assert_eq!(turn(170.0, -170.0), 20.0);
        assert_eq!(turn(-170.0, 170.0), -20.0);
        assert_eq!(turn(720.0 + 5.0, 0.0), -5.0);
    }

    #[test]
    fn boxes_that_only_meet_at_a_face_do_not_touch() {
        let (lo, hi) = ([0.0; 3], [1.0; 3]);
        assert!(touches(lo, hi, [0.5; 3], [1.5; 3]));
        assert!(!touches(lo, hi, [1.0, 0.0, 0.0], [2.0, 1.0, 1.0]));
    }

    #[test]
    fn the_helpers_stay_on_a_server_and_tell_a_players_client() {
        let source = include_str!("native_entities.rs");
        let code = source.split("#[cfg(test)]").next().unwrap();
        // Every helper is gated on the level in place of a server.
        for helper in [
            "fn native_entity(",
            "fn native_entities_in(",
            "fn native_set_delta_movement(",
            "fn native_reset_fall_distance(",
            "fn native_set_yaw(",
        ] {
            let body = &code[code.find(helper).unwrap()..];
            let body = &body[..body.find("\n    }\n").unwrap()];
            assert!(
                body.contains("self.native_server_level(dimension)"),
                "{helper}"
            );
        }
        let push = &code[code.find("fn native_set_delta_movement(").unwrap()..];
        assert!(push.contains("C::SetEntityMotion(play::SetEntityMotion"));
        let yaw = &code[code.find("fn native_set_yaw(").unwrap()..];
        assert!(yaw.contains("relative_yaw: true"));
        // A player reads as sneaking and flying as the server last heard them (API 64).
        assert!(code.contains("sneaking: g.sneaking,") && code.contains("flying: g.flying,"));
    }
}
