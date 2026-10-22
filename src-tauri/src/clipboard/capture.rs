use std::{fs, io::Cursor, path::Path};

use anyhow::Context;
use image::{DynamicImage, ImageFormat};

use crate::store::{ClipKind, NewClip};

/// 列表缩略图的最大尺寸（按 2 倍屏预留）。
const THUMB_MAX: (u32, u32) = (360, 180);

/// 从剪贴板读出的原始内容，读取时尽快关闭剪贴板，解码与编码放到之后进行。
#[derive(Debug)]
pub enum Captured {
    Files(Vec<String>),
    Text {
        text: String,
        html: Option<String>,
        rtf: Option<String>,
    },
    Image {
        bytes: Vec<u8>,
        format: ImageFormat,
    },
}

/// 计算内容哈希、落盘图片并生成缩略图。内容超出大小上限时返回 `None`。
pub fn prepare(
    captured: Captured,
    source_app: Option<String>,
    images_dir: &Path,
    max_item_bytes: usize,
) -> anyhow::Result<Option<NewClip>> {
    let clip = match captured {
        Captured::Files(paths) => {
            let text = paths.join("\n");
            if text.len() > max_item_bytes {
                return Ok(None);
            }
            NewClip {
                kind: ClipKind::Files,
                content_hash: hash(b"files", &[text.as_bytes()]),
                size_bytes: text.len() as i64,
                text: Some(text),
                html: None,
                rtf: None,
                image_file: None,
                thumb: None,
                source_app,
                permanent: false,
            }
        }
        Captured::Text { text, html, rtf } => {
            let size = text.len() + html.as_ref().map_or(0, String::len) + rtf.as_ref().map_or(0, String::len);
            if size > max_item_bytes {
                return Ok(None);
            }
            NewClip {
                kind: if html.is_some() || rtf.is_some() { ClipKind::Rich } else { ClipKind::Text },
                // 纯文本与富文本按文字内容去重，同一段文字只保留一条
                content_hash: hash(b"text", &[text.as_bytes()]),
                size_bytes: size as i64,
                text: Some(text),
                html,
                rtf,
                image_file: None,
                thumb: None,
                source_app,
                permanent: false,
            }
        }
        Captured::Image { bytes, format } => {
            let img = image::load_from_memory_with_format(&bytes, format).context("无法解码剪贴板图片")?;
            let rgba = img.to_rgba8();
            let (w, h) = rgba.dimensions();
            // 按像素去重：同一张图以 PNG 或 BMP 格式出现时哈希一致
            let content_hash = hash(b"image", &[&w.to_le_bytes(), &h.to_le_bytes(), rgba.as_raw()]);
            let img = DynamicImage::ImageRgba8(rgba);
            let png = encode_png(&img)?;
            if png.len() > max_item_bytes {
                return Ok(None);
            }
            let file_name = format!("{content_hash}.png");
            write_once(&images_dir.join(&file_name), &png)?;
            NewClip {
                kind: ClipKind::Image,
                text: Some(format!("{w}×{h}")),
                size_bytes: png.len() as i64,
                thumb: Some(encode_png(&thumbnail(&img))?),
                image_file: Some(file_name),
                content_hash,
                html: None,
                rtf: None,
                source_app,
                permanent: false,
            }
        }
    };
    Ok(Some(clip))
}

fn hash(tag: &[u8], parts: &[&[u8]]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(tag);
    hasher.update(&[0]);
    for p in parts {
        hasher.update(p);
    }
    hasher.finalize().to_hex().to_string()
}

fn thumbnail(img: &DynamicImage) -> DynamicImage {
    if img.width() <= THUMB_MAX.0 && img.height() <= THUMB_MAX.1 {
        img.clone()
    } else {
        img.thumbnail(THUMB_MAX.0, THUMB_MAX.1)
    }
}

fn encode_png(img: &DynamicImage) -> anyhow::Result<Vec<u8>> {
    let mut buf = Vec::new();
    img.write_to(&mut Cursor::new(&mut buf), ImageFormat::Png)?;
    Ok(buf)
}

/// 文件名由内容哈希决定，已存在即视为相同内容；先写临时文件再改名，避免留下半个文件。
fn write_once(path: &Path, data: &[u8]) -> anyhow::Result<()> {
    if path.exists() {
        return Ok(());
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, data).with_context(|| format!("写入图片失败 {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("保存图片失败 {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    const MAX: usize = 1024 * 1024;

    fn sample(w: u32, h: u32) -> DynamicImage {
        DynamicImage::ImageRgba8(RgbaImage::from_fn(w, h, |x, y| Rgba([x as u8, y as u8, 128, 255])))
    }

    fn encoded(img: &DynamicImage, format: ImageFormat) -> Vec<u8> {
        let mut buf = Vec::new();
        img.write_to(&mut Cursor::new(&mut buf), format).unwrap();
        buf
    }

    fn text(t: &str, html: Option<&str>) -> Captured {
        Captured::Text { text: t.into(), html: html.map(Into::into), rtf: None }
    }

    #[test]
    fn plain_and_rich_text_share_hash() {
        let dir = tempfile::tempdir().unwrap();
        let plain = prepare(text("hi", None), None, dir.path(), MAX).unwrap().unwrap();
        let rich = prepare(text("hi", Some("<b>hi</b>")), None, dir.path(), MAX).unwrap().unwrap();
        assert_eq!(plain.kind, ClipKind::Text);
        assert_eq!(rich.kind, ClipKind::Rich);
        assert_eq!(plain.content_hash, rich.content_hash);
    }

    #[test]
    fn kinds_do_not_collide() {
        let dir = tempfile::tempdir().unwrap();
        let t = prepare(text("C:\\a.txt", None), None, dir.path(), MAX).unwrap().unwrap();
        let f = prepare(Captured::Files(vec!["C:\\a.txt".into()]), None, dir.path(), MAX).unwrap().unwrap();
        assert_eq!(f.kind, ClipKind::Files);
        assert_ne!(t.content_hash, f.content_hash);
    }

    #[test]
    fn oversized_content_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        assert!(prepare(text("hello", None), None, dir.path(), 4).unwrap().is_none());
    }

    #[test]
    fn image_is_saved_with_thumbnail_and_pixel_hash() {
        let dir = tempfile::tempdir().unwrap();
        let img = sample(800, 200);
        let from_png = Captured::Image { bytes: encoded(&img, ImageFormat::Png), format: ImageFormat::Png };
        let clip = prepare(from_png, Some("SnippingTool.exe".into()), dir.path(), MAX).unwrap().unwrap();

        assert_eq!(clip.kind, ClipKind::Image);
        assert_eq!(clip.text.as_deref(), Some("800×200"));
        let file = dir.path().join(clip.image_file.as_ref().unwrap());
        assert_eq!(fs::read(&file).unwrap().len() as i64, clip.size_bytes);

        let thumb = image::load_from_memory(clip.thumb.as_ref().unwrap()).unwrap();
        assert!(thumb.width() <= THUMB_MAX.0 && thumb.height() <= THUMB_MAX.1);

        let from_bmp = Captured::Image { bytes: encoded(&img, ImageFormat::Bmp), format: ImageFormat::Bmp };
        let again = prepare(from_bmp, None, dir.path(), MAX).unwrap().unwrap();
        assert_eq!(again.content_hash, clip.content_hash);
    }

    #[test]
    fn corrupt_image_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let bad = Captured::Image { bytes: vec![1, 2, 3], format: ImageFormat::Png };
        assert!(prepare(bad, None, dir.path(), MAX).is_err());
    }
}
