//! 一次完整同步。远端布局（相对于用户配置的根目录）：
//!
//! ```text
//! meta.json                    格式版本与加密参数
//! clips/<id>.<version>.rec     条目内容（JSON，开启加密时为密文）
//! clips/<id>.<version>.del     删除标记
//! blobs/<id>                   图片 PNG（开启加密时为密文）
//! ```
//!
//! `id` 为内容哈希（加密时为带密钥的哈希），`version` 为更新时间（毫秒）。每次修改写入新对象，
//! 同一条目以版本最大者为准（后写入者胜出），旧版本在同步时清理。多台设备同时写入不会互相覆盖。

use std::{collections::HashMap, fs, path::Path};

use anyhow::{bail, Context};
use base64::{engine::general_purpose::STANDARD, Engine};
use opendal::{ErrorKind, Operator};
use serde::{Deserialize, Serialize};

use super::crypto::{random_salt, Cipher};
use crate::store::{ClipRecord, LocalState, Store};

const META: &str = "meta.json";
const CLIPS: &str = "clips/";
const BLOBS: &str = "blobs/";
const FORMAT: u32 = 1;
const CHECK: &[u8] = b"anypaste-sync-check";
/// 删除标记保留时长，超过后从云端清理。
pub const TOMBSTONE_TTL_MS: i64 = 30 * 24 * 3600 * 1000;

#[derive(Debug, Serialize, Deserialize)]
struct Meta {
    format: u32,
    encryption: Option<EncryptionMeta>,
}

#[derive(Debug, Serialize, Deserialize)]
struct EncryptionMeta {
    kdf: String,
    salt: String,
    /// 用派生密钥加密的固定串，用于校验口令。
    check: String,
}

/// 云端数据的现状，用于“测试连接”。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteState {
    Empty,
    Plain,
    Encrypted,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub uploaded: usize,
    pub downloaded: usize,
    /// 因云端删除而在本地删除的条目数。
    pub deleted: usize,
}

impl Report {
    pub fn changed_local(&self) -> bool {
        self.downloaded > 0 || self.deleted > 0
    }
}

pub async fn inspect(op: &Operator) -> anyhow::Result<RemoteState> {
    Ok(match read_meta(op).await? {
        None => RemoteState::Empty,
        Some(Meta { encryption: None, .. }) => RemoteState::Plain,
        Some(Meta { encryption: Some(_), .. }) => RemoteState::Encrypted,
    })
}

/// 读取（不存在时创建）meta.json，校验加密设置与口令，返回用于本次同步的密钥。
pub async fn open(op: &Operator, passphrase: Option<&str>) -> anyhow::Result<Option<Cipher>> {
    let Some(meta) = read_meta(op).await? else {
        let (cipher, encryption) = match passphrase {
            Some(p) => {
                let salt = random_salt();
                let cipher = Cipher::derive(p, &salt)?;
                let check = STANDARD.encode(cipher.encrypt(CHECK)?);
                (Some(cipher), Some(EncryptionMeta { kdf: "argon2id".into(), salt: STANDARD.encode(salt), check }))
            }
            None => (None, None),
        };
        let meta = Meta { format: FORMAT, encryption };
        op.write(META, serde_json::to_vec_pretty(&meta)?).await.context("写入 meta.json 失败")?;
        return Ok(cipher);
    };
    if meta.format > FORMAT {
        bail!("云端数据由更新版本的 AnyPaste 创建，请升级后再同步");
    }
    match (meta.encryption, passphrase) {
        (Some(enc), Some(p)) => {
            let salt = STANDARD.decode(&enc.salt).context("meta.json 中的 salt 无效")?;
            let cipher = Cipher::derive(p, &salt)?;
            let check = STANDARD.decode(&enc.check).context("meta.json 中的校验串无效")?;
            match cipher.decrypt(&check) {
                Ok(v) if v == CHECK => Ok(Some(cipher)),
                _ => bail!("同步口令错误"),
            }
        }
        (Some(_), None) => bail!("云端数据已加密，请开启端到端加密并填写同步口令"),
        (None, Some(_)) => bail!("云端已有未加密的数据，不能直接改为加密。如需加密，请换一个新的远端目录"),
        (None, None) => Ok(None),
    }
}

