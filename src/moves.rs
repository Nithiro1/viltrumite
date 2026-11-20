//! Phase 2: the moves. What a client asks for ([`Move`], on [`MOVE_CHANNEL`]) and the geometry the
//! server answers with: who is grabbed, who a clap reaches, how hard an impact lands. No game types.

/// Client to server: one [`Move`].
pub const MOVE_CHANNEL: &str = "viltrumite:move";
/// Server to client: one byte, 1 while holding a mob, 0 when the hands are free.
pub const HELD_CHANNEL: &str = "viltrumite:held";

/// How far a grab reaches along the look, and how far off the line a mob may stand.
pub const GRAB_REACH: f64 = 6.0;
pub const GRAB_RADIUS: f64 = 1.3;
/// A held mob stays this far ahead of the eyes.
pub const HOLD_DISTANCE: f64 = 2.2;
/// A throw's speed along the look, b/t, and its damage.
pub const THROW_SPEED: f64 = 3.0;
pub const THROW_DAMAGE: f32 = 4.0;
/// The thunderclap: its reach, its cone (the cosine of its half angle), damage, push and cooldown.
pub const CLAP_REACH: f64 = 10.0;
pub const CLAP_COS: f64 = 0.82;
pub const CLAP_DAMAGE: f32 = 8.0;
pub const CLAP_PUSH: f64 = 2.5;
pub const CLAP_COOLDOWN: u64 = 60;
/// An impact slower than this (b/t) is a landing, not a slam.
pub const SLAM_MIN_SPEED: f64 = 0.8;
pub const SLAM_COOLDOWN: u64 = 20;
/// A mob's middle above its feet, near enough for zombies, pigs and their like.
const MOB_MIDDLE: f64 = 0.9;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Grab what you look at, or throw what you hold.
    Grab = 1,
    /// The thunderclap.
    Clap = 2,
    /// A dive that hit the ground: `speed` is how fast.
    Slam = 3,
    /// Where the eyes are while holding: the held mob follows.
    Hold = 4,
    /// The sonic flight cuts a tunnel: from the eyes along `look`, `speed` blocks ahead.
    Tunnel = 5,
    /// Let go of the held mob where it is, without a throw.
    Drop = 6,
    /// The flight broke the sound barrier at the eyes: the boom, heard by others too.
    Boom = 7,
}

impl Kind {
    fn from_byte(b: u8) -> Option<Kind> {
        Some(match b {
            1 => Kind::Grab,
            2 => Kind::Clap,
            3 => Kind::Slam,
            4 => Kind::Hold,
            5 => Kind::Tunnel,
            6 => Kind::Drop,
            7 => Kind::Boom,
            _ => return None,
        })
    }
}

/// One move, from the client that makes it: where its eyes are, where they look, how fast it was
/// going (for a slam) and in which dimension.
#[derive(Clone, Debug, PartialEq)]
pub struct Move {
    pub kind: Kind,
    pub eye: [f64; 3],
    pub look: [f64; 3],
    pub speed: f64,
    pub dimension: String,
}

impl Move {
    /// `kind`, then eye, look and speed as 7 little-endian f64s, then the dimension's length and bytes.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(1 + 7 * 8 + 1 + self.dimension.len());
        out.push(self.kind as u8);
        for v in self.eye.iter().chain(&self.look).chain(std::iter::once(&self.speed)) {
            out.extend_from_slice(&v.to_le_bytes());
        }
        let dimension = &self.dimension.as_bytes()[..self.dimension.len().min(64)];
        out.push(dimension.len() as u8);
        out.extend_from_slice(dimension);
        out
    }

    /// The other way, refusing anything malformed: a client's bytes are checked by nothing else.
    pub fn decode(data: &[u8]) -> Option<Move> {
        let kind = Kind::from_byte(*data.first()?)?;
        let mut numbers = [0.0; 7];
        for (i, n) in numbers.iter_mut().enumerate() {
            let at = 1 + i * 8;
            *n = f64::from_le_bytes(data.get(at..at + 8)?.try_into().ok()?);
            if !n.is_finite() || n.abs() > 3.0e7 {
                return None;
            }
        }
        let len = usize::from(*data.get(57)?);
        let dimension = std::str::from_utf8(data.get(58..58 + len)?).ok()?;
        if data.len() != 58 + len
            || dimension.is_empty()
            || !dimension.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_:/.-".contains(&b))
        {
            return None;
        }
        let look = normalize([numbers[3], numbers[4], numbers[5]])?;
        Some(Move {
            kind,
            eye: [numbers[0], numbers[1], numbers[2]],
            look,
            speed: numbers[6].clamp(0.0, 10.0),
            dimension: dimension.to_owned(),
        })
    }
}

