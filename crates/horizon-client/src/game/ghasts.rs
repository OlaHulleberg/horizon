//! Happy ghasts on the game side: putting a harness on (and shearing it
//! off), riding, getting off on top, and the ghastling a dried ghast block
//! hatches.
use super::Game;
use horizon_client::{
    items::ItemStack,
    mobs::{MobEvent, happy_ghast},
};
use horizon_core::geometry::{Aabb, Vec3};
use horizon_world::registry::BlockStateId;

impl Game<'_> {
    /// Client `EntityGetter.getEntityCollisions` for the player: the
    /// entities that `canBeCollidedWith` it, solid happy ghasts, living
    /// shulkers (`Shulker.canBeCollidedWith`) and boats
    /// (`AbstractBoat.canBeCollidedWith`), except the player's own vehicle.
    pub(super) fn player_entity_collisions(&self) -> Vec<Aabb> {
        self.player_entity_collisions_with(&self.native_platforms())
    }
    /// [`Self::player_entity_collisions`] with the mods' moving boxes `platforms`
    /// (`native_platforms`) among them.
    pub(super) fn player_entity_collisions_with(&self, platforms: &[(Aabb, Vec3)]) -> Vec<Aabb> {
        let Some(p) = &self.player else {
            return Vec::new();
        };
        self.mobs
            .iter()
            .filter(|m| {
                (m.species() == horizon_client::mobs::Species::HappyGhast
                    && Some(m.id) != self.riding
                    && happy_ghast::collides_with_player(m, p.position.y))
                    || (m.species() == horizon_client::mobs::Species::Shulker && m.alive())
            })
            .map(|m| m.bounds())
            .chain(
                self.boats
                    .iter()
                    .filter(|b| Some(b.id) != self.riding)
                    .map(super::boats::Boat::bounds),
            )
            .chain(platforms.iter().map(|(b, _)| *b))
            .collect()
    }
    /// The mods' moving boxes near the window's player (`native_entity_collisions`): within its
    /// box grown by its motion and a block.
    pub(super) fn native_platforms(&self) -> Vec<(Aabb, Vec3)> {
        let Some(p) = &self.player else {
            return Vec::new();
        };
        let around = p
            .bounds()
            .expand_towards(p.velocity)
            .inflate(Vec3::new(1.0, 1.0, 1.0));
        let mut out = Vec::new();
        self.native_entity_collisions(around, &mut out);
        out
    }
    /// How far the window's player is carried this tick: the motion of a mod's box it stands on
    /// (on the ground, its feet at the box's top within a sixteenth, over it), else none.
    pub(super) fn platform_carry(&self, platforms: &[(Aabb, Vec3)]) -> Option<Vec3> {
        let p = self.player.as_ref()?;
        carried_by(p.bounds(), p.ground.on_ground, platforms)
    }
    /// `HappyGhast.mobInteract`. Returns whether the click was used.
    pub(super) fn interact_happy_ghast(&mut self, index: usize, item: &str) -> bool {
        let m = &self.mobs[index];
        if !m.alive() || Some(m.id) == self.riding {
            return false;
        }
        if m.is_baby() {
            // Animal.mobInteract: food grows it up.
            return false;
        }
        let at = m.center();
        let sneaking = self.keys.contains(&winit::keyboard::KeyCode::ShiftLeft);
        let harnessed = m.extra.head.is_some();
        // Equippable.equipOnInteract: a harness goes on a bare adult.
        if item.ends_with("_harness") && item.starts_with("minecraft:") && !harnessed {
            self.mobs[index].extra.head = Some(item.into());
            self.audio
                .play("minecraft:entity.happy_ghast.equip", Some(at), 1.0, 1.0);
            self.consume_held();
            return true;
        }
        // Equippable.canBeSheared: shears take the harness off again.
        if item == "minecraft:shears"
            && harnessed
            && !sneaking
            && let Some(harness) = self.mobs[index].extra.head.take()
        {
            self.audio
                .play("minecraft:entity.happy_ghast.unequip", Some(at), 1.0, 1.0);
            self.spawn_item(ItemStack::new(&harness, 1), at, [0.0, 0.2, 0.0], 10);
            self.damage_held_tool();
            return true;
        }
        if harnessed && !sneaking {
            // doPlayerRide.
            let id = self.mobs[index].id;
            self.break_leash(id, true, false);
            let was_empty = self.mobs[index].rider.is_none();
            // addPassenger: the rider is not standing on top, so no still
            // timeout.
            self.mobs[index].extra.counter = 0;
            self.yaw = self.mobs[index].yaw;
            self.riding = Some(id);
            self.mount.reset_jump();
            if was_empty {
                let down = happy_ghast::goggles(true, &self.mobs[index]);
                let _ = self.handle_mob_events(vec![down]);
            }
            self.trigger_player("minecraft:started_riding");
            return true;
        }
        false
    }
    /// Getting off lands on top of it.
    pub(super) fn dismount_happy_ghast(&mut self, id: u32) {
        // removePassenger: held still for half a second, and with no one
        // aboard the ghast forgets its home.
        if let Some(m) = self.mobs.iter_mut().find(|m| m.id == id) {
            m.extra.counter = 10;
            m.ai.home = None;
        }
        let Some(m) = self.mobs.iter().find(|m| m.id == id) else {
            return;
        };
        let top = Vec3::new(
            m.position.x,
            m.position.y + f64::from(m.height()),
            m.position.z,
        );
        let up: MobEvent = happy_ghast::goggles(false, m);
        if let Some(p) = self.player.as_mut() {
            p.position = top;
            p.velocity = Vec3::ZERO;
        }
        let _ = self.handle_mob_events(vec![up]);
    }
    /// `DriedGhastBlock.spawnGhastling`: a ghastling facing the way the block did.
    pub(super) fn spawn_ghastling(&mut self, pos: [i32; 3], state: BlockStateId) {
        let at = [
            f64::from(pos[0]) + 0.5,
            f64::from(pos[1]),
            f64::from(pos[2]) + 0.5,
        ];
        let facing = self
            .pack
            .assets
            .registry
            .state(state)
            .and_then(|s| s.properties.get("facing").cloned());
        let yaw = match facing.as_deref() {
            Some("north") => 180.0,
            Some("east") => 270.0,
            Some("west") => 90.0,
            _ => 0.0,
        };
        if let Some(id) = self.spawn_mob("minecraft:happy_ghast", at, true)
            && let Some(m) = self.mobs.iter_mut().find(|m| m.id == id)
        {
            m.extra.age = -24000;
            m.yaw = yaw;
            m.body_yaw = yaw;
            m.head_yaw = yaw;
            let at = m.center();
            self.audio
                .play("minecraft:entity.ghastling.spawn", Some(at), 1.0, 1.0);
        }
    }
}

