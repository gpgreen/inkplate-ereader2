// Copyright (C) 2024 Greg Green <ggreen@bit-builder.com>
// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::inkplate_platform::battery::BatteryMonitor;
use anyhow::{anyhow, Result};
use core::num::NonZeroU32;
use esp_idf_svc::{
    hal::{
        adc::config::Config,
        adc::*,
        delay,
        gpio::{self, Input, InterruptType, PinDriver},
        i2c::{I2cConfig, I2cDriver},
        interrupt,
        peripherals::Peripherals,
        prelude::*,
        task,
    },
    sys,
};
use inkplate_drivers::{
    eink::{
        config::Builder,
        display::{self, ColorDepth, Display},
        eink_reg_update::EinkGpioHelper,
        graphics::GraphicDisplayGray3Bit,
        inkplate_6plus_interface::InkPlate6PlusInterface,
        interface::{InterfaceCtrlPins, InterfaceDataPins, InterfaceMultiplexerPins},
    },
    front_light::FrontLight,
    multiplexer::{Multiplexer, OutputPinProxy, PinName},
    rtc::Rtc,
    touch_sensor::TouchSensor,
};
use log::*;

//////////////////////////////////////////////////////////////////////////////////////
// Define some types to shorten declarations
//////////////////////////////////////////////////////////////////////////////////////

pub type I2c0 = I2cDriver<'static>;

pub type I2cBus0 = &'static shared_bus::BusManager<std::sync::Mutex<I2c0>>;

pub type MplexOutputPin<'a> = OutputPinProxy<'a, I2c0>;

pub type Graphics<'a> = GraphicDisplayGray3Bit<
    InkPlate6PlusInterface<
        'a,
        I2cDriver<'static>,
        gpio::PinDriver<'a, gpio::Gpio0, gpio::Output>,
        gpio::PinDriver<'a, gpio::Gpio2, gpio::Output>,
        gpio::PinDriver<'a, gpio::Gpio32, gpio::Output>,
        gpio::PinDriver<'a, gpio::Gpio33, gpio::Output>,
        OutputPinProxy<'a, I2cDriver<'static>>,
        OutputPinProxy<'a, I2cDriver<'static>>,
        OutputPinProxy<'a, I2cDriver<'static>>,
        OutputPinProxy<'a, I2cDriver<'static>>,
        OutputPinProxy<'a, I2cDriver<'static>>,
        OutputPinProxy<'a, I2cDriver<'static>>,
        OutputPinProxy<'a, I2cDriver<'static>>,
        gpio::PinDriver<'a, gpio::Gpio4, gpio::Output>,
        gpio::PinDriver<'a, gpio::Gpio5, gpio::Output>,
        gpio::PinDriver<'a, gpio::Gpio18, gpio::Output>,
        gpio::PinDriver<'a, gpio::Gpio19, gpio::Output>,
        gpio::PinDriver<'a, gpio::Gpio23, gpio::Output>,
        gpio::PinDriver<'a, gpio::Gpio25, gpio::Output>,
        gpio::PinDriver<'a, gpio::Gpio26, gpio::Output>,
        gpio::PinDriver<'a, gpio::Gpio27, gpio::Output>,
        EinkGpioHelper,
    >,
>;

//////////////////////////////////////////////////////////////////////////////////////
// InkPlate Platform
//////////////////////////////////////////////////////////////////////////////////////

/// The struct to hold all devices
pub struct InkPlateDevices<'a> {
    pub i2c0bus: Option<I2cBus0>,
    pub mplex: Option<Multiplexer<'a, I2c0>>,
    pub touch_sensor: Option<TouchSensor<'a, I2c0, MplexOutputPin<'a>, MplexOutputPin<'a>>>,
    pub touch_sensor_int_pin: Option<PinDriver<'a, gpio::Gpio36, Input>>,
    pub adc1: Option<AdcDriver<'a, ADC1>>,
    pub bat_mon: Option<BatteryMonitor<MplexOutputPin<'a>>>,
    pub front_light: Option<FrontLight<'a, I2c0, MplexOutputPin<'a>>>,
    pub rtc: Option<Rtc<'a, I2c0>>,
    pub graphics: Option<Graphics<'a>>,
}

/// static variable to hold touch sensor task id, for notifications
pub static mut TOUCH_SENSOR_TASK_ID: Option<sys::TaskHandle_t> = None;

