//! The shulker mob's opening-lid pushes (`Shulker.onPeekAmountChange`, the
//! `MobEvent::Shift` arm of the mob event walk), in their own module so the
//! event walk stays readable. Every push is an `Entity.move(SHULKER)` through
//! the move gate (#2414); the armor stands' arm of the same lid push
//! (`lid_pushes_armor_stands`) resolves through the gate in its own shove.
//! The lid's parity bug — the push adds to position without a collision check
//! and reaches only players, mobs and armor stands — is #3085, not ours here.
use super::Game;
use horizon_client::mobs::Who;
use horizon_core::geometry::Vec3;
use horizon_world::entity_move::MoverType;
impl Game<'_> {
    /// `Entity.move(MoverType.SHULKER, delta)` for the player or the mob a
    /// shulker's opening lid pushes out of its way (#2414): the gate is armed
    /// with the shulker cause, and a cancelled or replaced push moves what the
    /// decision says.
    pub(super) fn lid_shift(&mut self, target: Who, delta: [f64; 3]) {
        let (entity, kind) = match target {
            Who::Player => (
                self.player.as_ref().map_or(0, |p| p.entity_id as u32),
                "minecraft:player",
            ),
            Who::Mob(id) => self
                .mobs
                .iter()
                .find(|m| m.id == id)
                .map_or((0, "minecraft:zombie"), |m| (m.id, m.kind.id.as_str())),
            _ => return,
        };
        let mut gate = horizon_client::move_gate::armed(
            &self.loop_context,
            entity,
            kind,
            MoverType::Shulker,
            Vec3::new(delta[0], delta[1], delta[2]),
        );
        if gate.skipped() {
            return;
        }
        let moved = gate.delta();
        match target {
            Who::Player => {
                if let Some(p) = self.player.as_mut() {
                    p.position = p.position.add_vector(moved);
                }
            }
            Who::Mob(id) => {
                if let Some(m) = self.mobs.iter_mut().find(|m| m.id == id) {
                    m.position = m.position.add_vector(moved);
                }
            }
            _ => {}
        }
    }
}