/// The mob nearest the eyes along the look, within `reach` and no more than `radius` off the line.
pub fn pick_target(eye: [f64; 3], look: [f64; 3], mobs: &[(u32, [f64; 3])], reach: f64, radius: f64) -> Option<u32> {
    mobs.iter()
        .filter_map(|&(id, feet)| {
            let to = sub(add(feet, [0.0, MOB_MIDDLE, 0.0]), eye);
            let along = dot(to, look);
            let off = length(sub(to, scale(look, along)));
            (along > 0.0 && along <= reach && off <= radius).then_some((id, along))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(id, _)| id)
}

/// Whether a mob standing at `feet` is inside the clap's cone.
pub fn in_cone(eye: [f64; 3], look: [f64; 3], feet: [f64; 3], reach: f64, min_cos: f64) -> bool {
    let to = sub(add(feet, [0.0, MOB_MIDDLE, 0.0]), eye);
    let distance = length(to);
    distance > 0.1 && distance <= reach && dot(to, look) / distance >= min_cos
}

/// Where a held mob's feet go: ahead of the eyes, centred on the look.
pub fn hold_point(eye: [f64; 3], look: [f64; 3]) -> [f64; 3] {
    sub(add(eye, scale(look, HOLD_DISTANCE)), [0.0, MOB_MIDDLE, 0.0])
}

/// A push away from `from` across the ground, `strength` b/t, with `lift` upward.
pub fn push_away(from: [f64; 3], to: [f64; 3], strength: f64, lift: f64) -> [f64; 3] {
    let across = [to[0] - from[0], 0.0, to[2] - from[2]];
    match normalize(across) {
        Some(dir) => [dir[0] * strength, lift, dir[2] * strength],
        None => [0.0, lift, 0.0],
    }
}

/// The impact's reach: wider the faster the dive.
pub fn slam_radius(speed: f64) -> f64 {
    (3.0 + speed * 1.5).clamp(3.0, 9.0)
}

/// The impact's damage at its centre; it falls to half at the edge ([`slam_damage_at`]).
pub fn slam_damage(speed: f64) -> f32 {
    (4.0 + speed * 4.0).min(20.0) as f32
}

pub fn slam_damage_at(speed: f64, distance: f64) -> f32 {
    let edge = (distance / slam_radius(speed)).clamp(0.0, 1.0);
    slam_damage(speed) * (1.0 - 0.5 * edge as f32)
}

/// A dive that the ground stopped: the flight asked to go down at `last_vertical` (b/t) and the
/// player went down far less.
pub fn impact(last_vertical: f64, moved_vertical: f64) -> bool {
    last_vertical < -SLAM_MIN_SPEED && moved_vertical > last_vertical * 0.3
}

/// The sonic tunnel's side, in blocks.
pub const TUNNEL_SIZE: i32 = 4;
/// Going down more steeply than this (the sine of 35°), nothing is cut: a dive ends in an impact.
pub const TUNNEL_STEEPEST: f64 = 0.574;
/// How far ahead a tunnel may be cut in one go, in blocks.
pub const TUNNEL_REACH: f64 = 10.0;
/// How far from the player a tunnel may start, in blocks.
pub const TUNNEL_START: f64 = 8.0;

/// Whether a flight going `dir` (a unit vector) cuts a tunnel.
pub fn tunnels(dir: [f64; 3]) -> bool {
    dir[1] >= -TUNNEL_STEEPEST
}

/// The blocks of a tunnel of `TUNNEL_SIZE` a side swept from `from` (the eyes: the tunnel then runs
/// from the feet to above the head) along `dir` for `length` blocks: at each half block, the cube of
/// blocks nearest around the point. Each once.
pub fn tunnel_cells(from: [f64; 3], dir: [f64; 3], length: f64) -> Vec<[i32; 3]> {
    let half = f64::from(TUNNEL_SIZE) / 2.0;
    let steps = (length.clamp(0.0, TUNNEL_REACH) / 0.5).ceil() as usize;
    let mut cells = std::collections::BTreeSet::new();
    for step in 0..=steps {
        let at = add(from, scale(dir, (step as f64 * 0.5).min(length)));
        let low = at.map(|c| (c - half).round() as i32);
        for dx in 0..TUNNEL_SIZE {
            for dy in 0..TUNNEL_SIZE {
                for dz in 0..TUNNEL_SIZE {
                    cells.insert([low[0] + dx, low[1] + dy, low[2] + dz]);
                }
            }
        }
    }
    cells.into_iter().collect()
}

pub fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub fn scale(a: [f64; 3], by: f64) -> [f64; 3] {
    [a[0] * by, a[1] * by, a[2] * by]
}

pub fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub fn length(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}

fn normalize(a: [f64; 3]) -> Option<[f64; 3]> {
    let l = length(a);
    (l > 1e-9).then(|| scale(a, 1.0 / l))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_move(kind: Kind) -> Move {
        Move { kind, eye: [10.5, 65.62, -3.25], look: [0.0, 0.0, 1.0], speed: 2.5, dimension: "minecraft:overworld".into() }
    }

    #[test]
    fn a_move_survives_the_wire() {
        for kind in [Kind::Grab, Kind::Clap, Kind::Slam, Kind::Hold, Kind::Tunnel, Kind::Drop, Kind::Boom] {
            let m = a_move(kind);
            assert_eq!(Move::decode(&m.encode()), Some(m));
        }
    }

    #[test]
    fn malformed_bytes_are_refused() {
        let good = a_move(Kind::Clap).encode();
        assert!(Move::decode(&[]).is_none());
        assert!(Move::decode(&good[..good.len() - 1]).is_none(), "cut short");
        let mut extra = good.clone();
        extra.push(0);
        assert!(Move::decode(&extra).is_none(), "trailing bytes");
        let mut kind = good.clone();
        kind[0] = 9;
        assert!(Move::decode(&kind).is_none(), "unknown move");
        let mut nan = good.clone();
        nan[1..9].copy_from_slice(&f64::NAN.to_le_bytes());
        assert!(Move::decode(&nan).is_none(), "not a number");
        let mut zero_look = a_move(Kind::Clap);
        zero_look.look = [0.0; 3];
        assert!(Move::decode(&zero_look.encode()).is_none(), "no direction");
        let mut odd = a_move(Kind::Clap);
        odd.dimension = "Overworld!".into();
        assert!(Move::decode(&odd.encode()).is_none(), "not a dimension id");
    }

    #[test]
    fn a_grab_takes_the_nearest_mob_on_the_line() {
        let eye = [0.0, 1.62, 0.0];
        let look = [0.0, 0.0, 1.0];
        let mobs = [(1, [0.0, 0.7, 5.0]), (2, [0.3, 0.7, 3.0]), (3, [3.0, 0.7, 2.0]), (4, [0.0, 0.7, -2.0]), (5, [0.0, 0.7, 9.0])];
        assert_eq!(pick_target(eye, look, &mobs, GRAB_REACH, GRAB_RADIUS), Some(2));
        assert_eq!(pick_target(eye, look, &[(3, [3.0, 0.7, 2.0]), (4, [0.0, 0.7, -2.0])], GRAB_REACH, GRAB_RADIUS), None);
    }

    #[test]
    fn the_clap_reaches_a_cone_ahead() {
        let eye = [0.0, 1.62, 0.0];
        let look = [1.0, 0.0, 0.0];
        assert!(in_cone(eye, look, [6.0, 0.7, 1.0], CLAP_REACH, CLAP_COS));
        assert!(!in_cone(eye, look, [6.0, 0.7, 6.0], CLAP_REACH, CLAP_COS), "too far to the side");
        assert!(!in_cone(eye, look, [12.0, 0.7, 0.0], CLAP_REACH, CLAP_COS), "too far away");
        assert!(!in_cone(eye, look, [-4.0, 0.7, 0.0], CLAP_REACH, CLAP_COS), "behind");
    }

    #[test]
    fn a_held_mob_floats_ahead_of_the_eyes() {
        let feet = hold_point([0.0, 66.62, 0.0], [0.0, 0.0, 1.0]);
        assert!((feet[2] - HOLD_DISTANCE).abs() < 1e-9 && (feet[1] - (66.62 - MOB_MIDDLE)).abs() < 1e-9);
    }

    #[test]
    fn pushes_point_away_across_the_ground() {
        let p = push_away([0.0, 0.0, 0.0], [3.0, 5.0, 4.0], 2.0, 0.5);
        assert!((p[0] - 1.2).abs() < 1e-9 && (p[2] - 1.6).abs() < 1e-9 && p[1] == 0.5);
        assert_eq!(push_away([1.0, 0.0, 1.0], [1.0, 3.0, 1.0], 2.0, 0.5), [0.0, 0.5, 0.0]);
    }

    #[test]
    fn faster_dives_land_wider_and_harder() {
        assert_eq!(slam_radius(0.0), 3.0);
        assert_eq!(slam_radius(4.0), 9.0);
        assert!(slam_damage(4.0) > slam_damage(1.0));
        assert!(slam_damage_at(4.0, 0.0) > slam_damage_at(4.0, 9.0));
        assert_eq!(slam_damage_at(4.0, 9.0), slam_damage(4.0) * 0.5);
    }

    #[test]
    fn only_a_stopped_dive_is_an_impact() {
        assert!(impact(-3.0, 0.0), "dove at 3 b/t and stopped");
        assert!(!impact(-3.0, -2.95), "still diving");
        assert!(!impact(-0.4, 0.0), "a gentle landing");
        assert!(!impact(0.5, 0.0), "going up");
    }

    #[test]
    fn a_straight_tunnel_is_four_by_four_along_the_way() {
        let cells = tunnel_cells([0.5, 64.9, 0.5], [1.0, 0.0, 0.0], 3.0);
        assert!(cells.iter().all(|c| (63..=66).contains(&c[1]) && (-2..=1).contains(&c[2])), "4 high, 4 wide");
        let xs: std::collections::BTreeSet<i32> = cells.iter().map(|c| c[0]).collect();
        assert_eq!(xs.into_iter().collect::<Vec<_>>(), (-2..=5).collect::<Vec<_>>(), "from behind the body to 3 ahead");
        assert_eq!(cells.len(), 8 * 4 * 4, "each block once");
    }

    #[test]
    fn a_tunnel_is_capped_and_a_steep_dive_cuts_nothing() {
        let far = tunnel_cells([0.0; 3], [0.0, 0.0, 1.0], 500.0);
        assert!(far.iter().all(|c| c[2] <= 12), "no further than the reach");
        assert!(tunnels([1.0, 0.0, 0.0]));
        assert!(tunnels(normalize([1.0, -0.5, 0.0]).unwrap()), "a shallow descent cuts");
        assert!(!tunnels(normalize([1.0, -1.0, 0.0]).unwrap()), "45° down is a dive");
        assert!(!tunnels([0.0, -1.0, 0.0]));
    }
}