/// main function to setup the InkPlate hardware
pub fn inkplate_setup() -> Result<InkPlateDevices<'static>> {
    let mut delay = delay::Ets;

    info!("InkPlate setup starting");

    // grab some hardware
    let dp = Peripherals::take().unwrap();
    let i2c0 = dp.i2c0;
    let sda = dp.pins.gpio21;
    let scl = dp.pins.gpio22;
    let config = I2cConfig::new().baudrate(400_u32.kHz().into());
    let i2c0 = I2cDriver::new(i2c0, sda, scl, &config)?;
    let i2c_bus0: &'static _ = shared_bus::new_std!(I2cDriver<'static> = i2c0).unwrap();

    // create a Multiplexer device
    let mut mplex = Multiplexer::new(i2c_bus0)?;

    // the touch sensor
    // unsafe due to interrupt callback, we are having the interrupt function notify
    // a thread to wakeup via a condvar, so safe enough
    let mut touch_interrupt_pin = PinDriver::input(dp.pins.gpio36)?;
    touch_interrupt_pin.set_interrupt_type(InterruptType::NegEdge)?;
    unsafe {
        touch_interrupt_pin.subscribe({
            move || {
                if let Some(id) = TOUCH_SENSOR_TASK_ID {
                    // if we have a task id in the static variable, then thread is ready
                    // so notify it to wake up
                    task::notify(id, NonZeroU32::new(1u32).unwrap());
                }
            }
        })?;
    }

    let touch_reset_pin = unsafe { mplex.take_pin(PinName::GPB2)?.into_output()? };
    let touch_en_pin = unsafe { mplex.take_pin(PinName::GPB4)?.into_output()? };

    let mut touch_sensor = TouchSensor::new(touch_en_pin, touch_reset_pin, i2c_bus0)?;
    touch_sensor.start_touch_sensor(&mut delay)?;

    // the adc
    let adc1 = AdcDriver::new(dp.adc1, &Config::new().calibration(true))?;

    let batv: AdcChannelDriver<{ attenuation::DB_11 }, _> = AdcChannelDriver::new(dp.pins.gpio35)?;
    let bat_sw_pin = unsafe { mplex.take_pin(PinName::VBatMos)?.into_output()? };

    // the battery monitor
    let bat_mon = BatteryMonitor::new(bat_sw_pin, batv);

    // front light device
    let light_en = unsafe { mplex.take_pin(PinName::GPB3)?.into_output()? };

    let mut front_light = FrontLight::new(i2c_bus0.acquire_i2c(), light_en);
    front_light.setup()?;

    // rtc
    let rtc = Rtc::new(i2c_bus0.acquire_i2c());

    // initialize the sdcard, which includes the dedicated SPI bus
    unsafe {
        let good = sys::sdcard_setup();
        info!("sdcard setup: {}", good);
    }
    std::env::set_var("TMPDIR", "/sdcard/tmp");
    info!("temp_dir: {:?}", std::env::temp_dir());

    // now the eink multiplexer pins
    let oe = unsafe { mplex.take_pin(PinName::EpdOe)?.into_output()? };
    let gmod = unsafe { mplex.take_pin(PinName::EpdGmode)?.into_output()? };
    let spv = unsafe { mplex.take_pin(PinName::EpdSpv)?.into_output()? };
    let wakeup = unsafe { mplex.take_pin(PinName::WakeUp)?.into_output()? };
    let pwr = unsafe { mplex.take_pin(PinName::PwrUp)?.into_output()? };
    let vcom = unsafe { mplex.take_pin(PinName::VcomCtrl)?.into_output()? };
    let gpio_en = unsafe { mplex.take_pin(PinName::Gpio0Mosfet)?.into_output()? };
    let mplex_pins = InterfaceMultiplexerPins {
        oe,
        gmod,
        spv,
        wakeup,
        pwr,
        gpio_en,
        vcom,
    };

    // create a gpio register updater
    let gpio_rs = EinkGpioHelper::new();

    // grab the display io pins
    let ctrl_pins = InterfaceCtrlPins {
        cl: gpio::PinDriver::output(dp.pins.gpio0)?,
        le: gpio::PinDriver::output(dp.pins.gpio2)?,
        ckv: gpio::PinDriver::output(dp.pins.gpio32)?,
        sph: gpio::PinDriver::output(dp.pins.gpio33)?,
    };

    // grab the display data pins
    let data_pins = InterfaceDataPins {
        d0: gpio::PinDriver::output(dp.pins.gpio4)?,
        d1: gpio::PinDriver::output(dp.pins.gpio5)?,
        d2: gpio::PinDriver::output(dp.pins.gpio18)?,
        d3: gpio::PinDriver::output(dp.pins.gpio19)?,
        d4: gpio::PinDriver::output(dp.pins.gpio23)?,
        d5: gpio::PinDriver::output(dp.pins.gpio25)?,
        d6: gpio::PinDriver::output(dp.pins.gpio26)?,
        d7: gpio::PinDriver::output(dp.pins.gpio27)?,
    };

    // display interface
    let eink_interface =
        InkPlate6PlusInterface::new(i2c_bus0, ctrl_pins, mplex_pins, data_pins, gpio_rs);
    // display config
    let eink_config = Builder::new()
        .dimensions(display::Dimensions::R1024x758)
        .depth(ColorDepth::Gray3Bit)
        //.depth(ColorDepth::BW)
        .rotation(display::Rotation::Rotate270)
        .build()
        .map_err(|e| anyhow!("unable to create eink config: {:?}", e))?;
    // the 3bit graphics interface
    let mut graphics = GraphicDisplayGray3Bit::new(Display::new(eink_interface, eink_config));
    // let mut graphics = GraphicDisplayBW::new(eink_display);
    graphics.init(&mut delay)?;

    info!("InkPlate setup done");

    // return all the devices
    Ok(InkPlateDevices {
        i2c0bus: Some(i2c_bus0),
        mplex: Some(mplex),
        touch_sensor: Some(touch_sensor),
        touch_sensor_int_pin: Some(touch_interrupt_pin),
        adc1: Some(adc1),
        bat_mon: Some(bat_mon),
        front_light: Some(front_light),
        graphics: Some(graphics),
        rtc: Some(rtc),
    })
}

/// register a task for the touch sensor interrupt
///
/// this is called from the task to register, following
/// this call, the task will be woken up by the touch sensor
/// interrupt
pub unsafe fn register_touch_task() {
    interrupt::free(|| {
        let task_id = match TOUCH_SENSOR_TASK_ID {
            Some(tid) => tid,
            None => task::current().unwrap(),
        };
        std::ptr::replace(&mut TOUCH_SENSOR_TASK_ID, Some(task_id));
    });
}
