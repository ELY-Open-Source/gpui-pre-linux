//! Keys a forwarder sends without `wl_keyboard.modifiers`.

use xkbcommon::xkb::{self, Keycode, State};

/// A key down, with the state its press found.
struct Down {
    keycode: Keycode,
    /// Depressed, latched, locked, group; `None` if down unseen.
    under: Option<[u32; 4]>,
    /// The modifiers its release would clear.
    own: u32,
}

/// The keys down, oldest first.
#[derive(Default)]
pub(super) struct HeldKeys {
    down: Vec<Down>,
    /// What unseen keys hold, by the first mask after them.
    pinned: Option<u32>,
}

impl HeldKeys {
    /// The compositor's mask; a state that disagrees is rebuilt.
    pub fn sent(&mut self, state: &mut State, mask: [u32; 4]) {
        let [depressed, latched, locked, group] = mask;
        let seen = self.down.iter().fold(0, |seen, down| seen | down.own);
        // A seen key's modifier the mask lacks: a lost release.
        if state.serialize_mods(xkb::STATE_MODS_DEPRESSED) & !depressed & seen != 0 {
            *state = State::new(&state.get_keymap());
            // Seen keys again, each under the state its press found.
            self.down.retain(|down| match down.under {
                _ if down.own & !depressed != 0 => false,
                Some([depressed, latched, locked, group]) => {
                    state.update_mask(depressed, latched, locked, 0, 0, group);
                    state.update_key(down.keycode, xkb::KeyDirection::Down);
                    true
                }
                None => true,
            });
        }
        // The mask wins; each key keeps what its release clears.
        state.update_mask(depressed, latched, locked, 0, 0, group);
        // Later masks may only show unseen keys letting go.
        self.pinned = self
            .pinned
            .map(|pinned| pinned & depressed)
            .or(Some(depressed));
        if !self.unseen_down() {
            self.pinned = None;
        }
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
                own: own(state, keycode, under),
            });
        } else {
            state.update_key(keycode, xkb::KeyDirection::Up);
        }
        if !self.unseen_down() {
            self.pinned = None;
        }
        // Unseen keys still hold what the mask said they did.
        let pinned = self.pinned.unwrap_or(0);
        let depressed = state.serialize_mods(xkb::STATE_MODS_DEPRESSED);
        if pinned & !depressed != 0 {
            state.update_mask(
                depressed | pinned,
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
        self.pinned = None;
        self.down = down
            .into_iter()
            .map(|keycode| Down {
                keycode,
                under: None,
                own: 0,
            })
            .collect();
    }

    /// A new keymap: its state saw no key down.
    pub fn rekeyed(&mut self) {
        self.pinned = None;
        for down in &mut self.down {
            down.under = None;
            down.own = 0;
        }
    }

    /// Focus lost: no key down here, nor in the state.
    pub fn left(&mut self, state: &mut State) {
        self.down.clear();
        self.pinned = None;
        // Its group and locks stay, as the layout shown.
        let locked = state.serialize_mods(xkb::STATE_MODS_LOCKED);
        let group = state.serialize_layout(xkb::STATE_LAYOUT_EFFECTIVE);
        *state = State::new(&state.get_keymap());
        state.update_mask(0, 0, locked, 0, 0, group);
    }

    fn unseen_down(&self) -> bool {
        self.down.iter().any(|down| down.under.is_none())
    }
}

/// What a key alone holds: what its release would clear.
fn own(state: &State, keycode: Keycode, under: [u32; 4]) -> u32 {
    let [depressed, latched, locked, group] = under;
    let mut probe = State::new(&state.get_keymap());
    probe.update_mask(depressed, latched, locked, 0, 0, group);
    probe.update_key(keycode, xkb::KeyDirection::Down);
    let held = probe.serialize_mods(xkb::STATE_MODS_DEPRESSED);
    probe.update_key(keycode, xkb::KeyDirection::Up);
    held & !probe.serialize_mods(xkb::STATE_MODS_DEPRESSED)
}

#[cfg(test)]
mod tests;
