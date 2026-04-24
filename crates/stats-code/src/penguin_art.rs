#![allow(dead_code)]
// Legacy ANSI-color penguin banner — superseded by Sixel renderer (gugugaga_art.rs).
// Kept for fallback on terminals without Sixel support.

use colored::Colorize;
use std::io::Write;

// Auto-generated penguin banner art
// Image: 32x34 pixels -> 32x17 character cells
pub fn write_penguin_art(out: &mut impl Write) -> Result<(), String> {
    fn eio(err: std::io::Error) -> String {
        err.to_string()
    }

    // Row 0
    write!(out, "                ").map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(251, 243, 221)
            .on_truecolor(249, 220, 159)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(250, 246, 231)
            .on_truecolor(250, 239, 212)
    )
    .map_err(eio)?;
    write!(out, "              ").map_err(eio)?;
    writeln!(out).map_err(eio)?;
    // Row 1
    write!(out, "     ").map_err(eio)?;
    write!(out, "{}", "\u{2584}".truecolor(250, 230, 184)).map_err(eio)?;
    write!(out, " ").map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(250, 245, 233)
            .on_truecolor(250, 237, 207)
    )
    .map_err(eio)?;
    write!(out, "{}", "\u{2584}".truecolor(250, 226, 173)).map_err(eio)?;
    write!(out, "       ").map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(250, 217, 149)
            .on_truecolor(248, 242, 224)
    )
    .map_err(eio)?;
    write!(out, "{}", "\u{2580}".truecolor(251, 247, 236)).map_err(eio)?;
    write!(out, "   ").map_err(eio)?;
    write!(out, "{}", "\u{2584}".truecolor(252, 247, 231)).map_err(eio)?;
    write!(out, "{}", "\u{2584}".truecolor(252, 246, 232)).map_err(eio)?;
    write!(out, "         ").map_err(eio)?;
    writeln!(out).map_err(eio)?;
    // Row 2
    write!(out, "     ").map_err(eio)?;
    write!(out, "{}", "\u{2580}".truecolor(252, 223, 166)).map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(251, 219, 154)
            .on_truecolor(250, 217, 145)
    )
    .map_err(eio)?;
    write!(out, "{}", "\u{2584}".truecolor(250, 231, 190)).map_err(eio)?;
    write!(out, "{}", "\u{2580}".truecolor(250, 241, 216)).map_err(eio)?;
    write!(out, "  ").map_err(eio)?;
    write!(out, "{}", "\u{2584}".truecolor(198, 198, 199)).map_err(eio)?;
    write!(out, "{}", "\u{2584}".truecolor(143, 145, 147)).map_err(eio)?;
    write!(out, "{}", "\u{2584}".truecolor(119, 124, 136)).map_err(eio)?;
    write!(out, "{}", "\u{2584}".truecolor(128, 136, 148)).map_err(eio)?;
    write!(out, "{}", "\u{2584}".truecolor(144, 149, 152)).map_err(eio)?;
    write!(out, "{}", "\u{2584}".truecolor(179, 180, 183)).map_err(eio)?;
    write!(out, "    ").map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(249, 219, 153)
            .on_truecolor(251, 240, 215)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(251, 241, 218)
            .on_truecolor(250, 233, 196)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(251, 238, 210)
            .on_truecolor(251, 222, 160)
    )
    .map_err(eio)?;
    write!(out, "{}", "\u{2580}".truecolor(251, 248, 239)).map_err(eio)?;
    write!(out, "       ").map_err(eio)?;
    writeln!(out).map_err(eio)?;
    // Row 3
    write!(out, "       ").map_err(eio)?;
    write!(out, "{}", "\u{2580}".truecolor(249, 242, 224)).map_err(eio)?;
    write!(out, "{}", "\u{2584}".truecolor(225, 225, 224)).map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(232, 232, 232)
            .on_truecolor(91, 93, 100)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(126, 127, 130)
            .on_truecolor(96, 99, 110)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(100, 101, 105)
            .on_truecolor(139, 140, 143)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(86, 94, 124).on_truecolor(151, 151, 73)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(87, 96, 104).on_truecolor(232, 225, 31)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(90, 99, 105).on_truecolor(241, 235, 26)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(83, 91, 121).on_truecolor(165, 161, 63)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(97, 100, 105)
            .on_truecolor(133, 134, 137)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(114, 118, 127)
            .on_truecolor(96, 104, 119)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(209, 209, 211).on_truecolor(79, 85, 95)
    )
    .map_err(eio)?;
    write!(out, "{}", "\u{2584}".truecolor(200, 200, 201)).map_err(eio)?;
    write!(out, "  ").map_err(eio)?;
    write!(out, "{}", "\u{2580}".truecolor(251, 244, 231)).map_err(eio)?;
    write!(out, "         ").map_err(eio)?;
    writeln!(out).map_err(eio)?;
    // Row 4
    write!(out, " ").map_err(eio)?;
    write!(out, "{}", "\u{2584}".truecolor(251, 242, 219)).map_err(eio)?;
    write!(out, "{}", "\u{2584}".truecolor(250, 221, 158)).map_err(eio)?;
    write!(out, "{}", "\u{2584}".truecolor(251, 245, 228)).map_err(eio)?;
    write!(out, "   ").map_err(eio)?;
    write!(out, "{}", "\u{2584}".truecolor(213, 215, 216)).map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(119, 122, 128)
            .on_truecolor(89, 95, 105)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(77, 81, 94).on_truecolor(89, 92, 103)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(100, 104, 121).on_truecolor(47, 50, 55)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(74, 75, 65).on_truecolor(38, 39, 43)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(170, 161, 22).on_truecolor(59, 62, 75)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(222, 208, 24).on_truecolor(70, 70, 70)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(217, 207, 23).on_truecolor(75, 76, 79)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(155, 144, 24).on_truecolor(70, 72, 90)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(54, 54, 50).on_truecolor(72, 73, 76)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(87, 93, 108).on_truecolor(48, 51, 57)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(88, 94, 108).on_truecolor(84, 87, 99)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(90, 95, 107).on_truecolor(93, 99, 114)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(230, 231, 229)
            .on_truecolor(170, 176, 181)
    )
    .map_err(eio)?;
    write!(out, "   ").map_err(eio)?;
    write!(out, "{}", "\u{2584}".truecolor(250, 240, 217)).map_err(eio)?;
    write!(out, "{}", "\u{2584}".truecolor(250, 235, 201)).map_err(eio)?;
    write!(out, "      ").map_err(eio)?;
    writeln!(out).map_err(eio)?;
    // Row 5
    write!(out, "  ").map_err(eio)?;
    write!(out, "{}", "\u{2580}".truecolor(251, 222, 164)).map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(252, 208, 124)
            .on_truecolor(250, 232, 192)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(250, 238, 212)
            .on_truecolor(249, 232, 195)
    )
    .map_err(eio)?;
    write!(out, " ").map_err(eio)?;
    write!(out, "{}", "\u{2584}".truecolor(234, 234, 233)).map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(149, 153, 159).on_truecolor(70, 72, 82)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(75, 85, 90).on_truecolor(71, 84, 88)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(45, 51, 55).on_truecolor(53, 65, 67)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(79, 75, 74).on_truecolor(199, 188, 178)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(113, 112, 111)
            .on_truecolor(122, 115, 111)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(78, 81, 82).on_truecolor(44, 40, 43)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(76, 80, 84).on_truecolor(64, 56, 56)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(81, 86, 91).on_truecolor(63, 53, 54)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(88, 93, 93).on_truecolor(61, 53, 55)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(95, 99, 102).on_truecolor(62, 58, 58)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(75, 77, 81).on_truecolor(67, 68, 69)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(50, 53, 59).on_truecolor(62, 64, 65)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(84, 88, 100).on_truecolor(54, 56, 62)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(120, 128, 138)
            .on_truecolor(80, 87, 100)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(237, 238, 236)
            .on_truecolor(192, 196, 196)
    )
    .map_err(eio)?;
    write!(out, "    ").map_err(eio)?;
    write!(out, "{}", "\u{2584}".truecolor(249, 218, 152)).map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(249, 236, 203)
            .on_truecolor(251, 222, 157)
    )
    .map_err(eio)?;
    write!(out, "{}", "\u{2580}".truecolor(251, 248, 240)).map_err(eio)?;
    write!(out, "   ").map_err(eio)?;
    writeln!(out).map_err(eio)?;
    // Row 6
    write!(out, "{}", "\u{2584}".truecolor(251, 240, 211)).map_err(eio)?;
    write!(out, "{}", "\u{2584}".truecolor(252, 244, 222)).map_err(eio)?;
    write!(out, "    ").map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(186, 188, 189)
            .on_truecolor(142, 144, 147)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(31, 34, 40).on_truecolor(30, 30, 37)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(56, 60, 60).on_truecolor(55, 56, 58)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(77, 78, 77).on_truecolor(93, 79, 79)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(220, 200, 188)
            .on_truecolor(134, 99, 99)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(174, 140, 131).on_truecolor(68, 51, 63)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(140, 99, 92)
            .on_truecolor(151, 131, 128)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(152, 115, 105)
            .on_truecolor(254, 230, 214)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(156, 118, 108)
            .on_truecolor(253, 229, 215)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(151, 110, 100)
            .on_truecolor(173, 148, 142)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(128, 85, 77).on_truecolor(58, 48, 64)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(105, 72, 68).on_truecolor(104, 80, 87)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(81, 74, 73).on_truecolor(84, 66, 66)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(60, 62, 63).on_truecolor(62, 63, 66)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(47, 51, 60).on_truecolor(46, 49, 51)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(134, 138, 143).on_truecolor(89, 93, 97)
    )
    .map_err(eio)?;
    write!(out, "    ").map_err(eio)?;
    write!(out, "{}", "\u{2580}".truecolor(252, 247, 234)).map_err(eio)?;
    write!(out, "{}", "\u{2584}".truecolor(251, 242, 223)).map_err(eio)?;
    write!(out, "    ").map_err(eio)?;
    writeln!(out).map_err(eio)?;
    // Row 7
    write!(out, "{}", "\u{2580}".truecolor(250, 226, 172)).map_err(eio)?;
    write!(out, "{}", "\u{2580}".truecolor(248, 213, 138)).map_err(eio)?;
    write!(out, "{}", "\u{2580}".truecolor(249, 233, 196)).map_err(eio)?;
    write!(out, "  ").map_err(eio)?;
    write!(out, "{}", "\u{2584}".truecolor(225, 225, 223)).map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(102, 102, 104).on_truecolor(55, 55, 57)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(40, 40, 44).on_truecolor(55, 55, 57)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(58, 59, 64).on_truecolor(55, 57, 60)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(94, 76, 76).on_truecolor(95, 80, 78)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(181, 181, 181)
            .on_truecolor(215, 186, 178)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(97, 121, 131)
            .on_truecolor(225, 192, 176)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(147, 153, 155)
            .on_truecolor(241, 216, 201)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(255, 239, 224)
            .on_truecolor(246, 231, 217)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(255, 236, 222)
            .on_truecolor(244, 227, 214)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(196, 194, 190)
            .on_truecolor(243, 224, 211)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(103, 122, 130)
            .on_truecolor(233, 195, 178)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(172, 179, 186)
            .on_truecolor(233, 195, 187)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(122, 100, 98).on_truecolor(122, 99, 94)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(50, 49, 52).on_truecolor(55, 53, 54)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(55, 56, 57).on_truecolor(56, 57, 60)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(62, 65, 64).on_truecolor(73, 75, 77)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(164, 167, 170)
            .on_truecolor(94, 99, 110)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(115, 118, 124).on_truecolor(79, 84, 97)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(130, 130, 134)
            .on_truecolor(100, 104, 111)
    )
    .map_err(eio)?;
    write!(out, " ").map_err(eio)?;
    write!(out, "{}", "\u{2580}".truecolor(249, 232, 196)).map_err(eio)?;
    write!(out, "{}", "\u{2580}".truecolor(250, 213, 135)).map_err(eio)?;
    write!(out, "{}", "\u{2580}".truecolor(251, 241, 222)).map_err(eio)?;
    write!(out, "   ").map_err(eio)?;
    writeln!(out).map_err(eio)?;
    // Row 8
    write!(out, "   ").map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(224, 224, 224)
            .on_truecolor(105, 107, 111)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(218, 218, 219)
            .on_truecolor(87, 90, 101)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(192, 193, 193)
            .on_truecolor(89, 94, 104)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(54, 56, 61).on_truecolor(51, 55, 59)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(57, 58, 61).on_truecolor(46, 49, 52)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(54, 55, 58).on_truecolor(38, 39, 41)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(75, 63, 63).on_truecolor(41, 40, 42)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(213, 168, 158)
            .on_truecolor(135, 114, 110)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(255, 214, 201)
            .on_truecolor(251, 236, 220)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(244, 217, 202)
            .on_truecolor(246, 229, 213)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(209, 135, 133)
            .on_truecolor(231, 166, 160)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(204, 120, 122)
            .on_truecolor(244, 159, 159)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(232, 197, 188)
            .on_truecolor(245, 221, 209)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(255, 227, 214)
            .on_truecolor(253, 241, 225)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(255, 217, 203)
            .on_truecolor(163, 143, 135)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(119, 98, 95).on_truecolor(48, 42, 44)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(43, 44, 46).on_truecolor(36, 38, 39)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(59, 65, 65).on_truecolor(45, 50, 51)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(68, 71, 76).on_truecolor(71, 73, 81)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(91, 94, 106).on_truecolor(96, 101, 112)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(82, 87, 99).on_truecolor(73, 78, 89)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(120, 122, 128)
            .on_truecolor(167, 167, 170)
    )
    .map_err(eio)?;
    write!(out, " ").map_err(eio)?;
    write!(out, "{}", "\u{2584}".truecolor(177, 177, 177)).map_err(eio)?;
    write!(out, "{}", "\u{2584}".truecolor(201, 201, 201)).map_err(eio)?;
    write!(out, "    ").map_err(eio)?;
    writeln!(out).map_err(eio)?;
    // Row 9
    write!(out, "   ").map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(130, 132, 135)
            .on_truecolor(211, 211, 209)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(79, 84, 96).on_truecolor(66, 69, 78)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(85, 90, 100).on_truecolor(94, 100, 112)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(75, 79, 87).on_truecolor(88, 94, 106)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(71, 74, 83).on_truecolor(90, 95, 108)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(47, 48, 54).on_truecolor(92, 96, 110)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(35, 35, 36).on_truecolor(60, 62, 70)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(65, 65, 68).on_truecolor(39, 42, 45)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(149, 116, 116).on_truecolor(73, 59, 65)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(184, 169, 166).on_truecolor(63, 58, 60)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(156, 161, 162)
            .on_truecolor(99, 102, 105)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(152, 152, 149)
            .on_truecolor(102, 100, 105)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(176, 172, 168)
            .on_truecolor(114, 114, 116)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(157, 147, 144).on_truecolor(66, 66, 67)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(99, 94, 100).on_truecolor(44, 44, 48)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(36, 35, 40).on_truecolor(60, 62, 72)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(45, 47, 51).on_truecolor(80, 84, 95)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(69, 71, 80).on_truecolor(98, 102, 115)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(92, 94, 106).on_truecolor(92, 96, 108)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(92, 96, 109).on_truecolor(80, 84, 98)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(82, 85, 94).on_truecolor(130, 132, 136)
    )
    .map_err(eio)?;
    write!(out, "{}", "\u{2580}".truecolor(225, 225, 225)).map_err(eio)?;
    write!(out, "{}", "\u{2584}".truecolor(234, 234, 235)).map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(177, 177, 177)
            .on_truecolor(200, 200, 200)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(195, 195, 194)
            .on_truecolor(221, 221, 221)
    )
    .map_err(eio)?;
    write!(out, "    ").map_err(eio)?;
    writeln!(out).map_err(eio)?;
    // Row 10
    write!(out, "    ").map_err(eio)?;
    write!(out, "{}", "\u{2580}".truecolor(143, 144, 147)).map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(61, 66, 79).on_truecolor(112, 113, 119)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(92, 98, 110).on_truecolor(61, 65, 77)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(76, 82, 93).on_truecolor(87, 92, 104)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(83, 87, 99).on_truecolor(79, 82, 93)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(85, 87, 100).on_truecolor(81, 84, 96)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(83, 87, 98).on_truecolor(134, 137, 145)
    )
    .map_err(eio)?;
    write!(out, "{}", "\u{2580}".truecolor(191, 195, 197)).map_err(eio)?;
    write!(out, "{}", "\u{2580}".truecolor(214, 214, 215)).map_err(eio)?;
    write!(out, "{}", "\u{2580}".truecolor(163, 162, 165)).map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(134, 134, 139)
            .on_truecolor(204, 204, 204)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(112, 110, 115)
            .on_truecolor(178, 177, 180)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(145, 143, 144)
            .on_truecolor(136, 134, 139)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(138, 137, 142)
            .on_truecolor(212, 212, 213)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(91, 94, 108)
            .on_truecolor(103, 107, 118)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(80, 84, 95).on_truecolor(85, 89, 101)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(86, 90, 101).on_truecolor(78, 82, 94)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(80, 84, 96).on_truecolor(59, 63, 76)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(77, 81, 92).on_truecolor(167, 169, 172)
    )
    .map_err(eio)?;
    write!(out, "{}", "\u{2580}".truecolor(218, 218, 219)).map_err(eio)?;
    write!(out, " ").map_err(eio)?;
    write!(out, "{}", "\u{2580}".truecolor(219, 219, 220)).map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(157, 157, 157)
            .on_truecolor(181, 181, 181)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(209, 209, 209)
            .on_truecolor(224, 224, 224)
    )
    .map_err(eio)?;
    write!(out, "    ").map_err(eio)?;
    writeln!(out).map_err(eio)?;
    // Row 11
    write!(out, "      ").map_err(eio)?;
    write!(out, "{}", "\u{2580}".truecolor(140, 141, 144)).map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(54, 58, 67).on_truecolor(163, 163, 165)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(81, 84, 95).on_truecolor(72, 76, 86)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(81, 85, 97).on_truecolor(87, 92, 104)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(181, 182, 188)
            .on_truecolor(213, 213, 216)
    )
    .map_err(eio)?;
    write!(out, "     ").map_err(eio)?;
    write!(out, "{}", "\u{2580}".truecolor(235, 233, 234)).map_err(eio)?;
    write!(out, " ").map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(135, 138, 145)
            .on_truecolor(170, 170, 174)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(87, 91, 104).on_truecolor(76, 81, 94)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(61, 66, 76).on_truecolor(94, 97, 105)
    )
    .map_err(eio)?;
    write!(out, "{}", "\u{2580}".truecolor(130, 134, 138)).map_err(eio)?;
    write!(out, "   ").map_err(eio)?;
    write!(out, "{}", "\u{2580}".truecolor(196, 196, 196)).map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(101, 101, 101)
            .on_truecolor(169, 169, 169)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(219, 219, 219)
            .on_truecolor(233, 233, 233)
    )
    .map_err(eio)?;
    write!(out, "    ").map_err(eio)?;
    writeln!(out).map_err(eio)?;
    // Row 12
    write!(out, "       ").map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(215, 215, 216)
            .on_truecolor(200, 200, 201)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(75, 78, 87).on_truecolor(70, 75, 83)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(93, 97, 108).on_truecolor(97, 101, 112)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(228, 228, 229)
            .on_truecolor(233, 233, 233)
    )
    .map_err(eio)?;
    write!(out, "       ").map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(191, 190, 193)
            .on_truecolor(194, 194, 196)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(73, 77, 91).on_truecolor(73, 78, 91)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(101, 104, 110)
            .on_truecolor(93, 95, 103)
    )
    .map_err(eio)?;
    write!(out, "    ").map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(166, 166, 166)
            .on_truecolor(238, 238, 236)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(108, 108, 108)
            .on_truecolor(179, 179, 177)
    )
    .map_err(eio)?;
    write!(out, "     ").map_err(eio)?;
    writeln!(out).map_err(eio)?;
    // Row 13
    write!(out, "       ").map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(184, 185, 185)
            .on_truecolor(191, 191, 190)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(66, 73, 84).on_truecolor(63, 69, 79)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(94, 99, 109).on_truecolor(84, 89, 101)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(228, 227, 229)
            .on_truecolor(206, 205, 209)
    )
    .map_err(eio)?;
    write!(out, "       ").map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(187, 187, 189)
            .on_truecolor(145, 145, 151)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(76, 80, 94).on_truecolor(67, 71, 84)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(84, 86, 94).on_truecolor(43, 46, 55)
    )
    .map_err(eio)?;
    write!(out, "{}", "\u{2584}".truecolor(168, 169, 169)).map_err(eio)?;
    write!(out, "   ").map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(232, 232, 230)
            .on_truecolor(234, 235, 232)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(238, 238, 237)
            .on_truecolor(239, 240, 237)
    )
    .map_err(eio)?;
    write!(out, "     ").map_err(eio)?;
    writeln!(out).map_err(eio)?;
    // Row 14
    write!(out, "       ").map_err(eio)?;
    write!(out, "{}", "\u{2580}".truecolor(223, 223, 221)).map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(53, 57, 64).on_truecolor(75, 78, 84)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(45, 50, 63).on_truecolor(42, 46, 59)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(107, 109, 117).on_truecolor(41, 45, 56)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(225, 221, 226)
            .on_truecolor(132, 131, 142)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(214, 207, 215)
            .on_truecolor(209, 201, 212)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(213, 206, 214)
            .on_truecolor(209, 200, 213)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(213, 206, 214)
            .on_truecolor(209, 201, 217)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(209, 202, 211)
            .on_truecolor(210, 201, 217)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(218, 210, 219)
            .on_truecolor(191, 182, 195)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(192, 186, 194).on_truecolor(82, 82, 91)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(55, 58, 68).on_truecolor(42, 46, 58)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(46, 51, 62).on_truecolor(50, 54, 66)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(37, 40, 50).on_truecolor(40, 42, 53)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(104, 113, 125).on_truecolor(54, 59, 73)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(201, 205, 206)
            .on_truecolor(95, 106, 118)
    )
    .map_err(eio)?;
    write!(out, "{}", "\u{2584}".truecolor(164, 169, 174)).map_err(eio)?;
    write!(out, "{}", "\u{2584}".truecolor(235, 235, 235)).map_err(eio)?;
    write!(out, "       ").map_err(eio)?;
    writeln!(out).map_err(eio)?;
    // Row 15
    write!(out, "        ").map_err(eio)?;
    write!(out, "{}", "\u{2580}".truecolor(160, 161, 161)).map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(24, 28, 39).on_truecolor(133, 135, 141)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(51, 55, 68).on_truecolor(41, 44, 54)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(44, 48, 60).on_truecolor(45, 50, 63)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(89, 89, 101).on_truecolor(35, 39, 50)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(122, 120, 132).on_truecolor(39, 44, 53)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(121, 119, 132).on_truecolor(45, 50, 61)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(101, 99, 113).on_truecolor(46, 50, 58)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(57, 61, 76).on_truecolor(51, 49, 50)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(38, 44, 59).on_truecolor(60, 57, 55)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(49, 53, 67).on_truecolor(44, 44, 48)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(37, 40, 51).on_truecolor(45, 50, 61)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(46, 49, 62).on_truecolor(45, 49, 59)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(48, 51, 64).on_truecolor(41, 44, 56)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(43, 46, 59).on_truecolor(41, 45, 56)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(49, 56, 69).on_truecolor(40, 44, 55)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(82, 89, 98).on_truecolor(38, 42, 54)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(128, 133, 138).on_truecolor(42, 48, 58)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}".truecolor(147, 148, 152).on_truecolor(75, 77, 85)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(166, 167, 168)
            .on_truecolor(155, 157, 160)
    )
    .map_err(eio)?;
    write!(out, "{}", "\u{2580}".truecolor(240, 242, 241)).map_err(eio)?;
    write!(out, "   ").map_err(eio)?;
    writeln!(out).map_err(eio)?;
    // Row 16
    write!(out, "     ").map_err(eio)?;
    write!(out, "{}", "\u{2584}".truecolor(230, 230, 232)).map_err(eio)?;
    write!(out, "{}", "\u{2584}".truecolor(197, 195, 203)).map_err(eio)?;
    write!(out, "{}", "\u{2584}".truecolor(178, 176, 185)).map_err(eio)?;
    write!(out, "{}", "\u{2584}".truecolor(168, 167, 179)).map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(193, 180, 152)
            .on_truecolor(128, 119, 106)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(164, 124, 28)
            .on_truecolor(150, 132, 84)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(172, 132, 35)
            .on_truecolor(159, 132, 68)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(140, 108, 36)
            .on_truecolor(150, 133, 103)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(163, 162, 169)
            .on_truecolor(159, 158, 180)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(196, 195, 202)
            .on_truecolor(161, 158, 172)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(168, 167, 172)
            .on_truecolor(156, 155, 176)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(155, 120, 37)
            .on_truecolor(139, 128, 106)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(198, 148, 35)
            .on_truecolor(154, 132, 76)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(162, 125, 37)
            .on_truecolor(144, 127, 91)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(172, 168, 157)
            .on_truecolor(146, 138, 135)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(188, 188, 193)
            .on_truecolor(172, 170, 189)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(179, 178, 182)
            .on_truecolor(173, 169, 185)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(183, 181, 186)
            .on_truecolor(178, 174, 189)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(191, 190, 195)
            .on_truecolor(188, 184, 198)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(200, 198, 201)
            .on_truecolor(204, 200, 210)
    )
    .map_err(eio)?;
    write!(
        out,
        "{}",
        "\u{2580}"
            .truecolor(221, 220, 220)
            .on_truecolor(217, 214, 221)
    )
    .map_err(eio)?;
    write!(out, "{}", "\u{2584}".truecolor(230, 229, 231)).map_err(eio)?;
    write!(out, "     ").map_err(eio)?;
    writeln!(out).map_err(eio)?;

    Ok(())
}

/// Returns each row of the penguin art as an ANSI-colored string (no trailing newline).
/// Used for side-by-side two-column layout.
pub fn penguin_art_rows() -> Vec<String> {
    let mut buf: Vec<u8> = Vec::new();
    let _ = write_penguin_art(&mut buf);
    let text = String::from_utf8_lossy(&buf).to_string();
    text.lines().map(std::string::ToString::to_string).collect()
}
