//! Riding for native server mods (docs/native-mods.md, "Riding", API 63): a seat a mod
//! spawns and moves (an empty `minecraft:item_display`, so every client draws nothing and
//! places its riders itself), players and mobs put on it or taken off (`Entity.startRiding`,
//! `stopRiding`), who rides what, and `native_entity_mount` (NeoForge's `EntityMountEvent`)
//! when a player gets on or sneaks off a vehicle. A seat saves as vanilla's vehicles do
//! (API 67): with its chunk and the mobs riding it, or in the file of the player riding it
//! (`RootVehicle`), and `native_seat_loaded` tells the mods when it comes back.
use super::{Game, commands::OtherEntity};
use horizon_client::mobs::riders;
use horizon_core::geometry::Vec3;
use horizon_world::nbt::Tag;

/// The key that marks a saved `minecraft:item_display` as a mod's seat (vanilla ignores it).
pub(super) const SEAT: &str = "horizon:seat";

/// `Display.DATA_POS_ROT_INTERPOLATION_DURATION_ID` of a seat: clients glide it to each new
/// position over vanilla's 3 interpolation steps, and its riders with it.
pub(super) const SEAT_GLIDE: i32 = 3;

/// `Player.DEFAULT_VEHICLE_ATTACHMENT`'s height: a player's feet are this far below the seat
/// it rides (a display is 0 by 0, so its passenger attachment is its own position).
const PLAYER_ATTACHMENT: f64 = 0.6;

/// Where a player riding a seat at `at` stands (`Entity.positionRider`).
pub(super) fn seated(at: Vec3) -> Vec3 {
    Vec3::new(at.x, at.y - PLAYER_ATTACHMENT, at.z)
}

fn finite(v: [f64; 3]) -> bool {
    v.into_iter().all(f64::is_finite)
}

impl Game<'_> {
    /// Native mod seat `id`'s position, on a server; None for anything else.
    pub(super) fn native_seat(&self, id: u32) -> Option<Vec3> {
        self.others
            .iter()
            .find(|o| o.seat && o.id == id)
            .map(|o| o.position)
    }

    /// Whether a server player in this level rides seat `id` (`hasExactlyOnePlayerPassenger`).
    pub(super) fn seat_has_player(&self, id: u32) -> bool {
        let rides = |h: &super::lan::LanHost| {
            let mut guests = h.hub.guests.iter();
            guests.any(|g| h.hub.here(g) && g.vehicle == Some(id))
        };
        self.lan.as_ref().is_some_and(rides)
    }

    /// Seat number `i` of `others` with the mobs riding it under `Passengers`, marked [`SEAT`].
    pub(super) fn seat_tree_nbt(&mut self, i: usize) -> Tag {
        let o = self.others[i].clone();
        let uuid = self.uuid_for(o.id);
        let mut tag = self.other_to_nbt(&o, uuid);
        if let Tag::Compound(c) = &mut tag {
            c.insert(SEAT.into(), Tag::Byte(1));
        }
        self.put_mob_passengers(&mut tag, o.id);
        tag
    }

    /// `ServerPlayer.saveParentVehicle`'s "Entity" for a player on a seat: the seat's tree,
    /// which leaves its chunk with the player (`Entity.shouldBeSaved`). None for any other
    /// vehicle, which stays in the world.
    pub(super) fn root_vehicle_entity(&mut self, vehicle: Option<u32>) -> Option<Tag> {
        let i = self
            .others
            .iter()
            .position(|o| o.seat && Some(o.id) == vehicle)?;
        Some(self.seat_tree_nbt(i))
    }

    /// A saved seat and the mobs riding it: its id.
    pub(super) fn load_seat(&mut self, mut tag: Tag) -> Option<u32> {
        let riders = self.take_mob_passengers(&mut tag);
        if let Tag::Compound(c) = &mut tag {
            c.remove(&SEAT.into());
        }
        let id = self.load_other(tag, "minecraft:item_display")?;
        if let Some(o) = self.others.iter_mut().find(|o| o.id == id) {
            o.seat = true;
        }
        for rider in riders {
            self.load_mob(rider, Some(id));
        }
        Some(id)
    }

    /// `native_seat_loaded` for each seat that came back, its riders on it.
    pub(super) fn seats_loaded(&mut self, seats: Vec<u32>) {
        for id in seats {
            if let Some(at) = self.native_seat(id) {
                self.native_seat_loaded(self.dim.id(), id, [at.x, at.y, at.z]);
            }
        }
    }

    /// `ServerPlayer.disconnect`'s `UNLOADED_WITH_PLAYER`: the seat a leaving player rode,
    /// saved in their file, leaves the world with the mobs on it.
    pub(super) fn seat_leaves_with_player(&mut self, vehicle: Option<u32>) {
        let Some(id) = vehicle.filter(|&id| self.native_seat(id).is_some()) else {
            return;
        };
        let gone: Vec<u32> = (0..self.mobs.len())
            .filter(|&i| self.mobs[self.mob_root(i)].extra.mount == Some(id))
            .map(|i| self.mobs[i].id)
            .chain([id])
            .collect();
        self.mobs.retain(|m| !gone.contains(&m.id));
        self.others.retain(|o| o.id != id);
        for id in gone {
            self.forget_entity(id);
        }
    }

    /// `LivingEntity.stopRiding` off a seat: the player stands where the seat is
    /// (`Entity.getDismountLocationForPassenger`: its box's top, which is its position).
    pub(super) fn get_off_seat(&mut self, at: Vec3) {
        self.riding = None;
        if let Some(p) = self.player.as_mut() {
            self.previous_position = p.position;
            p.position = at;
            p.velocity = Vec3::ZERO;
            p.fall_distance = 0.0;
        }
    }
}

