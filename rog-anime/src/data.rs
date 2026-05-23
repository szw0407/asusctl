use std::convert::TryFrom;
use std::str::FromStr;
use std::thread::sleep;
use std::time::{Duration, Instant};

use dmi_id::DMIID;
use log::info;
use serde::{Deserialize, Serialize};
#[cfg(feature = "dbus")]
use zbus::zvariant::{OwnedValue, Type, Value};

use crate::error::{AnimeError, Result};
use crate::usb::{AnimAwake, AnimBooting, AnimShutdown, AnimSleeping, Brightness};
use crate::{AnimTime, AnimeGif};

/// The first 7 bytes of every AniMe Matrix USB packet are the header:
///   `[0x5e, 0xc0, 0x02, START_LO, START_HI, LEN_LO, LEN_HI]`
///
/// where `START` and `LEN` are little-endian `u16` values describing the
/// LED index range this packet writes (LED indices are 1-based on the
/// wire). The remaining bytes (up to `BLOCK_END`) carry LED color data;
/// the rest of the 640-byte packet is zero padding.
///
/// Per-device packet header bytes are returned by [`usb_prefixes_for`].
/// The chunking strategy — how many LED bytes go in each packet — is
/// per-device-class:
///
/// - GA401, GA402, GU604, G835L: `PANE_LEN` bytes per pane (universal).
/// - G635L: 490 bytes (pane 1), 320 bytes (pane 2) — exact-fit chunking
///   for the 810-LED matrix, per G Helper's `AnimeMatrixDevice`. See
///   the doc comment on [`usb_prefixes_for`] for source citations.
const BLOCK_START: usize = 7;
/// *Not* inclusive — the byte before this is the final usable byte of
/// each "pane" (packet payload region). 640 (packet size) - 6 (trailing
/// padding required by HID feature reports).
const BLOCK_END: usize = 634;
/// Maximum usable LED data length per USB packet. Used by
/// GA401/GA402/GU604/G835L. G635L uses smaller per-pane lengths
/// ([`G635L_PANE1_LEN`], [`G635L_PANE2_LEN`]) — see [`usb_prefixes_for`].
const PANE_LEN: usize = BLOCK_END - BLOCK_START;

/// First packet is for GA401 + GA402
pub const USB_PREFIX1: [u8; 7] = [
    0x5e, 0xc0, 0x02, 0x01, 0x00, 0x73, 0x02,
];
/// Second packet is for GA401 + GA402
pub const USB_PREFIX2: [u8; 7] = [
    0x5e, 0xc0, 0x02, 0x74, 0x02, 0x73, 0x02,
];
/// Third packet is for GA402 matrix
pub const USB_PREFIX3: [u8; 7] = [
    0x5e, 0xc0, 0x02, 0xe7, 0x04, 0x73, 0x02,
];
/// First packet header for G635L: start=1, length=490 (per G Helper STRIX-class chunking)
pub const USB_PREFIX_G635L_1: [u8; 7] = [
    0x5e, 0xc0, 0x02, 0x01, 0x00, 0xea, 0x01,
];
/// Second packet header for G635L: start=491, length=320 (per G Helper STRIX-class chunking)
pub const USB_PREFIX_G635L_2: [u8; 7] = [
    0x5e, 0xc0, 0x02, 0xeb, 0x01, 0x40, 0x01,
];
/// LED data length carried in G635L packet 1 (pane 1).
/// G635L's 810-LED matrix splits exactly as 490 + 320 per G Helper STRIX-class chunking.
pub const G635L_PANE1_LEN: usize = 490;
/// LED data length carried in G635L packet 2 (pane 2). Equals
/// `AnimeType::G635L.data_length() - G635L_PANE1_LEN`.
pub const G635L_PANE2_LEN: usize = 320;

