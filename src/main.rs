use ereader_support::simple_db::SimpleDb;
use esp_idf_svc::{
    hal::{
        delay,
        i2c::{I2cConfig, I2cDriver},
        peripherals::Peripherals,
        prelude::*,
    },
    log::EspLogger,
};
use inkplate_drivers::rtc::Rtc;
use log::*;
use shared_bus;
use std::sync::Arc;

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
    EspLogger.set_target_level("Wire", ::log::LevelFilter::Info);
    EspLogger.set_target_level("ESP", ::log::LevelFilter::Info);
    EspLogger.set_target_level("spi_master", ::log::LevelFilter::Info);
    EspLogger.set_target_level("memory_layout", ::log::LevelFilter::Info);
    EspLogger.set_target_level("sdmmc_cmd", ::log::LevelFilter::Info);
    EspLogger.set_target_level("sdspi_transaction", ::log::LevelFilter::Info);
    EspLogger.set_target_level("sdspi_host", ::log::LevelFilter::Info);
    EspLogger.set_target_level("bus_lock", ::log::LevelFilter::Info);
    EspLogger.set_target_level("vfs_fat", ::log::LevelFilter::Debug);
    EspLogger.set_target_level("cpu_start", ::log::LevelFilter::Info);
    EspLogger.set_target_level("sled", ::log::LevelFilter::Debug);

    info!("Hello, world!");
    check_free_heap();
    let flags: u32 = (1 << 1) | (1 << 2) | (1 << 10) | (1 << 11);
    unsafe {
        esp_idf_svc::sys::heap_caps_print_heap_info(flags);
    }
    // inkplate-drivers
    // grab some hardware
    let dp = Peripherals::take().unwrap();
    let i2c0 = dp.i2c0;
    let sda = dp.pins.gpio21;
    let scl = dp.pins.gpio22;
    let config = I2cConfig::new().baudrate(400_u32.kHz().into());
    let i2c0 = I2cDriver::new(i2c0, sda, scl, &config).unwrap();
    let i2c_bus0: &'static _ = shared_bus::new_std!(I2cDriver<'static> = i2c0).unwrap();

    let mut delay = delay::Ets;

    // create a real time clock
    let mut rtc = Rtc::new(i2c_bus0.acquire_i2c());
    let utc = rtc.get_datetime().unwrap();
    info!("time from rtc: {}", utc);

    // setup the sdcard
    unsafe {
        let err = esp_idf_svc::sys::sdcard_setup();
        info!("sdcard_setup retval: {}", err);
    }

    // list the root directory
    let root_dir = std::path::Path::new("/sdcard");
    for entry in std::fs::read_dir(root_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        info!("dir entry: {:?}", &entry);
        if path == std::path::Path::new("/sdcard/welcome-to-sled") {
            for entry in std::fs::read_dir(path).unwrap() {
                info!("dir entry: {:?}", &entry.unwrap());
            }
        }
    }

    // what is the tempdir
    std::env::set_var("TMPDIR", "/sdcard/tmp");
    info!("temp_dir: {:?}", std::env::temp_dir());

    // try simpledb
    let db_path = std::path::Path::new("/sdcard/simple.db");
    let mut db = SimpleDb::new(&db_path).unwrap();
    for i in 0..4 {
        let mut v = Vec::new();
        for j in 0..i + 1 {
            v.push(j);
        }
        db.add_record(&v).unwrap();
    }
    assert!(db.record_count() == 4);

    db.set_current_idx(0);
    assert_eq!(db.get_record().unwrap().unwrap(), [0]);
    assert_eq!(db.get_record_size().unwrap(), 1);

    db.set_current_idx(1);
    assert!(db.get_record().unwrap().unwrap() == [0, 1]);
    assert_eq!(db.get_record_size().unwrap(), 2);

    db.set_current_idx(2);
    assert!(db.get_record().unwrap().unwrap() == [0, 1, 2]);
    assert_eq!(db.get_record_size().unwrap(), 3);

    db.set_current_idx(3);
    assert!(db.get_record().unwrap().unwrap() == [0, 1, 2, 3]);
    assert_eq!(db.get_record_size().unwrap(), 4);

    assert!(!db.have_deleted());
    std::fs::remove_file(&db_path);
}