async fn read_meta(op: &Operator) -> anyhow::Result<Option<Meta>> {
    match op.read(META).await {
        Ok(buf) => Ok(Some(serde_json::from_slice(&buf.to_vec()).context("meta.json 格式无效")?)),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(anyhow::Error::new(e).context("读取 meta.json 失败")),
    }
}

/// 云端某条目的全部版本。
#[derive(Debug, Default)]
struct RemoteItem {
    /// (版本, 是否为删除标记, 路径)
    versions: Vec<(i64, bool, String)>,
}

impl RemoteItem {
    fn latest(&self) -> Option<(i64, bool, &str)> {
        self.versions.iter().max_by_key(|v| (v.0, v.1)).map(|(v, d, p)| (*v, *d, p.as_str()))
    }
}

/// 解析 `<id>.<version>.<rec|del>`；无法识别的文件忽略。
fn parse_name(name: &str) -> Option<(&str, i64, bool)> {
    let mut parts = name.rsplitn(3, '.');
    let ext = parts.next()?;
    let version = parts.next()?.parse().ok()?;
    let id = parts.next().filter(|s| !s.is_empty())?;
    match ext {
        "rec" => Some((id, version, false)),
        "del" => Some((id, version, true)),
        _ => None,
    }
}

/// 本地图片均以 `<内容哈希>.png` 命名；只接受由字母、数字、`-`、`_` 组成主名的 PNG 文件名。
fn is_safe_image_name(name: &str) -> bool {
    name.strip_suffix(".png")
        .is_some_and(|stem| !stem.is_empty() && stem.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'))
}

async fn list_remote(op: &Operator) -> anyhow::Result<HashMap<String, RemoteItem>> {
    let entries = match op.list(CLIPS).await {
        Ok(entries) => entries,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(HashMap::new()),
        Err(e) => return Err(anyhow::Error::new(e).context("列出云端条目失败")),
    };
    let mut items: HashMap<String, RemoteItem> = HashMap::new();
    for entry in entries {
        if let Some((id, version, del)) = parse_name(entry.name()) {
            items.entry(id.to_string()).or_default().versions.push((version, del, entry.path().to_string()));
        }
    }
    Ok(items)
}

pub struct Sync<'a> {
    pub op: &'a Operator,
    pub store: &'a Store,
    pub images_dir: &'a Path,
    pub cipher: Option<&'a Cipher>,
    pub now: i64,
}

