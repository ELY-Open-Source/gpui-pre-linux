//! Keys a forwarder sends without `wl_keyboard.modifiers`.

use xkbcommon::xkb::{self, Keycode, State};

/// Keys down, with each press's group and locks.
#[derive(Default)]
pub(super) struct HeldKeys {
    /// `None` until a mask names a key's group and locks.
    down: Vec<(Keycode, Option<[u32; 2]>)>,
}

impl HeldKeys {
    /// The compositor's mask: a fresh state, held keys pressed again.
    pub fn sent(&mut self, state: &mut State, mask: [u32; 4]) {
        let [depressed, latched, locked, group] = mask;
        *state = State::new(&state.get_keymap());
        let mut kept = Vec::new();
        for (keycode, under) in self.down.drain(..) {
            let under = match under {
                Some(under) => press(state, keycode, under, depressed).then_some(under),
                // An unseen lock key may have unlocked: the mask's.
                None if locks(state, keycode, group) => None,
                // Unseen: the first group whose press the mask holds.
                None => groups(state, group)
                    .into_iter()
                    .map(|group| [group, locked])
                    .find(|under| press(state, keycode, *under, depressed)),
            };
            if let Some(under) = under {
                kept.push((keycode, Some(under)));
            }
        }
        self.down = kept;
        // The mask wins; each key keeps what its release clears.
        state.update_mask(depressed, latched, locked, 0, 0, group);
    }

    /// Moves the state through a key, as xkb does.
    pub fn key(&mut self, state: &mut State, keycode: Keycode, pressed: bool) {
        if let Some(at) = self.down.iter().position(|(down, _)| *down == keycode) {
            self.down.remove(at);
            if pressed {
                // Its release never came.
                state.update_key(keycode, xkb::KeyDirection::Up);
            }
        }
        if !pressed {
            state.update_key(keycode, xkb::KeyDirection::Up);
            return;
        }
        let under = [
            state.serialize_layout(xkb::STATE_LAYOUT_EFFECTIVE),
            state.serialize_mods(xkb::STATE_MODS_LOCKED),
        ];
        state.update_key(keycode, xkb::KeyDirection::Down);
        self.down.push((keycode, Some(under)));
    }

    /// Keys down before focus; the next mask presses them again.
    pub fn unseen(&mut self, down: Vec<Keycode>) {
        self.down = down.into_iter().map(|keycode| (keycode, None)).collect();
    }

    /// A new keymap: the next mask names the groups.
    pub fn rekeyed(&mut self) {
        for (_, under) in &mut self.down {
            *under = None;
        }
    }

    /// Focus lost: no key is down here.
    pub fn clear(&mut self) {
        self.down.clear();
    }
}

/// Presses a key again, released if the mask lacks it.
fn press(state: &mut State, keycode: Keycode, [group, locked]: [u32; 2], mask: u32) -> bool {
    let before = state.serialize_mods(xkb::STATE_MODS_DEPRESSED);
    state.update_mask(before, 0, locked, 0, 0, group);
    state.update_key(keycode, xkb::KeyDirection::Down);
    let added = state.serialize_mods(xkb::STATE_MODS_DEPRESSED) & !before;
    let held = added & !mask == 0;
    if !held {
        // The mask says it went up.
        state.update_key(keycode, xkb::KeyDirection::Up);
    }
    held
}

/// The keymap's groups, `first` first.
fn groups(state: &State, first: u32) -> Vec<u32> {
    let all = state.get_keymap().num_layouts();
    std::iter::once(first)
        .chain((0..all).filter(|group| *group != first))
        .collect()
}

/// Whether a key's press locks something, from no locks.
fn locks(state: &State, keycode: Keycode, group: u32) -> bool {
    let mut probe = State::new(&state.get_keymap());
    probe.update_mask(0, 0, 0, 0, 0, group);
    probe.update_key(keycode, xkb::KeyDirection::Down);
    probe.serialize_mods(xkb::STATE_MODS_LOCKED) != 0
        || probe.serialize_layout(xkb::STATE_LAYOUT_LOCKED) != group
}

#[cfg(test)]
mod tests;
