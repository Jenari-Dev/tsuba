//! On-board profiles: list, create (optionally copying another profile), rename, delete and
//! switch. Profile 1 always exists; devices hold up to `max` (5 on the supported models).

use std::io::Read;

use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use tsuba_hid::{Device, Kind, Model};
use tsuba_proto::{decode, profiles as p};

use crate::{apply, config::DpiConfig, performance, state};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileInfo {
    pub id: u8,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profiles {
    pub max: u8,
    pub active: u8,
    pub profiles: Vec<ProfileInfo>,
}

fn first_byte(dev: &Device, cmd: tsuba_proto::Command) -> Result<u8> {
    dev.query(cmd, &[])?.first().copied().context("empty reply")
}

pub fn ids(dev: &Device) -> Result<Vec<u8>> {
    let count = first_byte(dev, p::GET_COUNT)? as usize;
    Ok(p::id_list(&dev.query(p::GET_ID_LIST, &[])?, count))
}

pub fn active(dev: &Device) -> Result<u8> {
    first_byte(dev, p::GET_ACTIVE)
}

/// The profile's name: Synapse's 250-byte block, a firmware default ("default"), or "".
pub fn name(dev: &Device, id: u8) -> String {
    let Ok(first) = dev.query(p::GET_NAME, &p::encode_get_name(id, 0)) else { return String::new() };
    let Some((size, bytes)) = p::name_reply(&first) else { return String::new() };
    if size < p::NAME_BLOCK {
        return p::short_name(&bytes[..size.min(bytes.len())]);
    }
    let mut block = bytes[..bytes.len().min(p::NAME_CHUNK)].to_vec();
    while block.len() < p::NAME_BLOCK {
        let Ok(r) = dev.query(p::GET_NAME, &p::encode_get_name(id, block.len() as u16)) else { break };
        let Some((_, b)) = p::name_reply(&r) else { break };
        let n = (p::NAME_BLOCK - block.len()).min(p::NAME_CHUNK).min(b.len());
        if n == 0 {
            break;
        }
        block.extend_from_slice(&b[..n]);
    }
    p::name_block(&block).map(|n| n.name).unwrap_or_default()
}

pub fn list(dev: &Device) -> Result<Profiles> {
    let profiles = ids(dev)?.into_iter().map(|id| ProfileInfo { id, name: name(dev, id) }).collect();
    Ok(Profiles { max: first_byte(dev, p::GET_MAX)?, active: active(dev)?, profiles })
}

fn write_name(dev: &Device, id: u8, name: &str) -> Result<()> {
    let mut guid = [0u8; 16];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut guid)?;
    let block = p::encode_name_block(&p::NameBlock { guid, name: name.to_string() });
    for (i, chunk) in block.chunks(p::NAME_CHUNK).enumerate() {
        dev.query(p::set_name(chunk.len()), &p::encode_set_name(id, (i * p::NAME_CHUNK) as u16, chunk))?;
    }
    Ok(())
}

pub fn rename(dev: &Device, id: u8, new_name: &str) -> Result<()> {
    ensure!(!new_name.trim().is_empty(), "the profile needs a name");
    write_name(dev, id, new_name.trim())?;
    ensure!(name(dev, id) == new_name.trim(), "device kept the old profile name");
    Ok(())
}

/// Copy key/button assignments (both layers), DPI stages and scroll settings.
pub fn copy(dev: &Device, model: &Model, from: u8, to: u8) -> Result<usize> {
    let mut n = 0;
    for layer in [0, 1] {
        let Ok(list) = state::read_assignments(dev, model, from, layer) else { continue };
        for a in list {
            let x = decode::Assignment { profile: to, key: a.key, layer, function: a.function, data: a.data };
            if apply::set_assignment(dev, model, &x).is_ok() {
                n += 1;
            }
        }
    }
    if model.kind == Kind::Mouse {
        let src = performance::read(dev, model, from)?;
        if let (Some(stages), Some(active_stage)) = (src.dpi_stages.clone(), src.dpi_active_stage) {
            apply::apply_dpi_for(dev, to, &DpiConfig { stages, active_stage, active: None })?;
        }
        let flags = performance::Performance {
            profile: Some(to),
            free_spin: src.free_spin,
            scroll_acceleration: src.scroll_acceleration,
            smart_reel: src.smart_reel,
            ..Default::default()
        };
        performance::apply(dev, model, &flags)?;
    }
    Ok(n)
}

/// Create a profile in the lowest free slot, named `name`, optionally copying `copy_from`.
pub fn create(dev: &Device, model: &Model, new_name: &str, copy_from: Option<u8>) -> Result<u8> {
    let max = first_byte(dev, p::GET_MAX)?;
    let have = ids(dev)?;
    let id = (1..=max).find(|i| !have.contains(i)).with_context(|| format!("the device already holds its maximum of {max} profiles"))?;
    dev.query(p::CREATE, &[id])?;
    ensure!(ids(dev)?.contains(&id), "device didn't create profile {id}");
    rename(dev, id, new_name)?;
    if let Some(src) = copy_from {
        copy(dev, model, src, id)?;
    }
    Ok(id)
}

/// Create on-board profile `id` (a free slot within the device's maximum), named `new_name`.
pub fn create_at(dev: &Device, id: u8, new_name: &str) -> Result<()> {
    let max = first_byte(dev, p::GET_MAX)?;
    ensure!((1..=max).contains(&id), "the device has profile slots 1–{max}");
    ensure!(!ids(dev)?.contains(&id), "profile slot {id} is already in use");
    dev.query(p::CREATE, &[id])?;
    ensure!(ids(dev)?.contains(&id), "device didn't create profile {id}");
    rename(dev, id, new_name)
}

pub fn delete(dev: &Device, id: u8) -> Result<()> {
    ensure!(id != 1, "profile 1 is the device's default profile and can't be deleted");
    if active(dev)? == id {
        set_active(dev, 1)?;
    }
    dev.query(p::DELETE, &[id])?;
    ensure!(!ids(dev)?.contains(&id), "device kept profile {id}");
    Ok(())
}

pub fn set_active(dev: &Device, id: u8) -> Result<()> {
    ensure!(ids(dev)?.contains(&id), "profile {id} doesn't exist");
    dev.query(p::SET_ACTIVE, &[id])?;
    ensure!(active(dev)? == id, "device stayed on profile {}", active(dev)?);
    Ok(())
}
