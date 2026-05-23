//! Tests for G635L USB packet construction.
//!
//! The expected packet bytes here derive from G Helper's
//! AnimeMatrixDevice.Present() for STRIX class:
//!   https://github.com/seerge/g-helper/blob/main/app/AnimeMatrix/AnimeMatrixDevice.cs
//! G Helper uses UpdatePageLength=490 and LedCount=810 for STRIX, producing
//! two data packets:
//!   Packet 1: header 5E C0 02 01 00 EA 01 (start=1, length=490)
//!   Packet 2: header 5E C0 02 EB 01 40 01 (start=491, length=320)
//!
//! G635L shares STRIX-class LED ordering with G835L (G Helper treats them
//! as the same enum AnimeType.STRIX), so we reuse the existing
//! tests/data/g835l-diagonal.gif input file. Only the USB packet
//! chunking differs between G635L and G835L in current asusctl.

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use rog_anime::*;

    fn load_g635l_first_frame() -> AnimeDataBuffer {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("tests/data/g835l-diagonal.gif");

        let gif =
            AnimeGif::from_diagonal_gif(&path, AnimTime::Count(1), 1.0, AnimeType::G635L).unwrap();
        gif.frames()[0].frame().clone()
    }

    #[test]
    fn g635l_pane_lengths_match_data_length() {
        // If AnimeType::G635L.data_length() ever changes, the chunking
        // constants must change too — this test catches silent drift.
        assert_eq!(
            G635L_PANE1_LEN + G635L_PANE2_LEN,
            AnimeType::G635L.data_length(),
            "G635L pane lengths must sum to the device's data_length"
        );
    }

    #[test]
    fn g635l_packet_count_is_two() {
        let buf = load_g635l_first_frame();
        let pkts = AnimePacketType::try_from(buf).unwrap();
        assert_eq!(
            pkts.len(),
            2,
            "G635L should produce exactly 2 USB packets (810 LEDs split 490+320)"
        );
    }

    #[test]
    fn g635l_packet1_header_matches_g_helper() {
        let buf = load_g635l_first_frame();
        let pkts = AnimePacketType::try_from(buf).unwrap();
        assert_eq!(
            pkts[0][..7],
            USB_PREFIX_G635L_1,
            "packet 1 header should be 5E C0 02 01 00 EA 01 (start=1, length=490)"
        );
    }

    #[test]
    fn g635l_packet2_header_matches_g_helper() {
        let buf = load_g635l_first_frame();
        let pkts = AnimePacketType::try_from(buf).unwrap();
        assert_eq!(
            pkts[1][..7],
            USB_PREFIX_G635L_2,
            "packet 2 header should be 5E C0 02 EB 01 40 01 (start=491, length=320)"
        );
    }

    #[test]
    fn g635l_packet1_carries_first_490_led_bytes() {
        let buf = load_g635l_first_frame();
        // Save a copy of the LED data BEFORE try_from consumes the buffer.
        let led_data = buf.data().to_vec();
        assert_eq!(led_data.len(), 810, "G635L data buffer should be 810 bytes");

        let pkts = AnimePacketType::try_from(buf).unwrap();

        // Packet 1: bytes [7..7+490] should equal led_data[0..490]
        assert_eq!(
            &pkts[0][7..7 + G635L_PANE1_LEN],
            &led_data[..G635L_PANE1_LEN],
            "packet 1 LED data should be led_data[0..490]"
        );
    }

    #[test]
    fn g635l_packet2_carries_remaining_320_led_bytes() {
        let buf = load_g635l_first_frame();
        let led_data = buf.data().to_vec();

        let pkts = AnimePacketType::try_from(buf).unwrap();

        // Packet 2: bytes [7..7+320] should equal led_data[490..810]
        assert_eq!(
            &pkts[1][7..7 + G635L_PANE2_LEN],
            &led_data[G635L_PANE1_LEN..],
            "packet 2 LED data should be led_data[490..810]"
        );
    }

    #[test]
    fn g635l_packet1_padding_after_led_data_is_zero() {
        let buf = load_g635l_first_frame();
        let pkts = AnimePacketType::try_from(buf).unwrap();

        // Bytes [7+490..640] should all be zero.
        for (i, b) in pkts[0][7 + G635L_PANE1_LEN..].iter().enumerate() {
            assert_eq!(
                *b,
                0,
                "packet 1 padding byte at offset {} should be zero, got {:#04x}",
                7 + G635L_PANE1_LEN + i,
                b
            );
        }
    }

    #[test]
    fn g635l_packet2_padding_after_led_data_is_zero() {
        let buf = load_g635l_first_frame();
        let pkts = AnimePacketType::try_from(buf).unwrap();

        // Bytes [7+320..640] should all be zero.
        for (i, b) in pkts[1][7 + G635L_PANE2_LEN..].iter().enumerate() {
            assert_eq!(
                *b,
                0,
                "packet 2 padding byte at offset {} should be zero, got {:#04x}",
                7 + G635L_PANE2_LEN + i,
                b
            );
        }
    }
}
