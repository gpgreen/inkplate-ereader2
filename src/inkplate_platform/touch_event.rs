// Copyright (C) 2024 Greg Green <ggreen@bit-builder.com>
// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::inkplate_platform::inkplate::{self, I2c0, MplexOutputPin};
use anyhow::Result;
use esp_idf_svc::hal::{
    gpio::{self, Input, PinDriver},
    task,
};
use inkplate_drivers::{
    eink::config::Config,
    eink::display::Rotation,
    touch_sensor::{TouchSensor, TouchSensorPosition},
};
use log::*;
use std::num::NonZeroU32;
use std::{sync::mpsc, time::Duration};

/// Event kind
#[derive(Debug, Copy, Clone)]
pub enum TouchEventKind {
    None,
    Tap,
    Hold,
    SwipeLeft,
    SwipeRight,
    PinchEnlarge,
    PinchReduce,
    Release,
}

/// The touch event
#[derive(Debug, Copy, Clone)]
pub struct TouchEvent {
    kind: TouchEventKind,
    x: u32,
    y: u32,
    dist: f32,
}

impl TouchEvent {
    /// create a new touch event
    pub fn new(kind: TouchEventKind) -> Self {
        Self {
            kind,
            x: 0,
            y: 0,
            dist: 0.0,
        }
    }
}

// the state of the fsm
#[derive(Debug, Copy, Clone)]
enum TouchEventState {
    None,
    WaitNext { track1: Tracking1Position },
    Holding,
    Swiping { track2: Tracking2Position },
    Pinching { dist: f32 },
}

// Track a single point
#[derive(Debug, Copy, Clone)]
struct Tracking1Position {
    x: f32,
    y: f32,
}

impl Tracking1Position {
    /// transform the touch sensor coordinates to user coordinates
    pub fn transform_coord(&mut self, txfm: &CoordTransform) {
        self.x *= txfm.x_scale;
        self.y *= txfm.y_scale;
        match txfm.rotation {
            Rotation::Rotate0 => {}
            Rotation::Rotate90 => {}
            Rotation::Rotate180 => {}
            Rotation::Rotate270 => {}
        }
    }
}

// track a start and end points
#[derive(Debug, Copy, Clone)]
struct Tracking2Position {
    x: [f32; 2],
    y: [f32; 2],
}

impl Tracking2Position {
    /// transform the touch sensor coordinates to user coordinates
    pub fn transform_coord(&mut self, txfm: &CoordTransform) {
        self.x[0] *= txfm.x_scale;
        self.y[0] *= txfm.y_scale;
        self.x[1] *= txfm.x_scale;
        self.y[1] *= txfm.y_scale;
        match txfm.rotation {
            Rotation::Rotate0 => {}
            Rotation::Rotate90 => {}
            Rotation::Rotate180 => {}
            Rotation::Rotate270 => {}
        }
    }
}

// this is distance where we go from tap or hold to swipe
const DISTANCE_THRESHOLD: f32 = 30.0;

impl From<TouchSensorPosition> for Tracking1Position {
    fn from(tsp: TouchSensorPosition) -> Tracking1Position {
        Tracking1Position {
            x: tsp.x[0] as f32,
            y: tsp.y[0] as f32,
        }
    }
}

impl From<TouchSensorPosition> for Tracking2Position {
    fn from(tsp: TouchSensorPosition) -> Tracking2Position {
        Tracking2Position {
            x: [tsp.x[0] as f32, tsp.x[1] as f32],
            y: [tsp.y[0] as f32, tsp.y[1] as f32],
        }
    }
}

// pythagorean distance between 2 points
fn distance(x0: f32, y0: f32, x1: f32, y1: f32) -> f32 {
    let xsum = x1 - x0;
    let ysum = y1 - y0;
    f32::sqrt(xsum.powi(2) + ysum.powi(2))
}

/// transform touch sensor coordinates to user coordinates
struct CoordTransform {
    x_scale: f32,
    y_scale: f32,
    rotation: Rotation,
}

