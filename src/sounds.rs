//! The sound design: each moment of the powers as layers of vanilla sounds, with their volume,
//! pitch, delay and who hears them; the wind of the flight by speed; and how late a far echo
//! arrives. Pure: the client plays its own layers, the server the ones others hear too.

/// Who hears a layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Audience {
    /// Only the one acting, wherever they have gone by then.
    Actor,
    /// Everyone near where it happened.
    Here,
    /// Everyone in the level up to `FAR_RANGE` blocks away, later the farther they are (sound
    /// takes its time), and quieter.
    Far,
}

/// One sound of a cue.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layer {
    pub sound: &'static str,
    pub volume: f32,
    pub pitch: f32,
    /// Ticks after the moment.
    pub delay: u32,
    pub audience: Audience,
}

const fn layer(sound: &'static str, volume: f32, pitch: f32, delay: u32, audience: Audience) -> Layer {
    Layer { sound, volume, pitch, delay, audience }
}

/// A moment that sounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cue {
    /// The flight starts (heard by the flyer).
    TakeOff,
    /// A dash (the flyer).
    Dash,
    /// A landing that isn't an impact (the flyer); see [`landing`].
    Landing,
    /// Becoming a Viltrumite in the choice (the chooser).
    Awaken,
    /// Breaking the sound barrier: half a second of quiet for the flyer, then the boom; the
    /// thunder of it rolls out to the far ones.
    Sonic,
    /// The sonic tunnel cutting through rock.
    Tunnel,
    /// A powered punch landing.
    Punch,
    Grab,
    Throw,
    Clap,
    /// A dive into the ground, with pebbles falling after.
    Impact,
}

use Audience::{Actor, Far, Here};

/// The layers of `cue`.
pub fn layers(cue: Cue) -> &'static [Layer] {
    match cue {
        Cue::TakeOff => const { &[
            layer("minecraft:entity.ender_dragon.flap", 0.7, 0.7, 0, Actor),
            layer("minecraft:entity.breeze.wind_burst", 0.8, 0.8, 0, Actor),
        ] },
        Cue::Dash => const { &[
            layer("minecraft:entity.breeze.wind_burst", 0.9, 1.5, 0, Actor),
            layer("minecraft:entity.player.attack.sweep", 0.6, 0.6, 0, Actor),
        ] },
        Cue::Landing => const { &[layer("minecraft:entity.player.big_fall", 1.0, 0.7, 0, Actor)] },
        Cue::Awaken => const { &[
            layer("minecraft:block.beacon.activate", 1.0, 1.0, 0, Actor),
            layer("minecraft:block.bell.use", 0.8, 0.6, 4, Actor),
            layer("minecraft:entity.lightning_bolt.thunder", 0.4, 1.2, 10, Actor),
        ] },
        Cue::Sonic => const { &[
            layer("minecraft:entity.warden.sonic_boom", 1.6, 1.0, SONIC_QUIET, Actor),
            layer("minecraft:entity.generic.explode", 1.2, 0.5, SONIC_QUIET, Actor),
            layer("minecraft:entity.warden.sonic_boom", 1.4, 0.9, 0, Here),
            layer("minecraft:entity.lightning_bolt.thunder", 0.9, 0.6, 20, Far),
            layer("minecraft:entity.lightning_bolt.thunder", 0.5, 0.5, 38, Far),
        ] },
        Cue::Tunnel => const { &[
            layer("minecraft:entity.generic.explode", 0.6, 0.55, 0, Here),
            layer("minecraft:block.deepslate.break", 1.0, 0.6, 0, Here),
        ] },
        Cue::Punch => const { &[
            layer("minecraft:entity.player.attack.crit", 1.0, 0.8, 0, Here),
            layer("minecraft:item.mace.smash_ground", 0.8, 1.2, 0, Here),
            layer("minecraft:entity.generic.explode", 0.3, 1.6, 3, Here),
        ] },
        Cue::Grab => const { &[
            layer("minecraft:entity.player.attack.strong", 1.0, 0.8, 0, Here),
            layer("minecraft:entity.ravager.attack", 0.6, 0.7, 0, Here),
        ] },
        Cue::Throw => const { &[
            layer("minecraft:entity.breeze.wind_burst", 1.0, 0.8, 0, Here),
            layer("minecraft:entity.player.attack.knockback", 1.2, 0.6, 0, Here),
        ] },
        Cue::Clap => const { &[
            layer("minecraft:entity.lightning_bolt.impact", 1.2, 1.4, 0, Here),
            layer("minecraft:entity.warden.sonic_boom", 1.0, 1.3, 0, Here),
            layer("minecraft:block.bell.resonate", 1.0, 0.8, 4, Here),
            layer("minecraft:entity.lightning_bolt.thunder", 0.8, 1.0, 0, Far),
        ] },
        Cue::Impact => const { &[
            layer("minecraft:entity.generic.explode", 2.0, 0.6, 0, Here),
            layer("minecraft:item.mace.smash_ground_heavy", 1.5, 0.8, 0, Here),
            layer("minecraft:entity.lightning_bolt.thunder", 0.6, 0.5, 0, Far),
            layer("minecraft:block.pointed_dripstone.land", 0.6, 0.9, 8, Here),
            layer("minecraft:block.gravel.break", 0.5, 0.8, 12, Here),
            layer("minecraft:block.pointed_dripstone.land", 0.5, 1.1, 16, Here),
            layer("minecraft:block.pointed_dripstone.land", 0.4, 0.8, 26, Here),
        ] },
    }
}

