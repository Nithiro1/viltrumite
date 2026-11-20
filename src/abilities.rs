//! The character's controls: the abilities, the slots their keys fire, and the settings. Pure.
//!
//! The keys are on Options > Controls under the mod's name, where the player rebinds them: four
//! ability slots and the ability menu; boost is the game's own sprint key. Which ability each slot
//! fires is the loadout, saved in the mod's data; the ability menu (K) changes it. The settings
//! are on the mod's page in Options > Mods.

/// An ability a slot can fire. The dive's impact and the flight itself need no slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ability {
    Dash,
    Grab,
    Clap,
}

impl Ability {
    pub const ALL: [Ability; 3] = [Ability::Dash, Ability::Grab, Ability::Clap];

    /// Its id in the saved loadout.
    pub fn id(self) -> &'static str {
        match self {
            Ability::Dash => "dash",
            Ability::Grab => "grab",
            Ability::Clap => "clap",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Ability::Dash => "Dash",
            Ability::Grab => "Agarrar / arremessar",
            Ability::Clap => "Palmas trovejantes",
        }
    }

    pub fn from_id(id: &str) -> Option<Ability> {
        Ability::ALL.into_iter().find(|a| a.id() == id)
    }
}


/// How many ability slots there are, each with its own key.
pub const SLOTS: usize = 4;

/// The mod's keys on Options > Controls: id, label, default. Boost is the game's own sprint key.
pub const KEYS: [(&str, &str, &str); SLOTS + 1] = [
    ("slot1", "Habilidade 1", "key.keyboard.v"),
    ("slot2", "Habilidade 2", "key.keyboard.g"),
    ("slot3", "Habilidade 3", "key.keyboard.r"),
    ("slot4", "Habilidade 4", "key.keyboard.b"),
    ("menu", "Menu de habilidades", "key.keyboard.k"),
];

/// The id of slot `slot`'s key.
pub fn slot_key(slot: usize) -> &'static str {
    KEYS[slot].0
}

/// The id of the menu's key.
pub const MENU_KEY: &str = "menu";
/// The key that lets go of a held mob without a throw: id, label, default.
pub const RELEASE: (&str, &str, &str) = ("release", "Soltar o que segura", "key.keyboard.z");
/// Which ability each slot fires.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Loadout(pub [Option<Ability>; SLOTS]);

impl Default for Loadout {
    fn default() -> Self {
        Loadout([Some(Ability::Dash), Some(Ability::Grab), Some(Ability::Clap), None])
    }
}

impl Loadout {
    /// `dash,grab,clap,-`: one id a slot, `-` for an empty one.
    pub fn encode(&self) -> String {
        self.0.iter().map(|slot| slot.map_or("-", Ability::id)).collect::<Vec<_>>().join(",")
    }

    /// The other way; anything unreadable gives the default, and an ability is kept in one slot only.
    pub fn decode(text: &str) -> Loadout {
        let parts: Vec<&str> = text.trim().split(',').collect();
        if parts.len() != SLOTS {
            return Loadout::default();
        }
        let mut slots = [None; SLOTS];
        for (slot, part) in slots.iter_mut().zip(parts) {
            if part == "-" {
                continue;
            }
            match Ability::from_id(part) {
                Some(ability) => *slot = Some(ability),
                None => return Loadout::default(),
            }
        }
        let mut loadout = Loadout(slots);
        loadout.dedupe();
        loadout
    }

    /// Puts `ability` in `slot` (or empties it), taking it out of any other slot.
    pub fn set(&mut self, slot: usize, ability: Option<Ability>) {
        if slot >= SLOTS {
            return;
        }
        if let Some(ability) = ability {
            for other in self.0.iter_mut() {
                if *other == Some(ability) {
                    *other = None;
                }
            }
        }
        self.0[slot] = ability;
    }

    pub fn slot(&self, slot: usize) -> Option<Ability> {
        self.0.get(slot).copied().flatten()
    }

    fn dedupe(&mut self) {
        for i in 0..SLOTS {
            for j in i + 1..SLOTS {
                if self.0[i].is_some() && self.0[i] == self.0[j] {
                    self.0[j] = None;
                }
            }
        }
    }
}