// Native mods use these; nothing in Horizon does.
#[allow(dead_code)]
impl Game<'_> {
    /// Spawns a seat at `pos` facing `yaw` in the level in place, `dimension`: its id, for
    /// `native_start_riding`. A player on it stands 0.6 below it (`Player`'s vehicle
    /// attachment); a mob by its own attachment. It saves with its riders (API 67:
    /// `native_seat_loaded` hears it come back). None off a server, for another dimension, a
    /// position that isn't finite, or in a chunk that isn't loaded. API 63.
    pub(super) fn native_spawn_seat(
        &mut self,
        dimension: &str,
        pos: [f64; 3],
        yaw: f32,
    ) -> Option<u32> {
        let (x, z) = (pos[0].floor() as i32, pos[2].floor() as i32);
        if !self.native_server_level(dimension)
            || !finite(pos)
            || !yaw.is_finite()
            || !self.world.chunks.contains_key(&(x >> 4, z >> 4))
        {
            return None;
        }
        self.next_entity += 1;
        let id = self.next_entity;
        self.others.push(OtherEntity {
            id,
            kind: "minecraft:item_display".to_owned(),
            position: Vec3::new(pos[0], pos[1], pos[2]),
            yaw,
            pitch: 0.0,
            seat: true,
        });
        Some(id)
    }

    /// Moves seat `id` to `pos`, facing `yaw`: its riders go with it (clients glide them
    /// over 3 ticks). A move every tick is a smooth ride. False for no such seat, another
    /// dimension or a value that isn't finite. API 63.
    pub(super) fn native_move_seat(
        &mut self,
        dimension: &str,
        id: u32,
        pos: [f64; 3],
        yaw: f32,
    ) -> bool {
        if !self.native_server_level(dimension) || !finite(pos) || !yaw.is_finite() {
            return false;
        }
        let Some(o) = self.others.iter_mut().find(|o| o.seat && o.id == id) else {
            return false;
        };
        o.position = Vec3::new(pos[0], pos[1], pos[2]);
        o.yaw = yaw;
        true
    }

    /// Takes seat `id` away, its riders getting off first. False for no such seat or
    /// another dimension. API 63.
    pub(super) fn native_remove_seat(&mut self, dimension: &str, id: u32) -> bool {
        if !self.native_server_level(dimension) || self.native_seat(id).is_none() {
            return false;
        }
        for rider in self.native_riders(id) {
            self.native_stop_riding(dimension, rider);
        }
        self.others.retain(|o| !(o.seat && o.id == id));
        true
    }

    /// `Entity.startRiding`: `rider`, a player or a mob, gets on `vehicle`. A player rides a
    /// seat, a boat or a minecart (one player each); a mob rides a seat or, by vanilla's rules
    /// (`canRide`, one passenger), a mob. Its client is told by the tracker, and it sits where
    /// the vehicle puts it from its next tick. No `native_entity_mount` fires for it. False
    /// for no such rider or vehicle, one that already rides something (call
    /// `native_stop_riding` first), another dimension, or a player inside their own hook.
    /// API 63.
    pub(super) fn native_start_riding(
        &mut self,
        dimension: &str,
        rider: u32,
        vehicle: u32,
    ) -> bool {
        if !self.native_server_level(dimension) || rider == vehicle {
            return false;
        }
        let seat = self.native_seat(vehicle);
        if let Some(i) = self.mobs.iter().position(|m| m.id == rider && !m.removed) {
            if self.mobs[i].extra.mount.is_some() {
                return false;
            }
            if seat.is_some() {
                riders::link(&mut self.mobs, rider, vehicle);
                return true;
            }
            return riders::start_riding(&mut self.mobs, rider, vehicle);
        }
        let boat = self
            .boats
            .iter()
            .find(|b| b.id == vehicle)
            .map(|b| b.position);
        let cart = self
            .carts
            .iter()
            .find(|c| c.id == vehicle)
            .map(|c| c.position);
        let single = boat.is_some() || cart.is_some();
        let Some(host) = self.lan.as_mut() else {
            return false;
        };
        let taken = single && host.hub.guests.iter().any(|g| g.vehicle == Some(vehicle));
        let Some(g) = host
            .hub
            .guests
            .iter_mut()
            .find(|g| g.entity_id as u32 == rider && g.loaded && !g.dead)
        else {
            return false;
        };
        if g.vehicle.is_some() || taken || (seat.is_none() && !single) {
            return false;
        }
        g.vehicle = Some(vehicle);
        g.vehicle_good = boat.or(cart).map(|p| [p.x, p.y, p.z]);
        g.on_ground = false;
        if let Some(at) = seat {
            let p = seated(at);
            g.previous_position = g.position;
            g.position = [p.x, p.y, p.z];
        }
        true
    }

    /// `Entity.stopRiding`: `rider` gets off what it rides, where that vehicle lets it off (a
    /// seat: where the seat is; a player is teleported there, as vanilla's `dismountTo`). No
    /// `native_entity_mount` fires for it. False for no such rider, one that rides nothing,
    /// another dimension, or a player inside their own hook. API 63.
    pub(super) fn native_stop_riding(&mut self, dimension: &str, rider: u32) -> bool {
        if !self.native_server_level(dimension) {
            return false;
        }
        if let Some(i) = self.mobs.iter().position(|m| m.id == rider && !m.removed) {
            let Some(vehicle) = self.mobs[i].extra.mount else {
                return false;
            };
            let at = self.native_seat(vehicle);
            riders::stop_riding(&mut self.mobs[i], at);
            return true;
        }
        let Some(mut host) = self.lan.take() else {
            return false;
        };
        let conn = host
            .hub
            .guests
            .iter()
            .find(|g| g.entity_id as u32 == rider && g.vehicle.is_some())
            .map(|g| g.conn);
        // As the player's own sneak does: their state is swapped in, and the client is told
        // and teleported where they land (`ServerPlayer.dismountTo`).
        let done = conn.is_some_and(|conn| {
            self.as_guest(&mut host, conn, |g| g.stop_riding())
                .is_some()
        });
        self.lan = Some(host);
        done
    }

    /// Who rides `vehicle` (`Entity.getPassengers`): the server's players, then mobs, by
    /// entity id. Empty off a server or for another dimension. API 63.
    pub(super) fn native_passengers(&self, dimension: &str, vehicle: u32) -> Vec<u32> {
        if !self.native_server_level(dimension) {
            return Vec::new();
        }
        self.native_riders(vehicle)
    }
}

