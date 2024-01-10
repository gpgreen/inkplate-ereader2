pub mod inkplate_platform {
    pub mod battery;
    pub mod inkplate;
    pub mod touch_event;
}
use crate::inkplate_platform::{inkplate, touch_event};
use anyhow::Result;
use ereader_support::page_loc_simpledb::PageLocSimpleDb;
use ereader_support::{
    app_controller::AppController, event_mgr::EventManager, fonts::FaceCacheProxy,
};
use esp_idf_svc::{hal::delay, log::EspLogger};
use inkplate_platform::inkplate::InkPlateDevices;
use log::*;
use static_cel::StaticCell;
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

static DRAW_FACE_CACHE: StaticCell<FaceCacheProxy> = StaticCell::new();

/*
fn main_task() -> Result<()> {
    let mut main_loop = InkplateMainLoopManager::new();
    let mut evt_manager = InkplateMainEventManager::new();
    let db = PageLocSimpleDb::new(Path::new("/sdcard/ereader/book.db"))?;
    let (app_ctrl, draw_face_cache) = AppController::new(Path::new("/sdcard"), db);
    let face_cache_ref: &'static FaceCacheProxy = DRAW_FACE_CACHE.init(draw_face_cache);
    main_loop.init();
    main_loop.run(evt_manager);
}
 */

struct InkplateMainLoopManager<'a> {
    inkplate: Option<InkPlateDevices<'a>>,
}

impl<'a> InkplateMainLoopManager<'a> {
    pub fn new() -> Self {
        Self { inkplate: None }
    }

    pub fn init(&mut self) -> Result<()> {
        // setup the board
        let mut inkplate = inkplate::inkplate_setup()?;

        // spawn the touch event thread
        let touch_sensor = inkplate.touch_sensor.take().unwrap();
        let touch_sensor_ip = inkplate.touch_sensor_int_pin.take().unwrap();
        let (touch_send_ch, touch_receive_ch) = mpsc::channel();
        let display_config = inkplate.graphics.as_ref().unwrap().config();
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
        self.inkplate.replace(inkplate);
        OK(())
    }
}

impl<EM> AppControllerRun<EM> for InkplateMainLoopManager
where
    EM: EventManager,
{
    fn run(
        &mut self,
        mut evt_mgr: EM,
        mut app_ctrl: AppController,
        face_cache: &'static FaceCacheProxy,
    ) -> Result<()> {
        evt_mgr.setup();
        loop {
            evt_mgr.event_loop_handler();
            app_ctrl.event_loop_handler()?;
            if let Some(ev) = evt_mgr.get_event() {
                info!("event: {:?}", ev);
                app_ctrl.input_event(ev)?;
            }
            if let Some(page) = app_ctrl.get_page() {
                info!("got page");
                // draw the page
            }
            let evt = touch_receive_ch.recv()?;
            debug!("touch event: {:?}", evt);
            check_free_heap();
        }
        Ok(())
    }
}

struct InkplateMainEventManager {
    current_event: Option<Event>,
}

impl InkplateMainEventManager {
    pub fn new() -> Self {
        Self {
            current_event: None,
        }
    }
}

impl EventManager for InkplateMainEventManager {
    #[cfg(feature = "touch")]
    fn show_calibration(&self) {}
    #[cfg(feature = "touch")]
    fn calibration_event(&self, ev: TouchEvent) -> bool {
        true
    }
    //fn set_position(&mut self, x: u16, y: u16) {}
    //fn get_position(&self) -> (u16, u16) {}
    //fn to_user_coord(&self, x: u16, y: u16) -> (u16, u16) {
    //    (0, 0)
    //}

    fn setup(&mut self) {}

    fn event_loop_handler(&mut self) {
        // does nothing
    }

    fn get_event(&mut self) -> Option<Event> {
        self.current_event.take()
    }
}