impl Sync<'_> {
    fn id(&self, content_hash: &str) -> String {
        match self.cipher {
            Some(c) => c.object_id(content_hash),
            None => content_hash.to_string(),
        }
    }

    fn seal(&self, plain: Vec<u8>) -> anyhow::Result<Vec<u8>> {
        match self.cipher {
            Some(c) => c.encrypt(&plain),
            None => Ok(plain),
        }
    }

    fn open(&self, data: Vec<u8>) -> anyhow::Result<Vec<u8>> {
        match self.cipher {
            Some(c) => c.decrypt(&data),
            None => Ok(data),
        }
    }

    /// 双向同步：逐条比较本地与云端的最新版本，较新的一方覆盖另一方。
    /// 返回值中的 `image_files` 为需要由调用方删除的本地图片。
    pub async fn run(&self) -> anyhow::Result<(Report, Vec<String>)> {
        let mut remote = list_remote(self.op).await?;
        let mut report = Report::default();
        let mut removed_images = Vec::new();

        for entry in self.store.sync_entries()? {
            let id = self.id(&entry.content_hash);
            let item = remote.remove(&id).unwrap_or_default();
            let latest = item.latest().map(|(v, d, p)| (v, d, p.to_string()));
            let local_newer = latest.as_ref().is_none_or(|(v, _, _)| entry.version > *v);
            let remote_newer = latest.as_ref().is_some_and(|(v, _, _)| *v > entry.version);

            let mut written = None;
            match entry.state {
                LocalState::Live if local_newer => {
                    written = Some(self.upload_record(&id, &entry.content_hash).await?);
                    report.uploaded += 1;
                }
                // 本地已删除，或已转为临时：云端仍有内容时上传删除标记
                LocalState::Deleted | LocalState::Local if local_newer => {
                    if latest.as_ref().is_some_and(|(_, del, _)| !del) {
                        written = Some(self.upload_tombstone(&id, entry.version).await?);
                        report.uploaded += 1;
                    }
                }
                _ if remote_newer => {
                    let (version, del, path) = latest.as_ref().expect("remote_newer 意味着云端存在版本");
                    if *del {
                        if entry.state != LocalState::Local {
                            let d = self.store.apply_remote_delete(&entry.content_hash)?;
                            report.deleted += d.rows;
                            removed_images.extend(d.image_files);
                        }
                    } else {
                        self.download(path, *version).await?;
                        report.downloaded += 1;
                    }
                }
                _ => {}
            }
            self.cleanup(&item, written.as_deref()).await?;
        }

        // 仅云端存在的条目：本机没有过，下载有效内容；删除标记无需处理
        for item in remote.values() {
            if let Some((version, false, path)) = item.latest() {
                self.download(path, version).await?;
                report.downloaded += 1;
            }
            self.cleanup(item, None).await?;
        }
        Ok((report, removed_images))
    }

    /// 删除旧版本；删除标记超过保留期后也一并删除。`written` 为本轮刚写入的路径。
    async fn cleanup(&self, item: &RemoteItem, written: Option<&str>) -> anyhow::Result<()> {
        let keep = written.or_else(|| item.latest().map(|(_, _, p)| p));
        for (version, del, path) in &item.versions {
            let expired = *del && self.now - version > TOMBSTONE_TTL_MS;
            if Some(path.as_str()) != keep || expired {
                self.op.delete(path).await.with_context(|| format!("删除云端旧版本 {path} 失败"))?;
            }
        }
        Ok(())
    }

    async fn upload_record(&self, id: &str, content_hash: &str) -> anyhow::Result<String> {
        let rec = self
            .store
            .clip_record(content_hash)?
            .context("待上传的条目已不存在")?;
        if let Some(file) = &rec.image_file {
            self.upload_blob(content_hash, file).await?;
        }
        let path = format!("{CLIPS}{id}.{}.rec", rec.updated_at);
        let body = self.seal(serde_json::to_vec(&rec)?)?;
        self.op.write(&path, body).await.with_context(|| format!("上传 {path} 失败"))?;
        Ok(path)
    }

    async fn upload_tombstone(&self, id: &str, version: i64) -> anyhow::Result<String> {
        let path = format!("{CLIPS}{id}.{version}.del");
        self.op.write(&path, Vec::<u8>::new()).await.with_context(|| format!("上传 {path} 失败"))?;
        Ok(path)
    }

    /// 图片按内容只上传一次。
    async fn upload_blob(&self, content_hash: &str, file: &str) -> anyhow::Result<()> {
        let path = format!("{BLOBS}{}", self.id(content_hash));
        match self.op.stat(&path).await {
            Ok(_) => return Ok(()),
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => return Err(anyhow::Error::new(e).context("查询云端图片失败")),
        }
        let local = self.images_dir.join(file);
        let bytes = fs::read(&local).with_context(|| format!("读取图片失败 {}", local.display()))?;
        self.op.write(&path, self.seal(bytes)?).await.with_context(|| format!("上传 {path} 失败"))?;
        Ok(())
    }

    /// `version` 取自对象名。对象名不受加密保护，须与内容中的版本一致，
    /// 否则云端把旧版本改名为新版本号即可让各设备回退，并在每轮同步中反复下载。
    async fn download(&self, path: &str, version: i64) -> anyhow::Result<()> {
        let data = self.op.read(path).await.with_context(|| format!("下载 {path} 失败"))?.to_vec();
        let rec: ClipRecord = serde_json::from_slice(&self.open(data)?).with_context(|| format!("{path} 内容无效"))?;
        if rec.updated_at != version {
            bail!("{path} 的版本与内容不一致");
        }
        if let Some(file) = &rec.image_file {
            // 文件名来自云端，会与本地图片目录拼接后写入、日后删除，必须是单段文件名
            if !is_safe_image_name(file) {
                bail!("{path} 中的图片文件名无效");
            }
            self.download_blob(&rec.content_hash, file).await?;
        }
        self.store.apply_remote_record(&rec)?;
        Ok(())
    }

    async fn download_blob(&self, content_hash: &str, file: &str) -> anyhow::Result<()> {
        let local = self.images_dir.join(file);
        if local.exists() {
            return Ok(());
        }
        let path = format!("{BLOBS}{}", self.id(content_hash));
        let data = self.op.read(&path).await.with_context(|| format!("下载 {path} 失败"))?.to_vec();
        let tmp = local.with_extension("tmp");
        fs::write(&tmp, self.open(data)?)?;
        fs::rename(&tmp, &local)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{clips_tests::text_clip, sync_tests::perm, NewClip};

    const TTL: i64 = 1000;

    struct Device {
        store: Store,
        dir: tempfile::TempDir,
    }

    impl Device {
        fn new() -> Self {
            Self { store: Store::open_in_memory().unwrap(), dir: tempfile::tempdir().unwrap() }
        }

        async fn sync(&self, op: &Operator, pass: Option<&str>, now: i64) -> anyhow::Result<Report> {
            let cipher = open(op, pass).await?;
            let (report, images) =
                Sync { op, store: &self.store, images_dir: self.dir.path(), cipher: cipher.as_ref(), now }.run().await?;
            for f in images {
                let _ = fs::remove_file(self.dir.path().join(f));
            }
            Ok(report)
        }

        fn perm_texts(&self) -> Vec<String> {
            let mut v: Vec<String> =
                self.store.list_clips(true, None, None, 0).unwrap().into_iter().map(|i| i.preview).collect();
            v.sort();
            v
        }

        fn id_of(&self, text: &str) -> i64 {
            self.store.list_clips(true, Some(text), None, 0).unwrap()[0].id
        }
    }

    fn memory() -> Operator {
        Operator::new(opendal::services::Memory::default()).unwrap().finish()
    }

    /// 云端的条目对象名（WebDAV 列目录时会包含目录本身，按命名规则过滤）。
    async fn remote_names(op: &Operator) -> Vec<String> {
        let mut v: Vec<String> = op
            .list(CLIPS)
            .await
            .unwrap()
            .into_iter()
            .map(|e| e.name().to_string())
            .filter(|n| parse_name(n).is_some())
            .collect();
        v.sort();
        v
    }

    #[test]
    fn parses_object_names() {
        assert_eq!(parse_name("abc.123.rec"), Some(("abc", 123, false)));
        assert_eq!(parse_name("abc.9.del"), Some(("abc", 9, true)));
        assert_eq!(parse_name("abc.x.rec"), None);
        assert_eq!(parse_name(".1.rec"), None);
        assert_eq!(parse_name("readme.txt"), None);
    }

    #[test]
    fn image_names_must_be_single_segment() {
        assert!(is_safe_image_name("0123abcdef.png"));
        assert!(is_safe_image_name("img_1-a.png"));
        for bad in ["..\\..\\evil.bat", "../a.png", "a\\b.png", "C:a.png", ".png", "a.png.exe", "a b.png", "a.PNG"] {
            assert!(!is_safe_image_name(bad), "{bad}");
        }
    }

    #[tokio::test]
    async fn rejects_renamed_old_version() {
        let op = memory();
        let (a, b) = (Device::new(), Device::new());
        perm(&a.store, "old", 10);
        a.sync(&op, Some("pw"), 100).await.unwrap();

        // 云端把对象改名为更高的版本号，内容仍是旧版本
        let name = remote_names(&op).await.remove(0);
        let (id, version, _) = parse_name(&name).unwrap();
        let body = op.read(&format!("{CLIPS}{name}")).await.unwrap().to_vec();
        op.write(&format!("{CLIPS}{id}.{}.rec", version + 1000), body).await.unwrap();
        op.delete(&format!("{CLIPS}{name}")).await.unwrap();

        assert!(b.sync(&op, Some("pw"), 100).await.unwrap_err().to_string().contains("版本与内容不一致"));
        assert!(b.perm_texts().is_empty());
    }

    #[tokio::test]
    async fn rejects_traversal_in_remote_image_name() {
        let op = memory();
        let (a, b) = (Device::new(), Device::new());
        // 伪造一条图片文件名越出图片目录的云端记录
        let clip = NewClip { image_file: Some("..\\evil.png".into()), permanent: true, ..text_clip("bad") };
        a.store.upsert_clip(&clip, 10, TTL).unwrap();
        let rec = a.store.clip_record(&clip.content_hash).unwrap().unwrap();
        op.write(META, serde_json::to_vec(&Meta { format: FORMAT, encryption: None }).unwrap()).await.unwrap();
        op.write(&format!("{CLIPS}{}.{}.rec", clip.content_hash, rec.updated_at), serde_json::to_vec(&rec).unwrap())
            .await
            .unwrap();
        op.write(&format!("{BLOBS}{}", clip.content_hash), b"evil".to_vec()).await.unwrap();

        assert!(b.sync(&op, None, 100).await.unwrap_err().to_string().contains("图片文件名无效"));
        assert!(b.perm_texts().is_empty());
    }

    #[tokio::test]
    async fn two_devices_converge() {
        let op = memory();
        let (a, b) = (Device::new(), Device::new());
        perm(&a.store, "alpha", 10);
        perm(&a.store, "beta", 20);
        a.store.upsert_clip(&text_clip("temp only"), 30, TTL).unwrap();

        assert_eq!(a.sync(&op, None, 100).await.unwrap(), Report { uploaded: 2, ..Default::default() });
        assert_eq!(remote_names(&op).await.len(), 2);
        assert_eq!(b.sync(&op, None, 100).await.unwrap(), Report { downloaded: 2, ..Default::default() });
        assert_eq!(b.perm_texts(), vec!["alpha", "beta"]);
        // 再次同步没有变化
        assert_eq!(a.sync(&op, None, 100).await.unwrap(), Report::default());
        assert_eq!(b.sync(&op, None, 100).await.unwrap(), Report::default());
    }

    #[tokio::test]
    async fn deletion_propagates_and_old_versions_are_cleaned() {
        let op = memory();
        let (a, b) = (Device::new(), Device::new());
        perm(&a.store, "gone", 10);
        a.sync(&op, None, 100).await.unwrap();
        b.sync(&op, None, 100).await.unwrap();

        b.store.tombstone_clip(b.id_of("gone"), 200).unwrap();
        assert_eq!(b.sync(&op, None, 300).await.unwrap().uploaded, 1);
        // 只保留最新的删除标记
        let names = remote_names(&op).await;
        assert_eq!(names.len(), 1);
        assert!(names[0].ends_with(".200.del"));

        assert_eq!(a.sync(&op, None, 300).await.unwrap().deleted, 1);
        assert!(a.perm_texts().is_empty());

        // 删除标记过期后被清理
        a.sync(&op, None, 200 + TOMBSTONE_TTL_MS + 1).await.unwrap();
        assert!(remote_names(&op).await.is_empty());
    }

    #[tokio::test]
    async fn newer_change_wins() {
        let op = memory();
        let (a, b) = (Device::new(), Device::new());
        perm(&a.store, "shared", 10);
        a.sync(&op, None, 100).await.unwrap();
        b.sync(&op, None, 100).await.unwrap();

        // 两台设备分别使用（置顶）了该条目，B 更晚
        a.store.touch(a.id_of("shared"), 500, TTL).unwrap();
        b.store.touch(b.id_of("shared"), 600, TTL).unwrap();
        a.sync(&op, None, 700).await.unwrap();
        b.sync(&op, None, 700).await.unwrap();
        a.sync(&op, None, 700).await.unwrap();

        let last = |d: &Device| d.store.list_clips(true, None, None, 0).unwrap()[0].last_copied_at;
        assert_eq!((last(&a), last(&b)), (600, 600));
        assert_eq!(remote_names(&op).await.len(), 1);
    }

    #[tokio::test]
    async fn permanent_to_temp_removes_from_other_devices() {
        let op = memory();
        let (a, b) = (Device::new(), Device::new());
        perm(&a.store, "demote", 10);
        a.sync(&op, None, 100).await.unwrap();
        b.sync(&op, None, 100).await.unwrap();

        a.store.set_permanent(a.id_of("demote"), false, 200, TTL).unwrap();
        a.sync(&op, None, 300).await.unwrap();
        b.sync(&op, None, 300).await.unwrap();
        assert!(b.perm_texts().is_empty());
        // A 本机仍保留该临时条目
        assert_eq!(a.store.list_clips(false, None, None, 0).unwrap().len(), 1);
    }

    #[tokio::test]
    async fn same_content_on_two_devices_merges() {
        let op = memory();
        let (a, b) = (Device::new(), Device::new());
        perm(&a.store, "same", 10);
        perm(&b.store, "same", 20);
        a.sync(&op, None, 100).await.unwrap();
        b.sync(&op, None, 100).await.unwrap();
        a.sync(&op, None, 100).await.unwrap();
        assert_eq!(a.perm_texts(), vec!["same"]);
        assert_eq!(b.perm_texts(), vec!["same"]);
        assert_eq!(remote_names(&op).await.len(), 1);
    }

    #[tokio::test]
    async fn images_are_transferred() {
        let op = memory();
        let (a, b) = (Device::new(), Device::new());
        fs::write(a.dir.path().join("img.png"), b"png-bytes").unwrap();
        let clip = NewClip { image_file: Some("img.png".into()), permanent: true, ..text_clip("img") };
        a.store.upsert_clip(&clip, 10, TTL).unwrap();

        a.sync(&op, Some("pw"), 100).await.unwrap();
        b.sync(&op, Some("pw"), 100).await.unwrap();
        assert_eq!(fs::read(b.dir.path().join("img.png")).unwrap(), b"png-bytes");
    }

    #[tokio::test]
    async fn encryption_requires_matching_passphrase() {
        let op = memory();
        let (a, b) = (Device::new(), Device::new());
        perm(&a.store, "secret text", 10);
        a.sync(&op, Some("right"), 100).await.unwrap();

        // 云端对象不含明文，也不以内容哈希命名
        let names = remote_names(&op).await;
        assert!(!names[0].starts_with("hash-secret"));
        let body = op.read(&format!("{CLIPS}{}", names[0])).await.unwrap().to_vec();
        assert!(!String::from_utf8_lossy(&body).contains("secret text"));
        assert_eq!(inspect(&op).await.unwrap(), RemoteState::Encrypted);

        assert!(b.sync(&op, Some("wrong"), 100).await.unwrap_err().to_string().contains("口令错误"));
        assert!(b.sync(&op, None, 100).await.is_err());
        assert!(b.perm_texts().is_empty());
        b.sync(&op, Some("right"), 100).await.unwrap();
        assert_eq!(b.perm_texts(), vec!["secret text"]);
    }

    /// 针对真实 WebDAV 服务器的同步，需设置环境变量后手动运行：
    /// `ANYPASTE_TEST_WEBDAV=http://127.0.0.1:8765/dav ANYPASTE_TEST_WEBDAV_USER=u ANYPASTE_TEST_WEBDAV_PASS=p
    ///  cargo test -- --ignored webdav`
    #[tokio::test]
    #[ignore]
    async fn webdav_round_trip() {
        let Ok(endpoint) = std::env::var("ANYPASTE_TEST_WEBDAV") else {
            return;
        };
        let root = format!("/anypaste-test-{}", uuid::Uuid::new_v4());
        let mut b = opendal::services::Webdav::default().endpoint(&endpoint).root(&root);
        if let (Ok(u), Ok(p)) = (std::env::var("ANYPASTE_TEST_WEBDAV_USER"), std::env::var("ANYPASTE_TEST_WEBDAV_PASS")) {
            b = b.username(&u).password(&p);
        }
        let op = Operator::new(b).unwrap().finish();

        let (a, b) = (Device::new(), Device::new());
        perm(&a.store, "over webdav", 10);
        fs::write(a.dir.path().join("pic.png"), b"png").unwrap();
        let clip = NewClip { image_file: Some("pic.png".into()), permanent: true, ..text_clip("pic") };
        a.store.upsert_clip(&clip, 20, TTL).unwrap();

        assert_eq!(a.sync(&op, Some("pw"), 100).await.unwrap().uploaded, 2);
        assert_eq!(b.sync(&op, Some("pw"), 100).await.unwrap().downloaded, 2);
        assert_eq!(b.perm_texts(), vec!["over webdav", "pic"]);
        assert_eq!(fs::read(b.dir.path().join("pic.png")).unwrap(), b"png");

        b.store.tombstone_clip(b.id_of("over webdav"), 200).unwrap();
        b.sync(&op, Some("pw"), 300).await.unwrap();
        assert_eq!(a.sync(&op, Some("pw"), 300).await.unwrap().deleted, 1);
        assert_eq!(remote_names(&op).await.len(), 2);
        assert!(b.sync(&op, Some("wrong"), 300).await.is_err());

        op.delete_with("/").recursive(true).await.unwrap();
    }

    #[tokio::test]
    async fn plain_remote_rejects_new_passphrase() {
        let op = memory();
        let a = Device::new();
        a.sync(&op, None, 100).await.unwrap();
        assert_eq!(inspect(&op).await.unwrap(), RemoteState::Plain);
        assert!(a.sync(&op, Some("pw"), 100).await.is_err());
    }
}
