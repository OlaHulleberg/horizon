//! Minecarts on a mod's rails for native server mods (docs/native-mods.md, "Minecarts",
//! API 62): content blocks with `properties.rail` are plain rails to the carts, along their
//! `shape`, and `native_minecart_pass` hears each cart on one every tick
//! (NeoForge's `IBlockExtension.onMinecartPass`) and may change its motion; and the carts
//! of the level in place, with their riders, read by id.
use super::{Game, minecarts::Minecart};
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
}
