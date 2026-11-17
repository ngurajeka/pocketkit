//! Pure game logic. No drawing, no I/O: easy to unit test.

pub const STAT_MAX: f32 = 100.0;

/// Longest "while you were away" stretch we simulate, so the pet is never
/// hopelessly neglected after a long break.
pub const MAX_OFFLINE_SECS: u64 = 8 * 60 * 60;

/// How long the eat / play / wash animations last (seconds).
pub const FEED_SECS: f32 = 1.6;
pub const PLAY_SECS: f32 = 2.4;
pub const CLEAN_SECS: f32 = 2.0;

// Decay rates in stat points per second. Full to empty takes:
// fullness 3h, happiness 4h, cleanliness 5h, energy 6h.
const FULLNESS_DECAY: f32 = STAT_MAX / (3.0 * 3600.0);
const HAPPINESS_DECAY: f32 = STAT_MAX / (4.0 * 3600.0);
const CLEANLINESS_DECAY: f32 = STAT_MAX / (5.0 * 3600.0);
const ENERGY_DECAY: f32 = STAT_MAX / (6.0 * 3600.0);
/// Sleeping from empty to full takes 90 minutes.
const SLEEP_RESTORE: f32 = STAT_MAX / (90.0 * 60.0);
/// Below this a stat counts as "needs attention".
const LOW: f32 = 25.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Feed,
    Play,
    Sleep,
    Clean,
}

impl Action {
    pub const ALL: [Action; 4] = [Action::Feed, Action::Play, Action::Sleep, Action::Clean];
}

/// What the pet looks like it is feeling right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mood {
    Sleeping,
    Eating,
    Playing,
    Bathing,
    Hungry,
    Dirty,
    Tired,
    Sad,
    Happy,
    Content,
}

/// All stats run 0..=100 where 100 is "great".
#[derive(Clone, Debug, PartialEq)]
pub struct Pet {
    pub fullness: f32,
    pub happiness: f32,
    pub energy: f32,
    pub cleanliness: f32,
    pub sleeping: bool,
    /// Current action animation and seconds left. Not saved.
    pub busy: Option<(Action, f32)>,
}

impl Default for Pet {
    fn default() -> Self {
        Self {
            fullness: 80.0,
            happiness: 80.0,
            energy: 90.0,
            cleanliness: 90.0,
            sleeping: false,
            busy: None,
        }
    }
}

fn fix(v: &mut f32) {
    if !v.is_finite() {
        *v = 50.0;
    }
    *v = v.clamp(0.0, STAT_MAX);
}

impl Pet {
    fn clamp_stats(&mut self) {
        fix(&mut self.fullness);
        fix(&mut self.happiness);
        fix(&mut self.energy);
        fix(&mut self.cleanliness);
    }

    /// Advance the simulation by `dt` seconds.
    pub fn tick(&mut self, dt: f32) {
        if let Some((action, left)) = self.busy {
            let left = left - dt;
            self.busy = if left > 0.0 { Some((action, left)) } else { None };
        }

        if self.sleeping {
            self.energy += SLEEP_RESTORE * dt;
            self.fullness -= FULLNESS_DECAY * 0.4 * dt;
            self.cleanliness -= CLEANLINESS_DECAY * 0.5 * dt;
            if self.energy >= STAT_MAX {
                self.sleeping = false; // naturally wakes up fully rested
            }
        } else {
            self.fullness -= FULLNESS_DECAY * dt;
            self.cleanliness -= CLEANLINESS_DECAY * dt;
            self.energy -= ENERGY_DECAY * dt;
            // Every neglected need makes the pet sadder, faster.
            let neglected = [self.fullness, self.cleanliness, self.energy]
                .iter()
                .filter(|s| **s < LOW)
                .count() as f32;
            self.happiness -= HAPPINESS_DECAY * (1.0 + neglected) * dt;
        }
        self.clamp_stats();
    }

