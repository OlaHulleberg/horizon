//! Minecarts on a mod's rails for native server mods (docs/native-mods.md, "Minecarts",
//! API 62): content blocks with `properties.rail` are plain rails to the carts, along their
//! `shape`, and `native_minecart_pass` hears each cart on one every tick
//! (NeoForge's `IBlockExtension.onMinecartPass`) and may change its motion; and the carts
//! of the level in place, with their riders, read by id. API 65 spawns and removes them; API 68
//! moves them, fuels furnace carts and marks a cart carrying something that isn't an entity;
//! API 72 moves one as `Entity.move` does, stopped by blocks.
use super::{
    Game,
    minecarts::{CartKind, Minecart},
};
use horizon_core::{geometry::Vec3, position::BlockPos};
use horizon_world::registry::BlockStateId;

/// A minecart as a native mod reads it.
#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NativeMinecart {
    /// The entity id, as the other helpers take it; a cart keeps it while the world is open.
    pub(crate) id: u32,
    /// Kept with the cart in the save, for data a mod keeps about it across sessions.
    pub(crate) uuid: u128,
    /// `minecraft:minecart`, `minecraft:chest_minecart`...
    pub(crate) kind: &'static str,
    pub(crate) pos: [f64; 3],
    /// Blocks a tick. In `native_minecart_pass`, the motion the cart goes on with.
    pub(crate) velocity: [f64; 3],
    /// Degrees.
    pub(crate) yaw: f32,
    /// The players and mobs riding it, by entity id. Inside `native_minecart_pass` of a cart a
    /// player drives (it ticks on that player's turn) that player isn't listed: `ridden` is
    /// still true.
    pub(crate) riders: Vec<u32>,
    pub(crate) ridden: bool,
    /// A furnace cart's fuel ticks and push (`MinecartFurnace.fuel`, `push`; blocks a tick, its
    /// length the push's speed); 0 and none for other kinds. API 68.
    pub(crate) fuel: i32,
    pub(crate) push: [f64; 3],
    /// Set by `native_set_minecart_carrying`. API 68.
    pub(crate) carrying: bool,
}

