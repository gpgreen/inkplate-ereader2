// Copyright (C) 2024 Greg Green <ggreen@bit-builder.com>
// SPDX-License-Identifier: MIT OR Apache-2.0

use anyhow::{anyhow, Result};
use embedded_hal as hal;
use esp_idf_svc::hal::{adc::*, gpio};
use hal::{blocking::delay::DelayUs, digital::v2::OutputPin};
use log::*;
use std::{
    error::Error,
    marker::{Send, Sync},
};

/// Device to measure the Battery voltage
pub struct BatteryMonitor<SW> {
    bat_sw_pin: SW,
    adc_channel: AdcChannelDriver<'static, { attenuation::DB_11 }, gpio::Gpio35>,
}

impl<SW, PE> BatteryMonitor<SW>
where
    SW: OutputPin<Error = PE>,
    PE: Error + Send + Sync + 'static,
{
    /// create the device
    pub fn new(
        bat_sw_pin: SW,
        adc_channel: AdcChannelDriver<'static, { attenuation::DB_11 }, gpio::Gpio35>,
    ) -> Self {
        info!("battery monitor device");
        Self {
            bat_sw_pin,
            adc_channel,
        }
    }

    /// setup the device, default state is disabled
    pub fn read_level<D>(&mut self, adc: &mut AdcDriver<ADC1>, delay: &mut D) -> Result<f64>
    where
        D: DelayUs<u32>,
    {
        self.bat_sw_pin.set_high()?;
        delay.delay_us(1);
        let reading = adc
            .read(&mut self.adc_channel)
            .map_err(|_| anyhow!("battery monitor read failed"))?;
        self.bat_sw_pin.set_low()?;
        debug!("battery voltage raw: {}", reading);
        Ok(reading as f64 * 1.1 * 3.548133892 * 2.0 / 4095.0)
    }
}