    /// Catch up after the game was closed for `secs` seconds.
    pub fn apply_offline(&mut self, secs: u64) {
        let mut left = secs.min(MAX_OFFLINE_SECS) as f32;
        while left > 0.0 {
            let step = left.min(30.0);
            self.tick(step);
            left -= step;
        }
    }

    /// Try to do something with the pet. Returns `false` if it was refused
    /// (already full, too tired to play, busy, asleep...).
    pub fn perform(&mut self, action: Action) -> bool {
        if self.busy.is_some() {
            return false;
        }
        if self.sleeping {
            // While asleep the only valid action is waking up.
            if action == Action::Sleep {
                self.sleeping = false;
                return true;
            }
            return false;
        }
        match action {
            Action::Feed => {
                if self.fullness >= 95.0 {
                    return false;
                }
                self.fullness += 30.0;
                self.happiness += 3.0;
                self.busy = Some((action, FEED_SECS));
            }
            Action::Play => {
                if self.energy < 15.0 || self.fullness < 10.0 {
                    return false;
                }
                self.happiness += 25.0;
                self.energy -= 12.0;
                self.fullness -= 6.0;
                self.busy = Some((action, PLAY_SECS));
            }
            Action::Clean => {
                self.cleanliness += 45.0;
                self.busy = Some((action, CLEAN_SECS));
            }
            Action::Sleep => {
                if self.energy >= 90.0 {
                    return false; // not tired
                }
                self.sleeping = true;
            }
        }
        self.clamp_stats();
        true
    }

    pub fn mood(&self) -> Mood {
        if self.sleeping {
            return Mood::Sleeping;
        }
        if let Some((action, _)) = self.busy {
            return match action {
                Action::Feed => Mood::Eating,
                Action::Play => Mood::Playing,
                Action::Clean => Mood::Bathing,
                Action::Sleep => Mood::Content,
            };
        }
        if self.fullness < LOW {
            Mood::Hungry
        } else if self.cleanliness < LOW {
            Mood::Dirty
        } else if self.energy < LOW {
            Mood::Tired
        } else if self.happiness < 30.0 {
            Mood::Sad
        } else if self.happiness > 70.0 {
            Mood::Happy
        } else {
            Mood::Content
        }
    }

    /// Simple `key=value` text format: human readable, no dependencies.
    pub fn to_text(&self, saved_at: u64) -> String {
        format!(
            "version=1\nsaved_at={}\nfullness={:.2}\nhappiness={:.2}\nenergy={:.2}\ncleanliness={:.2}\nsleeping={}\n",
            saved_at, self.fullness, self.happiness, self.energy, self.cleanliness, self.sleeping
        )
    }

