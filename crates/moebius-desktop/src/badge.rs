//! What the app's icon says of the count of unread notices: the label for the window's title, and on Windows, where
//! the taskbar takes no number, the small image drawn over the icon. Drawn here, with no font and no window, so that a
//! test can read it.

/// The side of the overlay image in pixels.
pub const SIDE: u32 = 32;

/// The count as the title and the overlay show it: nothing for none, `9+` above nine.
pub fn label(count: usize) -> String {
    match count {
        0 => String::new(),
        1..=9 => count.to_string(),
        _ => "9+".to_string(),
    }
}

/// The window's title for the count: `(3) Moebius`, or `Moebius` for none.
pub fn title(count: usize) -> String {
    match label(count).as_str() {
        "" => "Moebius".to_string(),
        label => format!("({label}) Moebius"),
    }
}

/// The 3 by 5 pixels of `0` to `9` and `+`, a row to a number, the highest of the three bits to the left.
const GLYPHS: [[u8; 5]; 11] = [
    [0b111, 0b101, 0b101, 0b101, 0b111],
    [0b010, 0b110, 0b010, 0b010, 0b111],
    [0b111, 0b001, 0b111, 0b100, 0b111],
    [0b111, 0b001, 0b111, 0b001, 0b111],
    [0b101, 0b101, 0b111, 0b001, 0b001],
    [0b111, 0b100, 0b111, 0b001, 0b111],
    [0b111, 0b100, 0b111, 0b101, 0b111],
    [0b111, 0b001, 0b010, 0b010, 0b010],
    [0b111, 0b101, 0b111, 0b101, 0b111],
    [0b111, 0b101, 0b111, 0b001, 0b111],
    [0b000, 0b010, 0b111, 0b010, 0b000],
];

/// The overlay for `count` (more than none): white digits on a red disc, as `SIDE` by `SIDE` RGBA pixels.
pub fn overlay(count: usize) -> Vec<u8> {
    let side = SIDE as usize;
    let mut pixels = vec![0u8; side * side * 4];
    let middle = (side as f32 - 1.0) / 2.0;
    for y in 0..side {
        for x in 0..side {
            let (dx, dy) = (x as f32 - middle, y as f32 - middle);
            if dx * dx + dy * dy <= (side as f32 / 2.0).powi(2) {
                put(&mut pixels, x, y, [0xd9, 0x3a, 0x3a, 0xff]);
            }
        }
    }
    let label = label(count);
    let scale = if label.len() == 1 { 4 } else { 3 };
    let width = label.len() * 3 * scale + (label.len() - 1) * scale;
    let (left, top) = ((side - width) / 2, (side - 5 * scale) / 2);
    for (place, character) in label.chars().enumerate() {
        let glyph = &GLYPHS[character.to_digit(10).map_or(10, |digit| digit as usize)];
        for (row, bits) in glyph.iter().enumerate() {
            for column in 0..3 {
                if bits >> (2 - column) & 1 == 1 {
                    for (dx, dy) in (0..scale).flat_map(|x| (0..scale).map(move |y| (x, y))) {
                        let x = left + (place * 4 + column) * scale + dx;
                        put(&mut pixels, x, top + row * scale + dy, [0xff; 4]);
                    }
                }
            }
        }
    }
    pixels
}

fn put(pixels: &mut [u8], x: usize, y: usize, color: [u8; 4]) {
    let at = (y * SIDE as usize + x) * 4;
    pixels[at..at + 4].copy_from_slice(&color);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_label_is_the_count_up_to_nine() {
        assert_eq!(label(0), "");
        assert_eq!(label(3), "3");
        assert_eq!(label(9), "9");
        assert_eq!(label(10), "9+");
        assert_eq!(title(0), "Moebius");
        assert_eq!(title(12), "(9+) Moebius");
    }

    #[test]
    fn the_overlay_is_a_red_disc_with_white_digits_and_clear_corners() {
        let pixels = overlay(3);
        assert_eq!(pixels.len(), (SIDE * SIDE * 4) as usize);
        let at = |x: usize, y: usize| &pixels[(y * SIDE as usize + x) * 4..][..4];
        assert_eq!(at(0, 0), [0, 0, 0, 0]);
        assert_eq!(at(16, 2), [0xd9, 0x3a, 0x3a, 0xff]);
        assert!(
            pixels.chunks(4).any(|pixel| pixel == [0xff; 4]),
            "no digit is drawn"
        );
    }

    #[test]
    fn every_count_draws_inside_the_image() {
        for count in [1, 5, 9, 10, 99] {
            assert_eq!(overlay(count).len(), (SIDE * SIDE * 4) as usize);
        }
    }

    #[test]
    fn a_different_count_is_a_different_picture() {
        assert_ne!(overlay(1), overlay(2));
        assert_ne!(overlay(9), overlay(10));
    }
}