/// USB packet header bytes for each pane of an AniMe Matrix data write
/// transaction, per device class.
///
/// The protocol structure for each data packet is:
///   `[0x5e, 0xc0, 0x02, START_LO, START_HI, LEN_LO, LEN_HI, ...payload..., zeros]`
/// where `START` and `LEN` are little-endian `u16` values describing the
/// LED index range this packet writes (LED indices are 1-based on the wire).
///
/// Per-device chunking strategy (UpdatePageLength values, sourced from
/// G Helper's AnimeMatrixDevice — see citation at the top of the
/// `G635L` arm below):
/// - GA401: PANE_LEN bytes per pane (universal)
/// - GA402: PANE_LEN bytes per pane (universal)
/// - GU604: PANE_LEN bytes per pane (universal)
/// - G835L: PANE_LEN bytes per pane — currently shares the universal
///   chunking. Per G Helper this should be 490+320 (STRIX class),
///   but G835L is intentionally left unchanged here pending
///   hardware verification by a G835L owner.
/// - G635L: 490 bytes (pane 1), 320 bytes (pane 2) — exact fit for the
///   810-LED matrix.
pub fn usb_prefixes_for(anime_type: AnimeType) -> Vec<[u8; 7]> {
    match anime_type {
        // G635L: STRIX-class chunking (490 + 320), per G Helper:
        //   https://github.com/seerge/g-helper/blob/main/app/AnimeMatrix/AnimeMatrixDevice.cs
        // The original protocol knowledge comes from vddCore/Starlight
        // (referenced at the top of the same G Helper file).
        AnimeType::G635L => vec![
            USB_PREFIX_G635L_1, USB_PREFIX_G635L_2,
        ],
        // GA401 and G835L: two panes, universal PANE_LEN-based prefixes.
        AnimeType::GA401 | AnimeType::G835L => vec![
            USB_PREFIX1, USB_PREFIX2,
        ],
        // GA402, GU604, Unsupported: three panes, universal PANE_LEN-based prefixes.
        AnimeType::GA402 | AnimeType::GU604 | AnimeType::Unsupported => {
            vec![
                USB_PREFIX1, USB_PREFIX2, USB_PREFIX3,
            ]
        }
    }
}

#[cfg_attr(feature = "dbus", derive(Type, Value, OwnedValue))]
#[derive(Default, Deserialize, PartialEq, Eq, Clone, Copy, Serialize, Debug)]
pub struct Animations {
    pub boot: AnimBooting,
    pub awake: AnimAwake,
    pub sleep: AnimSleeping,
    pub shutdown: AnimShutdown,
}

// TODO: move this out
#[cfg_attr(feature = "dbus", derive(Type))]
#[derive(Debug, PartialEq, Eq, Copy, Clone, Deserialize, Serialize)]
pub struct DeviceState {
    pub display_enabled: bool,
    pub display_brightness: Brightness,
    pub builtin_anims_enabled: bool,
    pub builtin_anims: Animations,
    pub off_when_unplugged: bool,
    pub off_when_suspended: bool,
    pub off_when_lid_closed: bool,
    pub brightness_on_battery: Brightness,
}

#[cfg_attr(feature = "dbus", derive(Type), zvariant(signature = "s"))]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Deserialize, Serialize, Default)]
pub enum AnimeType {
    GA401,
    GA402,
    GU604,
    G635L,
    G835L,
    #[default]
    Unsupported,
}

impl FromStr for AnimeType {
    type Err = AnimeError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let dmi = s.to_uppercase();

        if dmi.contains("GA401") {
            return Ok(Self::GA401);
        } else if dmi.contains("GA402") {
            return Ok(Self::GA402);
        } else if dmi.contains("GU604") {
            return Ok(Self::GU604);
        } else if dmi.contains("G635L") {
            return Ok(Self::G635L);
        } else if dmi.contains("G835L") {
            return Ok(Self::G835L);
        }

        Ok(Self::Unsupported)
    }
}

impl AnimeType {
    pub fn from_dmi() -> Self {
        let board_name = DMIID::new().unwrap_or_default().board_name.to_uppercase();
        if board_name.contains("GA401I") || board_name.contains("GA401Q") {
            AnimeType::GA401
        } else if board_name.contains("GA402R")
            || board_name.contains("GA402X")
            || board_name.contains("GA402N")
        {
            AnimeType::GA402
        } else if board_name.contains("GU604V") {
            AnimeType::GU604
        } else if board_name.contains("G635L") {
            AnimeType::G635L
        } else if board_name.contains("G835L") {
            AnimeType::G835L
        } else {
            AnimeType::Unsupported
        }
    }