    /// Parse a save file. Returns the pet and the unix time it was saved at
    /// (0 if unknown). `None` if the file is unusable.
    pub fn from_text(text: &str) -> Option<(Pet, u64)> {
        let mut pet = Pet::default();
        let mut saved_at = 0;
        let mut seen_any = false;
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "saved_at" => saved_at = value.parse().ok()?,
                "fullness" => pet.fullness = value.parse().ok()?,
                "happiness" => pet.happiness = value.parse().ok()?,
                "energy" => pet.energy = value.parse().ok()?,
                "cleanliness" => pet.cleanliness = value.parse().ok()?,
                "sleeping" => pet.sleeping = value.parse().ok()?,
                _ => continue,
            }
            seen_any = true;
        }
        if !seen_any {
            return None;
        }
        pet.clamp_stats();
        Some((pet, saved_at))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_decay_while_awake() {
        let mut pet = Pet::default();
        pet.tick(3600.0);
        // One hour is a third of the 3h fullness budget.
        assert!((pet.fullness - (80.0 - 100.0 / 3.0)).abs() < 0.5);
        assert!(pet.energy < 90.0);
        assert!(pet.happiness < 80.0);
    }

    #[test]
    fn feeding_fills_and_refuses_when_full() {
        let mut pet = Pet { fullness: 40.0, ..Pet::default() };
        assert!(pet.perform(Action::Feed));
        assert_eq!(pet.mood(), Mood::Eating);
        assert!(pet.fullness > 69.0);

        let mut full = Pet { fullness: 100.0, ..Pet::default() };
        assert!(!full.perform(Action::Feed));
    }

    #[test]
    fn cannot_act_while_busy() {
        let mut pet = Pet { fullness: 40.0, ..Pet::default() };
        assert!(pet.perform(Action::Feed));
        assert!(!pet.perform(Action::Clean));
        pet.tick(FEED_SECS + 0.1);
        assert!(pet.busy.is_none());
        assert!(pet.perform(Action::Clean));
    }

    #[test]
    fn playing_needs_energy() {
        let mut tired = Pet { energy: 5.0, ..Pet::default() };
        assert!(!tired.perform(Action::Play));
        let mut ok = Pet::default();
        assert!(ok.perform(Action::Play));
        assert!(ok.happiness > 80.0 || ok.happiness == STAT_MAX);
    }

    #[test]
    fn sleeping_restores_energy_then_wakes() {
        let mut pet = Pet { energy: 10.0, ..Pet::default() };
        assert!(pet.perform(Action::Sleep));
        assert_eq!(pet.mood(), Mood::Sleeping);
        // Can't feed a sleeping pet.
        assert!(!pet.perform(Action::Feed));
        pet.tick(2.0 * 3600.0);
        assert!(!pet.sleeping);
        assert_eq!(pet.energy, STAT_MAX);
    }

    #[test]
    fn cannot_sleep_when_rested() {
        let mut pet = Pet { energy: 95.0, ..Pet::default() };
        assert!(!pet.perform(Action::Sleep));
    }

    #[test]
    fn offline_time_is_capped() {
        let mut a = Pet::default();
        let mut b = Pet::default();
        a.apply_offline(10 * 24 * 3600);
        b.apply_offline(MAX_OFFLINE_SECS);
        assert_eq!(a, b);
    }

    #[test]
    fn neglect_makes_pet_sadder_faster() {
        let mut happy = Pet::default();
        let mut neglected = Pet { fullness: 10.0, cleanliness: 10.0, ..Pet::default() };
        happy.tick(600.0);
        neglected.tick(600.0);
        assert!(neglected.happiness < happy.happiness);
    }

    #[test]
    fn save_round_trip() {
        let pet = Pet {
            fullness: 12.5,
            happiness: 66.0,
            energy: 40.25,
            cleanliness: 99.0,
            sleeping: true,
            busy: None,
        };
        let (loaded, at) = Pet::from_text(&pet.to_text(1_700_000_000)).unwrap();
        assert_eq!(at, 1_700_000_000);
        assert!((loaded.fullness - 12.5).abs() < 0.01);
        assert!((loaded.energy - 40.25).abs() < 0.01);
        assert!(loaded.sleeping);
    }

    #[test]
    fn bad_save_files_are_rejected_or_sanitised() {
        assert!(Pet::from_text("").is_none());
        assert!(Pet::from_text("garbage").is_none());
        assert!(Pet::from_text("fullness=abc").is_none());
        let (pet, _) = Pet::from_text("fullness=NaN\nhappiness=500\nenergy=-3").unwrap();
        assert!(pet.fullness.is_finite());
        assert_eq!(pet.happiness, STAT_MAX);
        assert_eq!(pet.energy, 0.0);
    }

    #[test]
    fn mood_priorities() {
        assert_eq!(Pet::default().mood(), Mood::Happy);
        assert_eq!(Pet { fullness: 5.0, ..Pet::default() }.mood(), Mood::Hungry);
        assert_eq!(Pet { cleanliness: 5.0, ..Pet::default() }.mood(), Mood::Dirty);
        assert_eq!(Pet { energy: 5.0, ..Pet::default() }.mood(), Mood::Tired);
        assert_eq!(Pet { happiness: 10.0, ..Pet::default() }.mood(), Mood::Sad);
    }
}
