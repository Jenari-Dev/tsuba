//! Macro library edits beyond save / delete / import: rename, duplicate, folders. They keep
//! the on-board copies in step (a renamed macro shows its new name on the device).

use anyhow::{ensure, Context, Result};
use tracing::{debug, info, warn};
use tsuba_core::macros as mac;

use crate::manager::{Manager, MACROS_LOCK};

impl Manager {
    /// Rename a library macro, on every attached device that holds a copy too.
    pub fn rename_macro(&self, guid: &str, name: &str) -> Result<()> {
        let name = name.trim();
        ensure!(!name.is_empty(), "the macro needs a name");
        let _g = MACROS_LOCK.lock().unwrap();
        let path = mac::library_path();
        let mut lib = mac::Library::load(&path)?;
        let mut m = lib.get(guid).cloned().context("no such macro in the library")?;
        if m.name == name {
            return Ok(());
        }
        m.name = lib.unique_name(name, guid);
        lib.upsert(m.clone())?;
        lib.save(&path)?;
        if self.ensure_writable().is_ok() {
            let pids: Vec<String> = self.devices.lock().unwrap().keys().cloned().collect();
            for pid in pids {
                if self.ensure_writable_for(&pid).is_err() {
                    continue;
                }
                let r = self.with_device(&pid, |dev, _| {
                    for id in mac::ids(dev)? {
                        if mac::hex(&mac::read_name(dev, id)?.guid) == guid {
                            mac::write_name(dev, id, &m)?;
                            info!("{pid}: renamed macro 0x{id:04x}");
                            debug!("{pid}: macro 0x{id:04x} is now {}", m.name);
                        }
                    }
                    Ok(())
                });
                if let Err(e) = r {
                    warn!("{pid}: renaming the on-board copy of a macro: {e:#}");
                    debug!("{pid}: the macro is {:?}", m.name);
                }
            }
        }
        Ok(())
    }

    /// A copy of a library macro under a new name (not on any device yet).
    pub fn duplicate_macro(&self, guid: &str, name: &str) -> Result<String> {
        ensure!(!name.trim().is_empty(), "the macro needs a name");
        let new = {
            let _g = MACROS_LOCK.lock().unwrap();
            let path = mac::library_path();
            let mut lib = mac::Library::load(&path)?;
            let unique = lib.unique_name(name, "");
            let new = lib.duplicate(guid, &unique)?;
            // Run-command events get slots of their own (the same command shares a slot).
            let mut copy = lib.get(&new).cloned().context("the copy")?;
            if self.alloc_run_slots(&mut copy)? {
                lib.upsert(copy)?;
            }
            lib.save(&path)?;
            new
        };
        crate::shortcuts::sync(self);
        Ok(new)
    }

    /// File a macro under a folder ("" = none).
    pub fn set_macro_group(&self, guid: &str, group: &str) -> Result<()> {
        let _g = MACROS_LOCK.lock().unwrap();
        let path = mac::library_path();
        let mut lib = mac::Library::load(&path)?;
        let mut m = lib.get(guid).cloned().context("no such macro in the library")?;
        m.group = group.trim().chars().take(60).collect();
        lib.upsert(m)?;
        lib.save(&path)
    }
}