    /// The width of diagonal images
    pub fn width(&self) -> usize {
        match self {
            AnimeType::GU604 => 70,
            // TODO: Find G635L W*H
            AnimeType::G635L => 68,
            AnimeType::G835L => 68,
            _ => 74,
        }
    }

    /// The height of diagonal images
    pub fn height(&self) -> usize {
        match self {
            AnimeType::GA401 => 36,
            AnimeType::GU604 => 43,
            AnimeType::G635L => 34,
            AnimeType::G835L => 34,
            _ => 39,
        }
    }

    /// The length of usable data for this type
    pub fn data_length(&self) -> usize {
        match self {
            AnimeType::GA401 => PANE_LEN * 2,
            // G835L has 810 LEDs: 210 (triangle) + 600 (staggered rectangle)
            // 810 verified on G635L hardware: 210 (triangle, rows 0-27 with
            // lengths 1+1+2+2+...+14+14) + 600 (rectangle, rows 28-67 × 15
            // LEDs each). USB capture evidence is shared on the asus-linux
            // dev Discord (see commit message of the G635L USB packet
            // chunking fix for context).
            AnimeType::G635L => 810,
            AnimeType::G835L => 810,
            _ => PANE_LEN * 3,
        }
    }
}

/// The minimal serializable data that can be transferred over wire types.
/// Other data structures in `rog_anime` will convert to this.
#[cfg_attr(feature = "dbus", derive(Type))]
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AnimeDataBuffer {
    data: Vec<u8>,
    anime: AnimeType,
}

impl AnimeDataBuffer {
    #[inline]
    pub fn new(anime: AnimeType) -> Self {
        let len = anime.data_length();

        AnimeDataBuffer {
            data: vec![0u8; len],
            anime,
        }
    }

    /// Get the inner data buffer
    #[inline]
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Get a mutable slice of the inner buffer
    #[inline]
    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }

    /// Create from a vector of bytes
    ///
    /// # Errors
    /// Will error if the vector length is not `ANIME_DATA_LEN`
    #[inline]
    pub fn from_vec(anime: AnimeType, data: Vec<u8>) -> Result<Self> {
        if data.len() != anime.data_length() {
            return Err(AnimeError::DataBufferLength);
        }

        Ok(Self { data, anime })
    }
}

/// The packets to be written to USB
pub type AnimePacketType = Vec<[u8; 640]>;

/// Split LED `data` into two panes within the USB packet `buffers`, with
/// pane 1 carrying `pane1_len` bytes and pane 2 carrying the remainder.
/// Used for G635L (490 bytes pane 1) and G835L (PANE_LEN bytes pane 1).
fn split_into_panes(buffers: &mut [[u8; 640]], data: &[u8], pane1_len: usize) {
    let pane1_actual = pane1_len.min(data.len());
    buffers[0][BLOCK_START..BLOCK_START + pane1_actual].copy_from_slice(&data[..pane1_actual]);
    if data.len() > pane1_len {
        let pane2_len = data.len() - pane1_len;
        buffers[1][BLOCK_START..BLOCK_START + pane2_len].copy_from_slice(&data[pane1_len..]);
    }
}

impl TryFrom<AnimeDataBuffer> for AnimePacketType {
    type Error = AnimeError;