/// The settings, from the mod's page (Options > Mods > Viltrumite).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    pub hud: bool,
    /// Camera shake, in percent.
    pub shake: u8,
    /// Top flight speed, m/s.
    pub max_speed: u32,
    /// The view widening as you fly faster.
    pub fov: bool,
    /// The sonic flight cuts a tunnel through blocks.
    pub tunnel: bool,
    /// The wind and the flight's own sounds, in percent.
    pub sfx: u8,
}

impl Default for Settings {
    fn default() -> Self {
        Settings::from_page(None, None, None, None, None, None)
    }
}

/// The camera shake's choices, in percent, and the one picked at first.
pub const SHAKES: [u8; 3] = [100, 50, 0];
pub const DEFAULT_SHAKE: usize = 0;
/// The top speed's choices, m/s, and the one picked at first.
pub const SPEEDS: [u32; 4] = [40, 60, 80, 100];
pub const DEFAULT_SPEED: usize = 2;

impl Settings {
    /// From the page's toggles and the indices of its choices; None (not registered, or odd)
    /// keeps the default.
    pub fn from_page(hud: Option<bool>, shake: Option<u32>, max_speed: Option<u32>, fov: Option<bool>, tunnel: Option<bool>, sfx: Option<u32>) -> Settings {
        let pick = |i: Option<u32>, len: usize, default: usize| i.map(|i| i as usize).filter(|&i| i < len).unwrap_or(default);
        Settings {
            hud: hud.unwrap_or(true),
            shake: SHAKES[pick(shake, SHAKES.len(), DEFAULT_SHAKE)],
            max_speed: SPEEDS[pick(max_speed, SPEEDS.len(), DEFAULT_SPEED)],
            fov: fov.unwrap_or(true),
            tunnel: tunnel.unwrap_or(true),
            sfx: SHAKES[pick(sfx, SHAKES.len(), DEFAULT_SHAKE)],
        }
    }

    /// Top speed in blocks a tick.
    pub fn max_speed_bt(&self) -> f64 {
        f64::from(self.max_speed) / 20.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_loadout_matches_phase_two_keys() {
        let l = Loadout::default();
        assert_eq!(l.encode(), "dash,grab,clap,-");
        assert_eq!(l.slot(0), Some(Ability::Dash));
        assert_eq!(l.slot(3), None);
        assert_eq!(l.slot(9), None);
    }

    #[test]
    fn a_loadout_survives_saving_and_bad_text_falls_back() {
        let l = Loadout([None, Some(Ability::Clap), Some(Ability::Dash), Some(Ability::Grab)]);
        assert_eq!(Loadout::decode(&l.encode()), l);
        assert_eq!(Loadout::decode("dash,grab"), Loadout::default());
        assert_eq!(Loadout::decode("dash,fly,clap,-"), Loadout::default());
        assert_eq!(Loadout::decode(""), Loadout::default());
        assert_eq!(Loadout::decode("dash,dash,clap,-").encode(), "dash,-,clap,-", "one slot each");
    }

    #[test]
    fn moving_an_ability_takes_it_out_of_its_old_slot() {
        let mut l = Loadout::default();
        l.set(3, Some(Ability::Dash));
        assert_eq!(l.encode(), "-,grab,clap,dash");
        l.set(1, None);
        assert_eq!(l.encode(), "-,-,clap,dash");
        l.set(7, Some(Ability::Grab));
        assert_eq!(l.encode(), "-,-,clap,dash", "no such slot");
    }

    #[test]
    fn each_slot_and_the_menu_have_a_key() {
        assert_eq!(slot_key(2), "slot3");
        assert_eq!(KEYS[SLOTS].0, MENU_KEY);
        assert_eq!(KEYS[1].2, "key.keyboard.g");
    }

    #[test]
    fn settings_read_from_the_page_keep_defaults_for_odd_values() {
        let d = Settings::default();
        assert_eq!(d, Settings { hud: true, shake: 100, max_speed: 80, fov: true, tunnel: true, sfx: 100 });
        let s = Settings::from_page(Some(false), Some(1), Some(0), Some(false), Some(false), Some(1));
        assert_eq!(s, Settings { hud: false, shake: 50, max_speed: 40, fov: false, tunnel: false, sfx: 50 });
        assert_eq!(Settings::from_page(None, Some(9), Some(4), None, None, Some(7)), d, "out of range");
        assert_eq!(Settings { max_speed: 80, ..s }.max_speed_bt(), 4.0);
    }
}
