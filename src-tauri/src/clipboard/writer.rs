use std::{fs, io::Cursor, path::Path};

use anyhow::Context;
use clipboard_win::{formats as cf, options::NoClear, raw};
use image::{DynamicImage, ImageFormat};

use super::formats::{formats, open_with_retry};
use crate::store::{ClipContent, ClipKind};

/// BMP 文件头长度；CF_DIB 是去掉文件头后的部分。
const BMP_FILE_HEADER: usize = 14;

enum Payload {
    Text {
        text: String,
        html: Option<String>,
        rtf: Option<String>,
    },
    Files(Vec<String>),
    Image {
        png: Vec<u8>,
        dib: Vec<u8>,
    },
}

/// 把条目写回剪贴板，并附带 AnyPaste 标记让监听器跳过这次变化。
/// `plain` 为 true 时只写纯文本：富文本去掉格式，文件列表写为路径文本。
pub fn write(content: ClipContent, plain: bool, images_dir: &Path) -> anyhow::Result<()> {
    // 图片编解码在打开剪贴板之前完成，尽量缩短占用时间
    let payload = payload(content, plain, images_dir)?;
    let f = formats();
    let _guard = open_with_retry()?;
    raw::empty().context("清空剪贴板失败")?;
    // 标记最先写入：后续某个格式写入失败时，监听器也不会把这份残缺内容当作新条目记录
    if let Some(own) = f.own {
        raw::set_without_clear(own.get(), &[1]).context("写入 AnyPaste 标记失败")?;
    }

    match &payload {
        Payload::Text { text, html, rtf } => {
            raw::set_string_with(text, NoClear).context("写入文本失败")?;
            if let (Some(html), Some(fmt)) = (html, f.html) {
                raw::set_html_with(fmt.get(), html, NoClear).context("写入 HTML 失败")?;
            }
            if let (Some(rtf), Some(fmt)) = (rtf, f.rtf) {
                let mut bytes = rtf.as_bytes().to_vec();
                bytes.push(0);
                raw::set_without_clear(fmt.get(), &bytes).context("写入 RTF 失败")?;
            }
        }
        Payload::Files(paths) => {
            raw::set_file_list_with(paths, NoClear).context("写入文件列表失败")?;
        }
        Payload::Image { png, dib } => {
            raw::set_without_clear(cf::CF_DIB, dib).context("写入位图失败")?;
            // 支持 PNG 的程序（浏览器、Office 等）可保留透明度
            if let Some(fmt) = f.png {
                raw::set_without_clear(fmt.get(), png).context("写入 PNG 失败")?;
            }
        }
    }
    Ok(())
}

fn payload(content: ClipContent, plain: bool, images_dir: &Path) -> anyhow::Result<Payload> {
    let text = content.text.unwrap_or_default();
    Ok(match content.kind {
        ClipKind::Files if !plain => {
            let paths: Vec<String> = text.lines().filter(|l| !l.is_empty()).map(String::from).collect();
            // 微信等程序复制的图片文件在临时目录被清理后原路径失效，回退使用采集时保存的 PNG 副本按图片粘贴
            if paths.iter().any(|p| !Path::new(p).exists()) {
                if let Some(file) = content.image_file.as_deref().filter(|f| images_dir.join(f).is_file()) {
                    return image_payload(file, images_dir);
                }
            }
            Payload::Files(paths)
        }
        ClipKind::Text | ClipKind::Rich | ClipKind::Files => {
            if plain {
                Payload::Text { text, html: None, rtf: None }
            } else {
                Payload::Text { text, html: content.html, rtf: content.rtf }
            }
        }
        ClipKind::Image => {
            let file = content.image_file.context("图片条目缺少文件")?;
            return image_payload(&file, images_dir);
        }
    })
}

fn image_payload(file: &str, images_dir: &Path) -> anyhow::Result<Payload> {
    let path = images_dir.join(file);
    let png = fs::read(&path).with_context(|| format!("读取图片失败 {}", path.display()))?;
    let img = image::load_from_memory_with_format(&png, ImageFormat::Png).context("无法解码图片")?;
    let mut bmp = Vec::new();
    // 24 位 BMP 兼容性最好；透明度由 PNG 格式承载
    DynamicImage::ImageRgb8(img.to_rgb8()).write_to(&mut Cursor::new(&mut bmp), ImageFormat::Bmp)?;
    Ok(Payload::Image { png, dib: bmp.split_off(BMP_FILE_HEADER) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    #[test]
    fn deleted_image_file_falls_back_to_saved_png() {
        let dir = tempfile::tempdir().unwrap();
        let images = dir.path().join("images");
        fs::create_dir_all(&images).unwrap();
        RgbaImage::from_pixel(16, 8, Rgba([10, 20, 30, 255]))
            .save(images.join("saved.png"))
            .unwrap();

        let existing = dir.path().join("alive.png");
        fs::write(&existing, "placeholder").unwrap();
        let alive = ClipContent {
            kind: ClipKind::Files,
            text: Some(existing.display().to_string()),
            html: None,
            rtf: None,
            image_file: Some("saved.png".into()),
        };
        assert!(matches!(payload(alive, false, &images).unwrap(), Payload::Files(_)));

        let missing_path = dir.path().join("1795569871982_d.png").display().to_string();
        let missing = ClipContent {
            kind: ClipKind::Files,
            text: Some(missing_path.clone()),
            html: None,
            rtf: None,
            image_file: Some("saved.png".into()),
        };
        assert!(matches!(payload(missing.clone(), false, &images).unwrap(), Payload::Image { .. }));
        assert!(matches!(
            payload(missing, true, &images).unwrap(),
            Payload::Text { text, html: None, rtf: None } if text == missing_path
        ));
    }
}