/// The quiet before the sonic boom, in ticks (half a second).
pub const SONIC_QUIET: u32 = 10;
/// The ringing in the ears of a player in a clap's cone.
pub const RINGING: Layer = layer("minecraft:block.amethyst_block.resonate", 1.0, 2.0, 0, Actor);
/// How far a far layer is heard, in blocks.
pub const FAR_RANGE: f64 = 160.0;
/// Sound's speed: 343 m/s is 17 blocks a tick.
const SOUND_SPEED: f64 = 343.0 / 20.0;

/// How many ticks later than the moment a far layer reaches someone `distance` blocks away.
pub fn echo_delay(distance: f64) -> u32 {
    (distance.max(0.0) / SOUND_SPEED).round() as u32
}

/// A far layer's volume `distance` blocks away: whole near, gone at `FAR_RANGE`; None past it.
pub fn far_volume(volume: f32, distance: f64) -> Option<f32> {
    (distance <= FAR_RANGE).then(|| volume * (1.0 - (distance / FAR_RANGE) as f32 * 0.8))
}

/// The wind of the flight: its loop, and its volume and pitch at `speed` (b/t) out of
/// `max_speed`; silent on the ground.
pub const WIND: &str = "minecraft:item.elytra.flying";

pub fn wind(flying: bool, speed: f64, max_speed: f64) -> (f32, f32) {
    if !flying || max_speed <= 0.0 {
        return (0.0, 1.0);
    }
    let share = (speed / max_speed).clamp(0.0, 1.0) as f32;
    (0.08 + 0.92 * share * share, 0.7 + 0.6 * share)
}

/// A landing's thud at `speed` (b/t): louder and deeper the faster; None for a soft one.
pub fn landing(speed: f64) -> Option<(f32, f32)> {
    (speed >= 0.3).then(|| {
        let s = (speed / 2.0).clamp(0.0, 1.0) as f32;
        (0.4 + 0.8 * s, 0.9 - 0.3 * s)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Cue; 11] = [
        Cue::TakeOff,
        Cue::Dash,
        Cue::Landing,
        Cue::Awaken,
        Cue::Sonic,
        Cue::Tunnel,
        Cue::Punch,
        Cue::Grab,
        Cue::Throw,
        Cue::Clap,
        Cue::Impact,
    ];

    #[test]
    fn every_cue_sounds_with_sane_layers() {
        for cue in ALL {
            let layers = layers(cue);
            assert!(!layers.is_empty(), "{cue:?}");
            for l in layers {
                assert!(l.sound.starts_with("minecraft:") && !l.sound.contains(' '), "{l:?}");
                assert!(l.volume > 0.0 && l.volume <= 2.0 && (0.5..=2.0).contains(&l.pitch), "{l:?}");
                assert!(l.delay <= 40, "{l:?}");
            }
        }
    }

    #[test]
    fn the_sonic_boom_waits_half_a_second_for_the_flyer() {
        let actor: Vec<_> = layers(Cue::Sonic).iter().filter(|l| l.audience == Actor).collect();
        assert!(actor.iter().all(|l| l.delay == SONIC_QUIET));
        assert!(layers(Cue::Sonic).iter().any(|l| l.audience == Far), "the thunder rolls out");
    }

    #[test]
    fn far_ones_hear_it_later_and_quieter() {
        assert_eq!(echo_delay(0.0), 0);
        assert_eq!(echo_delay(171.5), 10, "about half a second for 170 blocks");
        assert!(echo_delay(40.0) < echo_delay(120.0));
        let near = far_volume(1.0, 10.0).unwrap();
        let far = far_volume(1.0, 150.0).unwrap();
        assert!(near > far && far > 0.0);
        assert_eq!(far_volume(1.0, 200.0), None);
    }

    #[test]
    fn the_wind_grows_with_speed_and_hushes_on_the_ground() {
        assert_eq!(wind(false, 3.0, 4.0).0, 0.0);
        let (slow, slow_pitch) = wind(true, 0.2, 4.0);
        let (fast, fast_pitch) = wind(true, 4.0, 4.0);
        assert!(slow < 0.15 && fast > 0.95 && fast_pitch > slow_pitch);
        assert!(landing(0.1).is_none());
        let (soft, _) = landing(0.5).unwrap();
        let (hard, deep) = landing(3.0).unwrap();
        assert!(hard > soft && deep < 0.9);
    }
}
