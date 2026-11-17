//! Glue between input, game logic, rendering and the save file.

use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    game::{Action, Pet},
    input::Button,
    render::{self, Canvas},
};

const AUTOSAVE_SECS: f32 = 15.0;
const SHAKE_SECS: f32 = 0.4;

pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub struct App {
    pub pet: Pet,
    /// Index into `Action::ALL` of the highlighted menu button.
    pub selected: usize,
    time: f32,
    /// Seconds left of the "no, thanks" head-shake after a refused action.
    shake: f32,
    save_path: PathBuf,
    since_save: f32,
}

impl App {
    pub fn new(pet: Pet, save_path: PathBuf) -> Self {
        Self { pet, selected: 0, time: 0.0, shake: 0.0, save_path, since_save: 0.0 }
    }

    /// Load the saved pet (if any) and catch up on the time that passed.
    pub fn load(save_path: PathBuf) -> Self {
        let mut pet = Pet::default();
        if let Ok(text) = fs::read_to_string(&save_path) {
            if let Some((saved, saved_at)) = Pet::from_text(&text) {
                pet = saved;
                let now = unix_now();
                // The Mini+ clock can reset when the battery dies, so ignore
                // timestamps from the "future" or missing ones.
                if saved_at > 0 && now > saved_at {
                    pet.apply_offline(now - saved_at);
                }
            }
        }
        Self::new(pet, save_path)
    }

    pub fn save(&mut self) {
        if let Some(dir) = self.save_path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        // Write a temp file then rename, so losing power mid-write can't
        // leave a half-written save behind.
        let tmp = self.save_path.with_extension("tmp");
        if fs::write(&tmp, self.pet.to_text(unix_now())).is_ok() {
            let _ = fs::rename(&tmp, &self.save_path);
        }
        self.since_save = 0.0;
    }

    pub fn handle(&mut self, button: Button) {
        let n = Action::ALL.len();
        match button {
            Button::Left | Button::L1 => self.selected = (self.selected + n - 1) % n,
            Button::Right | Button::R1 => self.selected = (self.selected + 1) % n,
            Button::A => {
                if self.pet.perform(Action::ALL[self.selected]) {
                    self.save();
                } else {
                    self.shake = SHAKE_SECS;
                }
            }
            _ => {}
        }
    }

    pub fn update(&mut self, dt: f32) {
        self.time += dt;
        self.shake = (self.shake - dt).max(0.0);
        self.pet.tick(dt);
        self.since_save += dt;
        if self.since_save >= AUTOSAVE_SECS {
            self.save();
        }
    }

    pub fn render(&self, canvas: &mut Canvas) {
        render::draw_scene(canvas, &self.pet, self.selected, self.time, self.shake);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("pocket_pet_test_{}_{}", std::process::id(), name))
    }

    #[test]
    fn menu_selection_wraps() {
        let mut app = App::new(Pet::default(), temp_path("wrap"));
        app.handle(Button::Left);
        assert_eq!(app.selected, 3);
        app.handle(Button::Right);
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn refused_action_shakes_and_does_not_save() {
        let path = temp_path("refuse");
        let _ = fs::remove_file(&path);
        // Feed (selected 0) is refused when full.
        let mut app = App::new(Pet { fullness: 100.0, ..Pet::default() }, path.clone());
        app.handle(Button::A);
        assert!(app.shake > 0.0);
        assert!(!path.exists());
    }

    #[test]
    fn save_and_load_round_trip() {
        let path = temp_path("roundtrip");
        let mut app = App::new(Pet { fullness: 42.0, ..Pet::default() }, path.clone());
        app.save();
        let loaded = App::load(path.clone());
        // Saved "just now", so almost no offline decay is applied.
        assert!((loaded.pet.fullness - 42.0).abs() < 1.0);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn corrupt_save_starts_fresh() {
        let path = temp_path("corrupt");
        fs::write(&path, "not a save file").unwrap();
        let app = App::load(path.clone());
        assert_eq!(app.pet, Pet::default());
        let _ = fs::remove_file(path);
    }
}
