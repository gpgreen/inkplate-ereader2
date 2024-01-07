pub mod inkplate_platform {
    pub mod battery;
    pub mod inkplate;
    pub mod touch_event;
}
use crate::inkplate_platform::inkplate;
use anyhow::Result;
use esp_idf_svc::{hal::delay, log::EspLogger};
use inkplate_platform::touch_event;
use log::*;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

fn check_free_heap() {
    info!("Minimum free heap size: {} bytes", unsafe {
        esp_idf_svc::sys::esp_get_minimum_free_heap_size()
    });
}

fn main() {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    EspLogger::initialize_default();
    // set the logging level
    let _ = EspLogger.set_target_level("Wire", ::log::LevelFilter::Info);
    let _ = EspLogger.set_target_level("ESP", ::log::LevelFilter::Info);
    let _ = EspLogger.set_target_level("spi_master", ::log::LevelFilter::Info);
    let _ = EspLogger.set_target_level("memory_layout", ::log::LevelFilter::Info);
    let _ = EspLogger.set_target_level("sdmmc_cmd", ::log::LevelFilter::Info);
    let _ = EspLogger.set_target_level("sdspi_transaction", ::log::LevelFilter::Info);
    let _ = EspLogger.set_target_level("sdspi_host", ::log::LevelFilter::Info);
    let _ = EspLogger.set_target_level("bus_lock", ::log::LevelFilter::Info);
    let _ = EspLogger.set_target_level("vfs_fat", ::log::LevelFilter::Debug);
    let _ = EspLogger.set_target_level("cpu_start", ::log::LevelFilter::Info);

    info!("Starting inkplate-ereader2 application!");
    check_free_heap();
    let flags: u32 = (1 << 1) | (1 << 2) | (1 << 10) | (1 << 11);
    unsafe {
        esp_idf_svc::sys::heap_caps_print_heap_info(flags);
    }

    debug!("Rust main thread: {:?}", thread::current());
    info!("Spawning app thread");
    // spawn the main thread, panic if it fails
    //cfg.priority = esp_idf_sys::configMAX_PRIORITIES - 1
    let _builder = thread::Builder::new()
        .name("app_thd".to_string())
        .stack_size(80000)
        .spawn(main_task)
        .unwrap();
    loop {
        thread::sleep(Duration::from_secs(1));
    }
}

fn main_task() -> Result<()> {
    // setup the board
    let mut inkplate = inkplate::inkplate_setup()?;

    // spawn the touch event thread
    let touch_sensor = inkplate.touch_sensor.take().unwrap();
    let touch_sensor_ip = inkplate.touch_sensor_int_pin.take().unwrap();
    let (touch_send_ch, touch_receive_ch) = mpsc::channel();
    let display_config = inkplate.graphics.unwrap().config();
    let _builder = thread::Builder::new()
        .name("touch_thd".to_string())
        .stack_size(20000)
        .spawn(move || {
            touch_event::touch_event_thread(
                touch_sensor,
                touch_send_ch,
                display_config,
                touch_sensor_ip,
            )
        });

    let utc = inkplate.rtc.unwrap().get_datetime().unwrap();
    info!("time from rtc: {}", utc);
    // read the battery
    let mut delay = delay::Ets;
    let mut adc1 = inkplate.adc1.take().unwrap();
    let mut bat_mon = inkplate.bat_mon.take().unwrap();
    let level = bat_mon.read_level(&mut adc1, &mut delay)?;
    info!("battery level: {}", level);
    loop {
        let evt = touch_receive_ch.recv()?;
        debug!("touch event: {:?}", evt);
        check_free_heap();
    }
}

// fn old_main_task() -> Result<()> {
//     // inkplate-drivers

//     // grab some hardware
//     let dp = Peripherals::take().unwrap();
//     let i2c0 = dp.i2c0;
//     let sda = dp.pins.gpio21;
//     let scl = dp.pins.gpio22;
//     let config = I2cConfig::new().baudrate(400_u32.kHz().into());
//     let i2c0 = I2cDriver::new(i2c0, sda, scl, &config).unwrap();
//     let i2c_bus0: &'static _ = shared_bus::new_std!(I2cDriver<'static> = i2c0).unwrap();

//     let mut _delay = delay::Ets;

//     // create a real time clock
//     let mut rtc = Rtc::new(i2c_bus0.acquire_i2c());
//     let utc = rtc.get_datetime().unwrap();
//     info!("time from rtc: {}", utc);

//     // setup the sdcard
//     unsafe {
//         let err = esp_idf_svc::sys::sdcard_setup();
//         info!("sdcard_setup retval: {}", err);
//     }

//     // list the root directory
//     let root_dir = std::path::Path::new("/sdcard");
//     for entry in std::fs::read_dir(root_dir).unwrap() {
//         let entry = entry.unwrap();
//         info!("dir entry: {:?}", &entry);
//     }

//     // what is the tempdir
//     std::env::set_var("TMPDIR", "/sdcard/tmp");
//     info!("temp_dir: {:?}", std::env::temp_dir());

//     // try simpledb
//     let db_path = std::path::Path::new("/sdcard/simple.db");
//     let mut db = SimpleDb::new(db_path).unwrap();
//     for i in 0..4 {
//         let mut v = Vec::new();
//         for j in 0..i + 1 {
//             v.push(j);
//         }
//         db.add_record(&v).unwrap();
//     }
//     assert!(db.record_count() == 4);

//     db.set_current_idx(0);
//     assert_eq!(db.get_record().unwrap().unwrap(), [0]);
//     assert_eq!(db.get_record_size().unwrap(), 1);

//     db.set_current_idx(1);
//     assert!(db.get_record().unwrap().unwrap() == [0, 1]);
//     assert_eq!(db.get_record_size().unwrap(), 2);

//     db.set_current_idx(2);
//     assert!(db.get_record().unwrap().unwrap() == [0, 1, 2]);
//     assert_eq!(db.get_record_size().unwrap(), 3);

//     db.set_current_idx(3);
//     assert!(db.get_record().unwrap().unwrap() == [0, 1, 2, 3]);
//     assert_eq!(db.get_record_size().unwrap(), 4);

//     assert!(!db.have_deleted());
//     std::fs::remove_file(db_path).unwrap();

//     Ok(())
// }
