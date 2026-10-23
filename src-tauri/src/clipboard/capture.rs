use std::{fs, io::Cursor, path::Path};

use anyhow::Context;
use image::{DynamicImage, ImageFormat};

use crate::{
    settings::Settings,
    store::{ClipKind, NewClip},
};

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

/// 设置中“记录的内容类型”。
#[derive(Debug, Clone, Copy)]
pub struct KindFilter {
    pub text: bool,
    pub rich: bool,
    pub image: bool,
    pub files: bool,
}

impl From<&Settings> for KindFilter {
    fn from(s: &Settings) -> Self {
        Self { text: s.capture_text, rich: s.capture_rich, image: s.capture_image, files: s.capture_files }
    }
}

impl KindFilter {
    /// 过滤掉未启用的类型；关闭富文本时只保留纯文本。
    pub fn apply(self, captured: Captured) -> Option<Captured> {
        match captured {
            Captured::Files(_) if !self.files => None,
            Captured::Image { .. } if !self.image => None,
            Captured::Text { .. } if !self.text => None,
            Captured::Text { text, .. } if !self.rich => Some(Captured::Text { text, html: None, rtf: None }),
            other => Some(other),
        }
    }
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
            // 微信等程序复制图片时放入的是图片文件：生成缩略图并保存副本，原文件被清理后仍可预览
            let preview = match paths.as_slice() {
                [single] => image_file_preview(Path::new(single), images_dir, max_item_bytes),
                _ => None,
            };
            let (image_file, thumb) = preview.unzip();
            NewClip {
                kind: ClipKind::Files,
                content_hash: hash(b"files", &[text.as_bytes()]),
                size_bytes: text.len() as i64,
                text: Some(text),
                html: None,
                rtf: None,
                image_file,
                thumb,
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

/// 可生成预览的图片文件扩展名。
const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "gif", "bmp", "webp"];

/// 为单个图片文件保存 PNG 副本并生成缩略图，返回（副本文件名, 缩略图）。
/// 只是附加的预览：不是图片、过大或解码失败时返回 `None`，条目仍按文件记录。
fn image_file_preview(path: &Path, images_dir: &Path, max_item_bytes: usize) -> Option<(String, Vec<u8>)> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    if !IMAGE_EXTS.contains(&ext.as_str()) {
        return None;
    }
    let result = (|| -> anyhow::Result<Option<(String, Vec<u8>)>> {
        if fs::metadata(path)?.len() as usize > max_item_bytes {
            return Ok(None);
        }
        let img = image::load_from_memory(&fs::read(path)?)?;
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        let pixels = hash(b"image", &[&w.to_le_bytes(), &h.to_le_bytes(), rgba.as_raw()]);
        let img = DynamicImage::ImageRgba8(rgba);
        let file_name = format!("{pixels}.png");
        write_once(&images_dir.join(&file_name), &encode_png(&img)?)?;
        Ok(Some((file_name, encode_png(&thumbnail(&img))?)))
    })();
    result.unwrap_or_else(|e| {
        log::debug!("图片文件无法生成预览: {e:#}");
        None
    })
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
    fn kind_filter_drops_disabled_kinds() {
        let all = KindFilter { text: true, rich: true, image: true, files: true };
        let rich = || text("hi", Some("<b>hi</b>"));
        assert!(matches!(all.apply(rich()), Some(Captured::Text { html: Some(_), .. })));

        let no_rich = KindFilter { rich: false, ..all };
        assert!(matches!(no_rich.apply(rich()), Some(Captured::Text { html: None, rtf: None, .. })));
        assert!(KindFilter { text: false, ..all }.apply(rich()).is_none());
        assert!(KindFilter { files: false, ..all }.apply(Captured::Files(vec!["a".into()])).is_none());
        let img = Captured::Image { bytes: vec![], format: ImageFormat::Png };
        assert!(KindFilter { image: false, ..all }.apply(img).is_none());
    }

    #[test]
    fn single_image_file_gets_preview() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("wechat.jpg");
        fs::write(&src, encoded(&DynamicImage::ImageRgb8(sample(600, 300).to_rgb8()), ImageFormat::Jpeg)).unwrap();
        let images = dir.path().join("images");
        fs::create_dir_all(&images).unwrap();

        let files = Captured::Files(vec![src.display().to_string()]);
        let clip = prepare(files, Some("Weixin.exe".into()), &images, MAX).unwrap().unwrap();
        assert_eq!(clip.kind, ClipKind::Files);
        assert!(clip.thumb.is_some());
        assert!(images.join(clip.image_file.unwrap()).exists());
    }

    #[test]
    fn other_files_have_no_preview() {
        let dir = tempfile::tempdir().unwrap();
        let txt = dir.path().join("a.txt");
        fs::write(&txt, "hi").unwrap();
        let broken = dir.path().join("broken.png");
        fs::write(&broken, "not a png").unwrap();
        for files in [
            vec![txt.display().to_string()],
            vec![broken.display().to_string()],
            vec![txt.display().to_string(), broken.display().to_string()],
        ] {
            let clip = prepare(Captured::Files(files), None, dir.path(), MAX).unwrap().unwrap();
            assert!(clip.thumb.is_none() && clip.image_file.is_none());
        }
    }

    #[test]
    fn corrupt_image_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let bad = Captured::Image { bytes: vec![1, 2, 3], format: ImageFormat::Png };
        assert!(prepare(bad, None, dir.path(), MAX).is_err());
    }
}
