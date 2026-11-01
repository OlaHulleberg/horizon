//! Item entities for native mods (hook points API 52): the items lying or flying in a box of the
//! level in place, for machines that act on them (Create's fans wash them, its crushing wheels
//! take them).
use super::Game;
use horizon_client::{entities::ItemEntity, items::ItemStack};
use horizon_core::geometry::Vec3;

/// Visits the items whose box touches `[min, max]`, then drops the ones left empty.
fn items_in(
    items: &mut Vec<ItemEntity>,
    min: [f64; 3],
    max: [f64; 3],
    mut f: impl FnMut(u32, &mut ItemStack, [f64; 3], &mut [f64; 3]),
) {
    let mut emptied = false;
    for item in items.iter_mut() {
        if !item.bounds().intersects_corners(
            Vec3::new(min[0], min[1], min[2]),
            Vec3::new(max[0], max[1], max[2]),
        ) {
            continue;
        }
        let p = item.position;
        let mut v = [item.velocity.x, item.velocity.y, item.velocity.z];
        f(item.id, &mut item.stack, [p.x, p.y, p.z], &mut v);
        item.velocity.x = v[0];
        item.velocity.y = v[1];
        item.velocity.z = v[2];
        emptied |= item.stack.count == 0;
    }
    if emptied {
        items.retain(|i| i.stack.count > 0);
    }
}

/// Helpers for server mods, kept as stable as the hook points.
#[allow(dead_code)]
impl Game<'_> {
    /// The item entities of the level `dimension` whose box touches the box `min`..`max`
    /// (`Level.getEntitiesOfClass(ItemEntity.class, box)`): `f` gets each one's entity id, its
    /// stack, position and motion, and may change the stack and the motion
    /// (`setItem`, `setDeltaMovement`); a stack left empty removes the entity (`discard`).
    /// Server-side, for the level in place; nothing elsewhere. API 52.
    pub(super) fn native_items_in(
        &mut self,
        dimension: &str,
        min: [f64; 3],
        max: [f64; 3],
        f: impl FnMut(u32, &mut ItemStack, [f64; 3], &mut [f64; 3]),
    ) {
        if (self.serving.is_none() && self.options.dedicated.is_none())
            || dimension != self.dim.id()
        {
            return;
        }
        items_in(&mut self.dropped, min, max, f);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: u32, at: [f64; 3], count: u32) -> ItemEntity {
        ItemEntity {
            health: 5,
            fire_ticks: 0,
            id,
            stack: ItemStack {
                item: "minecraft:ice".into(),
                count,
                damage: 0,
                extra: None,
            },
            thrower: None,
            target: None,
            position: Vec3::new(at[0], at[1], at[2]),
            previous: Vec3::new(at[0], at[1], at[2]),
            velocity: Vec3::new(0.0, 0.0, 0.0),
            on_ground: true,
            age: 0,
            pickup_delay: 0,
            bob_offset: 0.0,
            ticks: 0,
        }
    }

    #[test]
    fn items_in_a_box_are_offered_and_emptied_ones_go() {
        let mut items = vec![
            item(1, [0.5, 64.0, 0.5], 3),
            item(2, [5.5, 64.0, 0.5], 1),
            item(3, [1.1, 64.0, 0.5], 2),
        ];
        let mut seen = Vec::new();
        items_in(
            &mut items,
            [0.0, 64.0, 0.0],
            [1.0, 65.0, 1.0],
            |id, stack, at, v| {
                seen.push((id, at));
                if id == 1 {
                    stack.count = 0;
                } else {
                    v[0] = 0.25;
                }
            },
        );
        // Item 3's box (0.25 wide) reaches back to x 0.975, inside the box; item 2 is far off.
        assert_eq!(seen.iter().map(|s| s.0).collect::<Vec<_>>(), vec![1, 3]);
        assert_eq!(
            items.iter().map(|i| i.id).collect::<Vec<_>>(),
            vec![2, 3],
            "the emptied item is gone"
        );
        assert_eq!(items[1].velocity.x, 0.25, "its new motion kept");
        assert_eq!(items[0].velocity.x, 0.0);
    }
}