#[cfg(test)]
mod tests {
    use super::{SEAT_GLIDE, seated};
    use horizon_core::geometry::Vec3;

    #[test]
    fn a_player_sits_its_vehicle_attachment_below_the_seat() {
        assert_eq!(
            seated(Vec3::new(1.5, 65.1, -2.5)),
            Vec3::new(1.5, 65.1 - 0.6, -2.5)
        );
        assert_eq!(SEAT_GLIDE, 3);
    }

    #[test]
    fn seats_save_as_vanilla_vehicles() {
        // With their chunk and their mobs, unless a player rides one: their file holds it.
        let persist = include_str!("persist.rs");
        let chunks = &persist[persist.find("fn entity_chunks(").unwrap()..];
        let skip = chunks
            .find("} else if self.seat_has_player(o.id) {")
            .unwrap();
        assert!(chunks[skip..].contains("continue;"));
        assert!(chunks.contains("self.seat_tree_nbt(i)"));
        assert!(
            chunks
                .contains("self.boats.iter().all(|b| b.id != v) && self.native_seat(v).is_none()")
        );
        let load = &persist[persist.find("fn spawn_loaded_entities(").unwrap()..];
        assert!(load.contains("seats.extend(self.load_seat(tag));"));
        assert!(load.contains("self.seats_loaded(seats);"));
        assert!(persist.contains("|| self.native_seat(id).is_some();"));
        // The single player server writes its owner's file, the seat in it.
        let game = include_str!("../game.rs");
        assert!(game.contains("if self.options.dedicated.is_some() && !owner {"));
        let lan = include_str!("lan.rs");
        assert!(lan.contains("let entity = self.root_vehicle_entity(guest.vehicle);"));
        assert!(lan.contains("self.seat_leaves_with_player(guest.vehicle);"));
        assert!(
            include_str!("dedicated.rs")
                .contains("let entity = self.root_vehicle_entity(g.vehicle);")
        );
    }

