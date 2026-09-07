//! Window position and size persistence.
//!
//! Deliberately fixes the C# original's bug: it called Save() on every 1.5 second
//! watchdog tick and wrote the file unconditionally, about 57,000 disk writes a day
//! whether or not the window had moved. This only writes when the value changed.

use std::path::{Path, PathBuf};

const FILE: &str = "window.txt";
const MIN_W: u32 = 300;
const MIN_H: u32 = 200;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Placement {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    pub maximized: bool,
}

impl Placement {
    fn plausible(&self) -> bool {
        self.w >= MIN_W && self.h >= MIN_H
    }
}

pub struct Store {
    path: PathBuf,
    last_written: Option<Placement>,
}

impl Store {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            path: data_dir.join(FILE),
            last_written: None,
        }
    }

    pub fn load(&mut self) -> Option<Placement> {
        let text = std::fs::read_to_string(&self.path).ok()?;
        let mut x = None;
        let mut y = None;
        let mut w = None;
        let mut h = None;
        let mut maximized = false;

        for line in text.lines() {
            let (key, value) = match line.split_once('=') {
                Some((k, v)) => (k.trim(), v.trim()),
                None => continue,
            };
            match key {
                "x" => x = value.parse::<i32>().ok(),
                "y" => y = value.parse::<i32>().ok(),
                "w" => w = value.parse::<u32>().ok(),
                "h" => h = value.parse::<u32>().ok(),
                "maximized" => maximized = value == "1",
                _ => {}
            }
        }

        let p = Placement {
            x: x?,
            y: y?,
            w: w?,
            h: h?,
            maximized,
        };
        if !p.plausible() {
            return None;
        }
        // Remember what is on disk so the first save does not rewrite an identical file.
        self.last_written = Some(p);
        Some(p)
    }

    /// Write only if the placement actually changed. Returns true if a write happened.
    pub fn save_if_changed(&mut self, p: Placement) -> bool {
        if !p.plausible() {
            return false;
        }
        if self.last_written == Some(p) {
            return false;
        }
        let body = format!(
            "x={}\ny={}\nw={}\nh={}\nmaximized={}\n",
            p.x,
            p.y,
            p.w,
            p.h,
            if p.maximized { 1 } else { 0 }
        );
        if std::fs::write(&self.path, body).is_ok() {
            self.last_written = Some(p);
            return true;
        }
        false
    }
}
