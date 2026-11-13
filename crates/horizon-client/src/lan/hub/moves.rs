//! A guest's move checks: the flying kicks and the `handlePlayerPositionChange`
//! collision check whose resolved move — a Player-cause move through the move
//! gate (#2414) — decides "moved wrongly", in their own module so `hub.rs`
//! keeps its size.
use super::*;
impl Hub {
    /// `ServerGamePacketListenerImpl.tickPlayer`'s flying check: a player the
    /// last move left hovering for more than `getMaximumFlyingTicks` ticks is
    /// kicked.
    pub(super) fn tick_floating(&mut self, i: usize, events: &mut Vec<HostEvent>) {
        let kick = self.guests.update(i, |g| {
            if g.client_is_floating && !g.sleeping && g.vehicle.is_none() && !g.dead {
                g.above_ground_ticks += 1;
                if g.above_ground_ticks > maximum_flying_ticks(PLAYER_GRAVITY) {
                    events.push(HostEvent::Warn(format!(
                        "{} was kicked for floating too long!",
                        g.name
                    )));
                    return Some(g.conn);
                }
            } else {
                g.client_is_floating = false;
                g.above_ground_ticks = 0;
            }
            None
        });
        if let Some(conn) = kick {
            self.kick(conn, translatable("multiplayer.disconnect.flying", &[]));
        }
        // The vehicle the player drives, by its own gravity: a boat may hover 160 ticks, a horse 80.
        let kick = self.guests.update(i, |g| {
            if g.vehicle_floating && g.vehicle.is_some() {
                g.above_ground_vehicle_ticks += 1;
                if g.above_ground_vehicle_ticks > maximum_flying_ticks(g.vehicle_gravity) {
                    events.push(HostEvent::Warn(format!(
                        "{} was kicked for floating a vehicle too long!",
                        g.name
                    )));
                    return Some(g.conn);
                }
            } else {
                g.vehicle_floating = false;
                g.above_ground_vehicle_ticks = 0;
            }
            None
        });
        if let Some(conn) = kick {
            self.kick(conn, translatable("multiplayer.disconnect.flying", &[]));
        }
    }

    /// The collision part of handlePlayerPositionChange: whether the move
    /// counts as moved wrongly (logged), and whether it is refused (the
    /// player is sent back where they were).
    pub(super) fn collide(&self, i: usize, to: [f64; 3], ctx: &TickCtx) -> (bool, bool, bool) {
        let g = &self.guests[i];
        // noPhysics (spectators) and sleepers move freely.
        let Some(binding) = ctx.collision else {
            return (false, false, false);
        };
        if g.spectator() || g.sleeping {
            return (false, false, false);
        }
        // getBoundingBox: the box of the body's pose (swimming and crawling
        // are 0.6 tall), so a swimmer under a ceiling isn't sent back.
        let d = g.dimensions();
        let (half, height) = (f64::from(d.width) / 2.0, f64::from(d.height));
        let at = |p: [f64; 3]| {
            Aabb::new(
                Vec3::new(p[0] - half, p[1], p[2] - half),
                Vec3::new(p[0] + half, p[1] + height, p[2] + half),
            )
        };
        let (old, new) = (at(g.position), at(to));
        let mut scene = SceneCollisions {
            world: ctx.world,
            binding,
            subject: None,
        };
        // player.move, then how far from the target it ended (vanilla drops
        // the vertical part: `yDist > -0.5 || yDist < 0.5` always holds). A
        // Player-cause move through the move gate (#2414); PR 1a records the
        // movement unchanged.
        let movement = Vec3::new(
            to[0] - g.position[0],
            to[1] - g.position[1],
            to[2] - g.position[2],
        );
        let mut gate = crate::move_gate::armed(
            &self.loop_context,
            g.entity_id as u32,
            "minecraft:player",
            horizon_world::entity_move::MoverType::Player,
            movement,
        );
        // The gate decides what actually moves: the requested delta, a
        // listener's replacement, or nothing (a cancel). «Moved wrongly»
        // still measures the **requested** move against what happened, as
        // vanilla measures the packet's target - a cancelled move that went
        // nowhere is the full discrepancy.
        let requested = movement;
        let movement = gate.delta();
        let done = crate::move_gate::scene_resolve_with_context(
            &mut gate,
            &mut scene,
            old,
            movement,
            0.6,
            g.on_ground,
            // The mover's own sneaking, as CollisionContext.of(player):
            // a sneaking player descends through scaffolding.
            CollisionContext::position(g.position[1]).with_descending(g.sneaking),
        )
        .ok();
        let fail = !g.creative()
            && done.is_some_and(|done| {
                let (dx, dz) = (requested.x - done.x, requested.z - done.z);
                dx * dx + dz * dz > 0.0625
            });
        // Entity.move: verticalCollisionBelow, the player "stands on something".
        let stands = done.is_some_and(|done| movement.y < 0.0 && done.y != movement.y);
        if fail && scene.unobstructed(old).unwrap_or(true) {
            return (true, true, stands);
        }
        // isEntityCollidingWithAnythingNew: a block the old box did not
        // already touch.
        let shrink = f64::from(1.0E-5_f32);
        let shrink = Vec3::new(shrink, shrink, shrink);
        let Ok(old_shape) = VoxelShape::from_box(old.deflate(shrink)) else {
            return (fail, false, stands);
        };
        let context = CollisionContext::position(g.position[1]).with_descending(g.sneaking);
        let Ok(scan) =
            BlockCollisionScan::new(&mut scene, context, new.deflate(shrink), false, 1 << 20)
        else {
            return (fail, false, stands);
        };
        for hit in scan {
            let Ok(hit) = hit else { break };
            if !hit
                .shape
                .join_is_not_empty(&old_shape, ShapeOp::AND)
                .unwrap_or(true)
            {
                return (fail, true, stands);
            }
        }
        (fail, false, stands)
    }
}