    #[test]
    fn seats_are_ridden_everywhere() {
        // The server keeps a player on it, lets it off on a sneak, and asks the mods first.
        let vehicles = include_str!("lan_vehicles.rs");
        let tick = &vehicles[vehicles.find("fn guest_vehicle_tick(").unwrap()..];
        let ask = tick
            .find("self.native_entity_mount(self.dim.id(), rider, vehicle, false, &mut canceled);")
            .unwrap();
        let sit = tick
            .find("if let Some(at) = game.riding.and_then(|id| game.native_seat(id)) {")
            .unwrap();
        assert!(ask < sit);
        assert!(tick.contains("keys.shift = !canceled;"));
        let boarded = &vehicles[vehicles.find("fn guest_boarded(").unwrap()..];
        assert!(boarded.contains(
            "self.native_entity_mount(self.dim.id(), rider, vehicle, true, &mut canceled);"
        ));
        let consume = include_str!("consume.rs");
        assert!(consume.contains("} else if let Some(at) = self.native_seat(id) {"));
        // Mobs sit on it on the server, and a client seats its player on the display.
        let passengers = include_str!("passengers.rs");
        assert!(passengers.contains("|| self.native_seat(v).is_some()"));
        let remote = include_str!("remote_vehicles.rs");
        assert!(remote.contains("} else if let Some(d) = self.text_displays.get(&(id as i32)) {"));
        let tracked = include_str!("lan_entities.rs");
        assert!(tracked.contains("e.meta.push((10, serializer::INT, Value::Int(SEAT_GLIDE)));"));
    }
}
