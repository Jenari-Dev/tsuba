//! Named Chroma Studio profiles (Synapse's effect profiles): each holds a layer stack per
//! device. Stored in `~/.config/tsuba/chroma-profiles.json`; applying one makes it every
//! device's (startup) lighting.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};

use crate::studio::Studio;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ChromaProfile {
    pub name: String,
    /// Device pid → its layers.
    #[serde(default)]
    pub devices: BTreeMap<String, Studio>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ChromaProfiles {
    #[serde(default)]
    pub profiles: Vec<ChromaProfile>,
    /// Name of the applied profile.
    #[serde(default)]
    pub active: String,
}

pub fn path() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("tsuba").join("chroma-profiles.json")
}

impl ChromaProfiles {
    pub fn load(p: &Path) -> Result<Self> {
        match std::fs::read_to_string(p) {
            Ok(s) => serde_json::from_str(&s).with_context(|| format!("parsing {}", p.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).with_context(|| format!("reading {}", p.display())),
        }
    }

    pub fn save(&self, p: &Path) -> Result<()> {
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d)?;
        }
        let tmp = p.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(&tmp, p)?;
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&ChromaProfile> {
        self.profiles.iter().find(|p| p.name == name)
    }

    pub fn upsert(&mut self, p: ChromaProfile) -> Result<()> {
        ensure!(!p.name.trim().is_empty(), "the profile needs a name");
        match self.profiles.iter_mut().find(|x| x.name == p.name) {
            Some(slot) => *slot = p,
            None => self.profiles.push(p),
        }
        Ok(())
    }

    pub fn rename(&mut self, old: &str, new: &str) -> Result<()> {
        ensure!(!new.trim().is_empty(), "the profile needs a name");
        ensure!(self.get(new).is_none(), "a profile named {new:?} already exists");
        let p = self.profiles.iter_mut().find(|x| x.name == old).context("no such profile")?;
        p.name = new.trim().to_string();
        if self.active == old {
            self.active = p.name.clone();
        }
        Ok(())
    }

    pub fn delete(&mut self, name: &str) {
        self.profiles.retain(|p| p.name != name);
        if self.active == name {
            self.active.clear();
        }
    }
}