/// The motion of the first of `platforms` that a box `b` on the ground stood on: its bottom at
/// the platform's top within a sixteenth (a millimetre below for rounding), over it, where the
/// platform was before it moved (it may have jumped from under the box since).
fn carried_by(b: Aabb, on_ground: bool, platforms: &[(Aabb, Vec3)]) -> Option<Vec3> {
    if !on_ground {
        return None;
    }
    let (lo, hi) = (b.min_position(), b.max_position());
    platforms.iter().find_map(|(platform, motion)| {
        let before = platform.move_by(Vec3::new(-motion.x, -motion.y, -motion.z));
        let (plo, phi) = (before.min_position(), before.max_position());
        let gap = lo.y - phi.y;
        let over = lo.x < phi.x && hi.x > plo.x && lo.z < phi.z && hi.z > plo.z;
        (over && (-1.0e-3..=0.0625).contains(&gap)).then_some(*motion)
    })
}

#[cfg(test)]
mod platform_tests {
    use super::carried_by;
    use horizon_core::geometry::{Aabb, Vec3};

    #[test]
    fn a_player_standing_on_a_moving_box_is_carried_by_it() {
        // The deck was at 0 to 1 and has moved half a block along x.
        let deck = Aabb::new(Vec3::new(0.5, 0.0, 0.0), Vec3::new(1.5, 1.0, 1.0));
        let motion = Vec3::new(0.5, 0.0, 0.0);
        let platforms = [(deck, motion)];
        let at = |x: f64, y: f64| Aabb::new(Vec3::new(x, y, 0.2), Vec3::new(x + 0.6, y + 1.8, 0.8));
        assert_eq!(carried_by(at(0.2, 1.0), true, &platforms), Some(motion));
        // In the air, beside it, or too far above it: not carried.
        assert_eq!(carried_by(at(0.2, 1.0), false, &platforms), None);
        assert_eq!(carried_by(at(1.0, 1.0), true, &platforms), None);
        // Over where it was, though no longer over it (it jumped ahead): still carried.
        assert_eq!(carried_by(at(-0.2, 1.0), true, &platforms), Some(motion));
        assert_eq!(carried_by(at(0.2, 1.1), true, &platforms), None);
        // A sixteenth over it (the box moved down this tick) still carries.
        assert_eq!(carried_by(at(0.2, 1.05), true, &platforms), Some(motion));
    }
}