// Native mods use these; nothing in Horizon does.
#[allow(dead_code)]
impl Game<'_> {
    /// Who rides vehicle `id`: the server's players in the level, then its mobs.
    pub(super) fn native_riders(&self, id: u32) -> Vec<u32> {
        let players = self.lan.as_ref().into_iter().flat_map(|h| {
            h.hub
                .guests
                .iter()
                .filter(move |g| h.hub.here(g) && g.vehicle == Some(id))
                .map(|g| g.entity_id as u32)
        });
        let mobs = self
            .mobs
            .iter()
            .filter(|m| !m.removed && m.extra.mount == Some(id))
            .map(|m| m.id);
        players.chain(mobs).collect()
    }

    fn native_minecart_of(&mut self, c: &Minecart) -> NativeMinecart {
        let riders = self.native_riders(c.id);
        let ridden = !riders.is_empty() || self.riding == Some(c.id);
        let uuid = horizon_client::lan::playerdata::ints_uuid(self.uuid_for(c.id));
        let v = |v: Vec3| [v.x, v.y, v.z];
        NativeMinecart {
            id: c.id,
            uuid,
            kind: c.kind.id(),
            pos: v(c.position),
            velocity: v(c.velocity),
            yaw: c.yaw,
            riders,
            ridden,
            fuel: c.fuel,
            push: v(c.push),
            carrying: c.carrying,
        }
    }

    /// Minecart `id` in the level in place, `dimension`. None for no such cart, another
    /// dimension or off a server. API 62.
    pub(super) fn native_minecart(&mut self, dimension: &str, id: u32) -> Option<NativeMinecart> {
        if !self.native_server_level(dimension) {
            return None;
        }
        let c = self.carts.iter().find(|c| c.id == id)?.clone();
        Some(self.native_minecart_of(&c))
    }

    /// The minecarts of the level in place, `dimension`; none off a server. API 62.
    pub(super) fn native_minecarts(&mut self, dimension: &str) -> Vec<NativeMinecart> {
        if !self.native_server_level(dimension) {
            return Vec::new();
        }
        let carts = self.carts.clone();
        carts.iter().map(|c| self.native_minecart_of(c)).collect()
    }

    /// `EntityType.create` and `Level.addFreshEntity`: a minecart of `kind`
    /// (`minecraft:minecart`, `minecraft:chest_minecart`...) at `pos`, facing `yaw`, moving at
    /// `velocity` (blocks a tick); its entity id. Clients see it through the tracker and it
    /// saves with its chunk. None for another kind, another dimension, off a server, an
    /// unloaded chunk or a value that isn't finite. API 65.
    pub(super) fn native_spawn_minecart(
        &mut self,
        dimension: &str,
        kind: &str,
        pos: [f64; 3],
        yaw: f32,
        velocity: [f64; 3],
    ) -> Option<u32> {
        let (x, z) = (pos[0].floor() as i32, pos[2].floor() as i32);
        if !self.native_server_level(dimension)
            || !pos.iter().chain(&velocity).all(|v| v.is_finite())
            || !yaw.is_finite()
            || !self.world.chunks.contains_key(&(x >> 4, z >> 4))
        {
            return None;
        }
        let kind = CartKind::from_id(kind)?;
        self.next_entity += 1;
        let mut cart = Minecart::new(self.next_entity, kind, Vec3::new(pos[0], pos[1], pos[2]));
        cart.yaw = yaw;
        cart.velocity = Vec3::new(velocity[0], velocity[1], velocity[2]);
        self.world.modified.insert((x >> 4, z >> 4));
        self.carts.push(cart);
        Some(self.next_entity)
    }

    /// `Entity.discard`: minecart `id` goes, dropping nothing (not even what it holds), its
    /// riders getting off first. False for no such cart, another dimension or off a server.
    /// API 65.
    pub(super) fn native_remove_minecart(&mut self, dimension: &str, id: u32) -> bool {
        if !self.native_server_level(dimension) {
            return false;
        }
        let Some(i) = self.carts.iter().position(|c| c.id == id) else {
            return false;
        };
        for rider in self.native_riders(id) {
            self.native_stop_riding(dimension, rider);
        }
        let c = self.carts.remove(i);
        let p = c.position;
        self.world
            .modified
            .insert((p.x.floor() as i32 >> 4, p.z.floor() as i32 >> 4));
        true
    }

    /// `Entity.setPos`: minecart `id` stands at `pos`, its motion kept (clients see it move
    /// through the tracker, as any move). The mod keeps it on its rail: a cart off one falls.
    /// False for no such cart, another dimension, off a server or a value that isn't finite.
    /// API 68.
    pub(super) fn native_set_minecart_position(
        &mut self,
        dimension: &str,
        id: u32,
        pos: [f64; 3],
    ) -> bool {
        if !self.native_server_level(dimension) || !pos.iter().all(|v| v.is_finite()) {
            return false;
        }
        let Some(c) = self.carts.iter_mut().find(|c| c.id == id) else {
            return false;
        };
        c.position = Vec3::new(pos[0], pos[1], pos[2]);
        let (x, z) = (pos[0].floor() as i32, pos[2].floor() as i32);
        self.world.modified.insert((x >> 4, z >> 4));
        true
    }

    /// `Entity.move(MoverType.SELF, delta)` for minecart `id`: moved by `delta` as far as blocks
    /// let it (a blocked component stops at the block), its motion kept. Where it ended up;
    /// None for no such cart, another dimension, off a server or a value that isn't finite.
    /// API 72.
    pub(super) fn native_move_minecart(
        &mut self,
        dimension: &str,
        id: u32,
        delta: [f64; 3],
    ) -> Option<[f64; 3]> {
        if !self.native_server_level(dimension) || !delta.iter().all(|v| v.is_finite()) {
            return None;
        }
        let i = self.carts.iter().position(|c| c.id == id)?;
        let mut c = self.carts.remove(i);
        self.move_cart(&mut c, Vec3::new(delta[0], delta[1], delta[2]));
        let p = c.position;
        self.carts.insert(i, c);
        self.world
            .modified
            .insert((p.x.floor() as i32 >> 4, p.z.floor() as i32 >> 4));
        Some([p.x, p.y, p.z])
    }

    /// A furnace minecart's `fuel` (ticks, `MinecartFurnace.fuel`) and `push` (its `push`,
    /// blocks a tick: the way it drives and, by its length, how hard), as a player's coal sets
    /// them. Fuel runs out as vanilla's does, and the push with it. False for no such cart,
    /// another kind, another dimension, off a server or a value that isn't finite. API 68.
    pub(super) fn native_set_furnace_fuel(
        &mut self,
        dimension: &str,
        id: u32,
        fuel: i32,
        push: [f64; 3],
    ) -> bool {
        if !self.native_server_level(dimension) || !push.iter().all(|v| v.is_finite()) {
            return false;
        }
        let Some(c) = self.carts.iter_mut().find(|c| c.id == id) else {
            return false;
        };
        if c.kind != CartKind::Furnace {
            return false;
        }
        c.fuel = fuel.max(0);
        c.push = Vec3::new(push[0], push[1], push[2]);
        true
    }

    /// Whether minecart `id` carries something of the mod's that isn't an entity (a
    /// contraption on it): it then moves as a cart with a passenger does (`isVehicle` in
    /// `OldMinecartBehavior.moveAlongTrack` and `applyNaturalSlowdown`: 0.997 of its speed kept
    /// a tick, not 0.96, and three quarters of its motion moved). Not saved: set it again when
    /// the world opens. False for no such cart, another dimension or off a server. API 68.
    pub(super) fn native_set_minecart_carrying(
        &mut self,
        dimension: &str,
        id: u32,
        carrying: bool,
    ) -> bool {
        if !self.native_server_level(dimension) {
            return false;
        }
        let Some(c) = self.carts.iter_mut().find(|c| c.id == id) else {
            return false;
        };
        c.carrying = carrying;
        true
    }

    /// `IBlockExtension.onMinecartPass` for cart `c` on the mod rail `state` at `pos`, as the
    /// cart's tick starts to follow it (`OldMinecartBehavior.moveAlongTrack`): the hook may
    /// change its motion, which the track then turns along the rail.
    pub(super) fn native_minecart_passes(
        &mut self,
        c: &mut Minecart,
        pos: [i32; 3],
        state: BlockStateId,
    ) {
        if self.serving.is_none() && self.options.dedicated.is_none() {
            return;
        }
        let mut cart = self.native_minecart_of(c);
        let dimension = self.dim.id();
        let at = BlockPos::new(pos[0], pos[1], pos[2]);
        self.native_minecart_pass(dimension, at, state, &mut cart);
        if cart.velocity.into_iter().all(f64::is_finite) {
            c.velocity = Vec3::new(cart.velocity[0], cart.velocity[1], cart.velocity[2]);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn carts_follow_flagged_rails_and_ask_the_mods() {
        let carts = include_str!("minecarts.rs");
        let rail = &carts[carts.find("fn rail(").unwrap()..];
        let rail = &rail[..rail.find("\n    }\n").unwrap()];
        // Vanilla's rails by name; any other block only with the flag, one bit read.
        assert!(rail.contains("if !vanilla && !facts.rail {"));
        assert!(rail.contains("native: (!vanilla).then_some(state)"));
        let tick = &carts[carts.find("fn tick_cart(").unwrap()..];
        let pass = tick
            .find("self.native_minecart_passes(c, pos, state)")
            .unwrap();
        let track = tick
            .find("self.move_along_track(c, pos, &rail, intent)")
            .unwrap();
        assert!(pass < track, "the mods change the motion the track follows");
        let source = include_str!("native_minecarts.rs");
        let passes = &source[source.find("fn native_minecart_passes(").unwrap()..];
        let gate = passes.find("self.serving.is_none()").unwrap();
        let hook = passes
            .find("self.native_minecart_pass(dimension, at, state, &mut cart)")
            .unwrap();
        assert!(gate < hook);
        assert!(passes.contains("if cart.velocity.into_iter().all(f64::is_finite)"));
    }

    #[test]
    fn mods_spawn_and_remove_carts_on_a_server_only() {
        let source = include_str!("native_minecarts.rs");
        let code = source.split("#[cfg(test)]").next().unwrap();
        for helper in ["fn native_spawn_minecart(", "fn native_remove_minecart("] {
            let body = &code[code.find(helper).unwrap()..];
            let body = &body[..body.find("\n    }\n").unwrap()];
            assert!(
                body.contains("self.native_server_level(dimension)"),
                "{helper}"
            );
            // Saved with its chunk: the chunk is marked changed.
            assert!(
                body.contains("self.world") && body.contains(".modified"),
                "{helper}"
            );
        }
        let spawn = &code[code.find("fn native_spawn_minecart(").unwrap()..];
        assert!(
            spawn.contains("CartKind::from_id(kind)?") && spawn.contains("self.carts.push(cart)")
        );
        // Riders get off before the cart goes.
        let remove = &code[code.find("fn native_remove_minecart(").unwrap()..];
        let off = remove
            .find("self.native_stop_riding(dimension, rider)")
            .unwrap();
        assert!(off < remove.find("self.carts.remove(i)").unwrap());
    }
    #[test]
    fn mods_move_carts_as_entity_move_does() {
        let source = include_str!("native_minecarts.rs");
        let code = source.split("#[cfg(test)]").next().unwrap();
        let body = &code[code.find("fn native_move_minecart(").unwrap()..];
        let body = &body[..body.find("\n    }\n").unwrap()];
        // On a server, finite: vanilla's cart move with its collisions, the chunk marked.
        let gate = body.find("self.native_server_level(dimension)").unwrap();
        let moved = body
            .find("self.move_cart(&mut c, Vec3::new(delta[0], delta[1], delta[2]));")
            .unwrap();
        assert!(gate < moved && body.contains(".modified"));
        let carts = include_str!("minecarts.rs");
        let mv = &carts[carts.find("pub(super) fn move_cart(").unwrap()..];
        assert!(mv[..mv.find("\n    }\n").unwrap()].contains("scene.resolve(c.bounds(), delta,"));
    }
    #[test]
    fn mods_move_fuel_and_load_carts_on_a_server_only() {
        let source = include_str!("native_minecarts.rs");
        let code = source.split("#[cfg(test)]").next().unwrap();
        for helper in [
            "fn native_set_minecart_position(",
            "fn native_set_furnace_fuel(",
            "fn native_set_minecart_carrying(",
        ] {
            let body = &code[code.find(helper).unwrap()..];
            let body = &body[..body.find("\n    }\n").unwrap()];
            assert!(
                body.contains("self.native_server_level(dimension)"),
                "{helper}"
            );
        }
        let fuel = &code[code.find("fn native_set_furnace_fuel(").unwrap()..];
        assert!(fuel.contains("if c.kind != CartKind::Furnace {"));
        // A carrying cart moves as a ridden one: its slowdown and its step, one bool read.
        let carts = include_str!("minecarts.rs");
        let track = &carts[carts.find("fn move_along_track(").unwrap()..];
        assert!(track.contains("let ridden = Some(c.id) == self.riding || c.carrying;"));
        assert!(track.contains("self.natural_slowdown(c, ridden)"));
        let slow = &carts[carts.find("fn natural_slowdown(").unwrap()..];
        assert!(slow.contains("let f = if ridden { 0.997 } else { 0.96 };"));
    }

    #[test]
    fn carrying_is_not_saved() {
        use super::super::minecarts::{CartKind, Minecart};
        use horizon_world::nbt::Tag;
        let tag = Tag::compound([
            ("id", Tag::String("minecraft:minecart".into())),
            (
                "Pos",
                Tag::List(
                    [0.5f64, 40.5, 0.5]
                        .map(|v| Tag::Double(v.to_bits()))
                        .to_vec(),
                ),
            ),
        ]);
        let mut cart = Minecart::from_nbt(&tag, 1, CartKind::Rideable).unwrap();
        assert!(!cart.carrying, "nothing carried until a mod says so");
        cart.carrying = true;
        let saved = cart.to_nbt([0; 4], None);
        let back = Minecart::from_nbt(&saved, 2, CartKind::Rideable).unwrap();
        assert!(!back.carrying, "the mod sets it again when the world opens");
    }
}
