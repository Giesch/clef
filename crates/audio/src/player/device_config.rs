//! Device information needed at startup on windows, used for resampling

use std::fmt::Debug;

use anyhow::bail;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SupportedStreamConfig};
use log::*;

pub struct CpalDeviceConfig {
    pub device: Device,
    pub config: SupportedStreamConfig,
}

impl CpalDeviceConfig {
    pub fn get_default() -> anyhow::Result<Self> {
        let host = cpal::default_host();

        let device = match host.default_output_device() {
            Some(device) => device,
            _ => {
                bail!("failed to get default audio output device");
            }
        };

        let config = match device.default_output_config() {
            Ok(config) => config,
            Err(err) => {
                bail!("failed to get default audio output device config: {}", err);
            }
        };

        Ok(Self { device, config })
    }
}

impl Debug for CpalDeviceConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CpalDeviceConfig")
            .field("config", &self.config)
            .finish()
    }
}