/// thread function for touch events
pub fn touch_event_thread<'a>(
    mut touch_sensor: TouchSensor<'a, I2c0, MplexOutputPin<'a>, MplexOutputPin<'a>>,
    touch_send_ch: mpsc::Sender<TouchEvent>,
    display_config: Config,
    mut touch_sensor_int_pin: PinDriver<'a, gpio::Gpio36, Input>,
) -> Result<()> {
    info!("started touch event thread");
    unsafe {
        inkplate::register_touch_task();
    }
    touch_sensor_int_pin.enable_interrupt()?;
    // first get the touch sensor dimensions
    let tres = touch_sensor.resolution()?;
    let transform = CoordTransform {
        x_scale: display_config.dimensions.width() as f32 / tres.x() as f32,
        y_scale: display_config.dimensions.height() as f32 / tres.y() as f32,
        rotation: display_config.rotation,
    };
    let mut state = TouchEventState::None;
    let mut timeout = Duration::from_millis(100_000);
    loop {
        if let Some(notice) = task::wait_notification(timeout.as_millis() as u32) {
            if notice == NonZeroU32::new(1).unwrap() {
                trace!("touch sensor notification {}", notice);
                let pos = touch_sensor.get_position()?;
                trace!("state: {:?}", state);
                match state {
                    TouchEventState::None => {
                        if pos.num_fingers == 1 {
                            let mut track1: Tracking1Position = pos.into();
                            track1.transform_coord(&transform);
                            timeout = Duration::from_millis(500);
                            state = TouchEventState::WaitNext { track1 };
                        } else if pos.num_fingers == 2 {
                            let mut track2: Tracking2Position = pos.into();
                            track2.transform_coord(&transform);
                            let dist = distance(track2.x[0], track2.y[0], track2.x[1], track2.y[1]);
                            timeout = Duration::from_millis(100_000);
                            state = TouchEventState::Pinching { dist };
                        }
                    }
                    TouchEventState::WaitNext { track1 } => {
                        if pos.num_fingers == 0 {
                            // got a tap
                            let mut event = TouchEvent::new(TouchEventKind::Tap);
                            event.x = track1.x as u32;
                            event.y = track1.y as u32;
                            touch_send_ch.send(event)?;
                            state = TouchEventState::None;
                        } else if pos.num_fingers == 1 {
                            let mut track1new: Tracking1Position = pos.into();
                            track1new.transform_coord(&transform);
                            let dist = distance(track1.x, track1.y, track1new.x, track1new.y);
                            if dist > DISTANCE_THRESHOLD {
                                let track2 = Tracking2Position {
                                    x: [track1.x, track1new.x],
                                    y: [track1.y, track1new.y],
                                };
                                state = TouchEventState::Swiping { track2 };
                            }
                        } else if pos.num_fingers == 2 {
                            let mut track: Tracking2Position = pos.into();
                            track.transform_coord(&transform);
                            let dist = distance(track.x[1], track.y[1], track.x[0], track.y[0]);
                            state = TouchEventState::Pinching { dist };
                        }
                        timeout = Duration::from_millis(100_000);
                    }
                    TouchEventState::Holding => {
                        if pos.num_fingers == 0 {
                            let event = TouchEvent::new(TouchEventKind::Release);
                            touch_send_ch.send(event)?;
                            state = TouchEventState::None;
                        }
                        // don't need to handle 1 or 2 finger case in holding, continue
                        // with hold to release instead
                        timeout = Duration::from_millis(1000);
                    }
                    TouchEventState::Swiping { track2 } => {
                        if pos.num_fingers == 0 {
                            let mut event = TouchEvent::new(TouchEventKind::SwipeLeft);
                            if track2.x[0] < track2.x[1] {
                                event.kind = TouchEventKind::SwipeRight;
                            }
                            touch_send_ch.send(event)?;
                            state = TouchEventState::None;
                        } else if pos.num_fingers == 1 {
                            let mut track1new: Tracking1Position = pos.into();
                            track1new.transform_coord(&transform);
                            let track2 = Tracking2Position {
                                x: [track2.x[0], track1new.x],
                                y: [track2.y[0], track1new.y],
                            };
                            state = TouchEventState::Swiping { track2 };
                        } else if pos.num_fingers == 2 {
                            let mut track2: Tracking2Position = pos.into();
                            track2.transform_coord(&transform);
                            let dist = distance(track2.x[0], track2.y[0], track2.x[1], track2.y[1]);
                            state = TouchEventState::Pinching { dist };
                        }
                        timeout = Duration::from_millis(100_000);
                    }
                    TouchEventState::Pinching { dist } => {
                        if pos.num_fingers == 0 {
                            let event = TouchEvent::new(TouchEventKind::Release);
                            touch_send_ch.send(event)?;
                            state = TouchEventState::None;
                        } else if pos.num_fingers == 2 {
                            let mut track2new: Tracking2Position = pos.into();
                            track2new.transform_coord(&transform);
                            let this_dist = distance(
                                track2new.x[1],
                                track2new.y[1],
                                track2new.x[0],
                                track2new.y[0],
                            );
                            let new_dist_diff = (dist - this_dist).abs();
                            trace!("distance diffs: {} {}", dist, new_dist_diff);
                            if new_dist_diff > 1.0 {
                                let mut event = TouchEvent::new(TouchEventKind::PinchReduce);
                                if dist < this_dist {
                                    event.kind = TouchEventKind::PinchEnlarge;
                                }
                                event.dist = new_dist_diff;
                                touch_send_ch.send(event)?;
                                state = TouchEventState::Pinching { dist: this_dist };
                            }
                        }
                        timeout = Duration::from_millis(100_000);
                    }
                }
            } else {
                // this is spurious wake up branch
            }
            // enable the interrupt again
            touch_sensor_int_pin.enable_interrupt()?;
        } else {
            trace!("wait timeout state: {:?}", state);
            match state {
                TouchEventState::WaitNext { track1: _ } => {
                    let event = TouchEvent::new(TouchEventKind::Hold);
                    touch_send_ch.send(event)?;
                    timeout = Duration::from_millis(1000);
                    state = TouchEventState::Holding;
                }
                TouchEventState::Holding => {
                    let event = TouchEvent::new(TouchEventKind::Release);
                    touch_send_ch.send(event)?;
                    timeout = Duration::from_millis(100_000);
                    state = TouchEventState::None;
                }
                TouchEventState::Pinching { dist: _ } => {
                    let event = TouchEvent::new(TouchEventKind::Release);
                    touch_send_ch.send(event)?;
                    timeout = Duration::from_millis(100_000);
                    state = TouchEventState::None;
                }
                TouchEventState::Swiping { track2 } => {
                    let mut event = TouchEvent::new(TouchEventKind::SwipeLeft);
                    if track2.x[0] < track2.x[1] {
                        event.kind = TouchEventKind::SwipeRight;
                    }
                    touch_send_ch.send(event)?;
                    timeout = Duration::from_millis(100_000);
                    state = TouchEventState::None;
                }
                _ => {}
            }
        }
    }
}
