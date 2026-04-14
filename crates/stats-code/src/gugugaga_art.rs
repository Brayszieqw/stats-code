use image::imageops::FilterType;
use std::io::{self, Write};
use icy_sixel::{sixel_string, PixelFormat, DiffusionMethod, MethodForLargest, MethodForRep, Quality};

/// 原图字节（编译时嵌入）—— 已裁切去除顶部白色留白
const GUGUGAGA_BYTES: &[u8] = include_bytes!("../assets/gugugaga.jpg");

/// Sixel 图片目标高度（像素）
/// 300px ≈ 终端 15-18 行
const SIXEL_TARGET_HEIGHT: u32 = 300;


/// 图片在终端中的近似字符宽度（列数）
/// 300 * 0.875 = 262px，262 / 8 ≈ 33 列，+2 安全边距
pub const SIXEL_COLS: usize = 35;

/// 在终端通过 Sixel 协议渲染高清企鹅图片（已裁切，无顶部白边）
pub fn print_gugugaga_image() {
    let img = match image::load_from_memory(GUGUGAGA_BYTES) {
        Ok(img) => img,
        Err(e) => {
            eprintln!("  [gugugaga] 图片加载失败: {e}");
            return;
        }
    };

    // 等比缩放到目标高度
    let target_height: u32 = SIXEL_TARGET_HEIGHT;
    let aspect = img.width() as f32 / img.height() as f32;
    let target_width = (target_height as f32 * aspect) as u32;
    let resized = img.resize_exact(target_width, target_height, FilterType::Lanczos3);

    // 转为 RGB888 字节数组
    let rgb = resized.to_rgb8();
    let raw_bytes = rgb.as_raw();

    // 编码为 Sixel 序列
    match sixel_string(
        raw_bytes,
        target_width as i32,
        target_height as i32,
        PixelFormat::RGB888,
        DiffusionMethod::Stucki,
        MethodForLargest::Auto,
        MethodForRep::Auto,
        Quality::HIGH,
    ) {
        Ok(sixel_data) => {
            let mut out = io::stdout();
            let _ = write!(out, "{sixel_data}");
            let _ = out.flush();
        }
        Err(e) => {
            eprintln!("  [gugugaga] Sixel 编码失败: {e}");
        }
    }
}
