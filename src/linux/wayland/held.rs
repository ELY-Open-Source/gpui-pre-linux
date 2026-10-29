//! Keys a forwarder sends without `wl_keyboard.modifiers`.

use xkbcommon::xkb::{self, Keycode, State};

/// A key down, with the state its press found.
struct Down {
    keycode: Keycode,
    /// Depressed, latched, locked, group; `None` if down unseen.
    under: Option<[u32; 4]>,
}

/// The keys down, oldest first.
#[derive(Default)]
pub(super) struct HeldKeys {
    down: Vec<Down>,
    /// The mask's modifiers, held while an unseen key is down.
    pinned: u32,
}

impl HeldKeys {
    /// The compositor's mask; a state that disagrees is rebuilt.
    pub fn sent(&mut self, state: &mut State, mask: [u32; 4]) {
        let [depressed, latched, locked, group] = mask;
        // Only held keys' counts need a fresh state.
        if state.serialize_mods(xkb::STATE_MODS_DEPRESSED) != depressed {
            *state = State::new(&state.get_keymap());
            // Seen keys again, under the state each press found.
            self.down.retain(|down| match down.under {
                Some(under) => press(state, down.keycode, under, depressed),
                None => true,
            });
        }
        // The mask wins; each key keeps what its release clears.
        state.update_mask(depressed, latched, locked, 0, 0, group);
        self.pinned = if self.unseen_down() { depressed } else { 0 };
    }

    /// Moves the state through a key, as xkb does.
    pub fn key(&mut self, state: &mut State, keycode: Keycode, pressed: bool) {
        if let Some(at) = self.down.iter().position(|down| down.keycode == keycode) {
            let down = self.down.remove(at);
            if pressed && down.under.is_some() {
                // Its release never came.
                state.update_key(keycode, xkb::KeyDirection::Up);
            }
        }
        if pressed {
            let under = [
                state.serialize_mods(xkb::STATE_MODS_DEPRESSED),
                state.serialize_mods(xkb::STATE_MODS_LATCHED),
                state.serialize_mods(xkb::STATE_MODS_LOCKED),
                state.serialize_layout(xkb::STATE_LAYOUT_EFFECTIVE),
            ];
            state.update_key(keycode, xkb::KeyDirection::Down);
            self.down.push(Down {
                keycode,
                under: Some(under),
            });
        } else {
            state.update_key(keycode, xkb::KeyDirection::Up);
        }
        if !self.unseen_down() {
            self.pinned = 0;
        }
        // Unseen keys still hold what the mask said they did.
        let depressed = state.serialize_mods(xkb::STATE_MODS_DEPRESSED);
        if self.pinned & !depressed != 0 {
            state.update_mask(
                depressed | self.pinned,
                state.serialize_mods(xkb::STATE_MODS_LATCHED),
                state.serialize_mods(xkb::STATE_MODS_LOCKED),
                state.serialize_layout(xkb::STATE_LAYOUT_DEPRESSED),
                state.serialize_layout(xkb::STATE_LAYOUT_LATCHED),
                state.serialize_layout(xkb::STATE_LAYOUT_LOCKED),
            );
        }
    }

    /// Keys down before focus, which the mask alone speaks for.
    pub fn unseen(&mut self, down: Vec<Keycode>) {
        self.down = down
            .into_iter()
            .map(|keycode| Down {
                keycode,
                under: None,
            })
            .collect();
    }

    /// A new keymap: its state saw no key down.
    pub fn rekeyed(&mut self) {
        for down in &mut self.down {
            down.under = None;
        }
    }

    /// Focus lost: no key is down here.
    pub fn clear(&mut self) {
        self.down.clear();
        self.pinned = 0;
    }

    fn unseen_down(&self) -> bool {
        self.down.iter().any(|down| down.under.is_none())
    }
}

/// Presses a key again unless the mask lacks its modifiers.
fn press(state: &mut State, keycode: Keycode, under: [u32; 4], mask: u32) -> bool {
    let [depressed, latched, locked, group] = under;
    // What the key alone holds: what its release would clear.
    let mut probe = State::new(&state.get_keymap());
    probe.update_mask(depressed, latched, locked, 0, 0, group);
    probe.update_key(keycode, xkb::KeyDirection::Down);
    let held = probe.serialize_mods(xkb::STATE_MODS_DEPRESSED);
    probe.update_key(keycode, xkb::KeyDirection::Up);
    let own = held & !probe.serialize_mods(xkb::STATE_MODS_DEPRESSED);
    if own & !mask != 0 {
        // The mask says it went up.
        return false;
    }
    state.update_mask(depressed, latched, locked, 0, 0, group);
    state.update_key(keycode, xkb::KeyDirection::Down);
    true
}

#[cfg(test)]
mod tests;