    fn try_from(anime: AnimeDataBuffer) -> std::result::Result<Self, Self::Error> {
        if anime.data.len() != anime.anime.data_length() {
            return Err(AnimeError::DataBufferLength);
        }

        let mut buffers = match anime.anime {
            AnimeType::GA401 | AnimeType::G635L | AnimeType::G835L => vec![[0; 640]; 2],
            AnimeType::GA402 | AnimeType::GU604 | AnimeType::Unsupported => {
                vec![[0; 640]; 3]
            }
        };

        if anime.anime == AnimeType::G835L {
            // G835L: 627 bytes in pane 1, 183 bytes in pane 2 (universal
            // PANE_LEN-based chunking; pane 2 is shorter than the announced
            // length but the firmware tolerates it). Behavior unchanged
            // pending hardware verification by a G835L owner.
            split_into_panes(&mut buffers, anime.data.as_slice(), PANE_LEN);
        } else if anime.anime == AnimeType::G635L {
            // G635L: 490 bytes in pane 1, 320 bytes in pane 2 — exact-fit
            // STRIX-class chunking per G Helper's AnimeMatrixDevice.
            split_into_panes(&mut buffers, anime.data.as_slice(), G635L_PANE1_LEN);
        } else {
            for (idx, chunk) in anime.data.as_slice().chunks(PANE_LEN).enumerate() {
                buffers[idx][BLOCK_START..BLOCK_START + chunk.len()].copy_from_slice(chunk);
            }
        }

        for (i, prefix) in usb_prefixes_for(anime.anime).iter().enumerate() {
            buffers[i][..7].copy_from_slice(prefix);
        }
        Ok(buffers)
    }
}

/// This runs the animations as a blocking loop by using the `callback` to write
/// data
///
/// If `callback` is `Ok(true)` then `run_animation` will exit the animation
/// loop early.
pub fn run_animation(frames: &AnimeGif, callback: &dyn Fn(AnimeDataBuffer) -> Result<bool>) {
    let mut count = 0;
    let start = Instant::now();

    let mut timed = false;
    let mut run_time = frames.total_frame_time();
    if let AnimTime::Fade(time) = frames.duration() {
        if let Some(middle) = time.show_for() {
            run_time = middle + time.total_fade_time();
        }
        // add a small buffer
        run_time += Duration::from_millis(250);
        timed = true;
    } else if let AnimTime::Time(time) = frames.duration() {
        run_time = time;
        timed = true;
    }

    // After setting up all the data
    let mut fade_in = Duration::from_millis(0);
    let mut fade_out = Duration::from_millis(0);
    let mut fade_in_step = 0.0;
    let mut fade_in_accum = 0.0;
    let mut fade_out_step = 0.0;
    let mut fade_out_accum;
    if let AnimTime::Fade(time) = frames.duration() {
        fade_in = time.fade_in();
        fade_out = time.fade_out();
        fade_in_step = 1.0 / fade_in.as_secs_f32();
        fade_out_step = 1.0 / fade_out.as_secs_f32();

        if time.total_fade_time() > run_time {
            println!("Total fade in/out time larger than gif run time. Setting fades to half");
            fade_in = run_time / 2;
            fade_in_step = 1.0 / (run_time / 2).as_secs_f32();

            fade_out = run_time / 2;
            fade_out_step = 1.0 / (run_time / 2).as_secs_f32();
        }
    }

    'animation: loop {
        for frame in frames.frames() {
            let frame_start = Instant::now();
            let mut output = frame.frame().clone();

            if let AnimTime::Fade(_) = frames.duration() {
                if frame_start <= start + fade_in {
                    for pixel in output.data_mut() {
                        *pixel = (*pixel as f32 * fade_in_accum) as u8;
                    }
                    fade_in_accum = fade_in_step * (frame_start - start).as_secs_f32();
                } else if frame_start > (start + run_time) - fade_out {
                    if run_time > (frame_start - start) {
                        fade_out_accum =
                            fade_out_step * (run_time - (frame_start - start)).as_secs_f32();
                    } else {
                        fade_out_accum = 0.0;
                    }
                    for pixel in output.data_mut() {
                        *pixel = (*pixel as f32 * fade_out_accum) as u8;
                    }
                }
            }

            // TODO: Log this error
            if matches!(callback(output), Ok(true)) {
                info!("rog-anime: animation frame-loop callback asked to exit early");
                return;
            }

            if timed && Instant::now().duration_since(start) > run_time {
                break 'animation;
            }
            sleep(frame.delay());
        }
        if let AnimTime::Count(times) = frames.duration() {
            count += 1;
            if count >= times {
                break 'animation;
            }
        }
    }
}
