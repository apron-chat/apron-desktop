//! Opt-in private profile storage. No token, message history, or attachment fields exist.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::PathBuf,
    sync::mpsc,
};
pub const FILE_LIMIT: usize = 512 * 1024;
pub const DRAFT_LIMIT: usize = 256 * 1024;
/// Small, profile-scoped choices. No identifiers, text, file paths or secrets.
#[derive(Clone, Default, Serialize, Deserialize, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Preferences {
    pub dark: bool,
    pub notifications: bool,
    pub save_local: bool,
    #[serde(default)]
    pub report_idle: bool,
    #[serde(default)]
    pub suppress_typing: bool,
}
#[derive(Clone, Default, Serialize, Deserialize, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub version: u8,
    #[serde(default)]
    pub preferences: Option<Preferences>,
    pub drafts: BTreeMap<String, (String, Option<u64>)>,
    pub reads: BTreeMap<String, u64>,
    #[serde(default)]
    pub mentions: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub attention_pending: BTreeMap<String, BTreeMap<u64, bool>>,
    #[serde(default)]
    pub attention_evidence: BTreeMap<String, BTreeMap<u64, (u64, bool)>>,
}
impl Snapshot {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.attention_pending.len() <= 256
                && self
                    .attention_pending
                    .keys()
                    .all(|r| !r.is_empty() && r.len() <= 256)
                && self
                    .attention_pending
                    .values()
                    .map(BTreeMap::len)
                    .sum::<usize>()
                    <= 2000,
            "Pending attention exceeds limits"
        );
        ensure!(
            self.attention_evidence.len() <= 256
                && self
                    .attention_evidence
                    .keys()
                    .all(|r| !r.is_empty() && r.len() <= 256)
                && self
                    .attention_evidence
                    .values()
                    .map(BTreeMap::len)
                    .sum::<usize>()
                    <= 2000,
            "Attention evidence exceeds limits"
        );
        ensure!(self.version == 1, "Unsupported local-state version");
        if self.preferences.as_ref().is_some_and(|p| !p.save_local) {
            ensure!(
                self.drafts.is_empty()
                    && self.reads.is_empty()
                    && self.mentions.is_empty()
                    && self.attention_pending.is_empty()
                    && self.attention_evidence.is_empty(),
                "Private profile data must be absent when storage is disabled"
            );
        }
        ensure!(
            self.drafts.len() <= 256 && self.reads.len() <= 256 && self.mentions.len() <= 256,
            "Too many saved rooms"
        );
        ensure!(
            self.drafts
                .keys()
                .chain(self.reads.keys())
                .all(|r| !r.is_empty() && r.len() <= 256),
            "Invalid saved room ID"
        );
        ensure!(
            self.drafts.values().all(|(text, _)| text.len() <= 32000),
            "A draft exceeds 32,000 bytes; it was not saved"
        );
        ensure!(
            self.drafts
                .values()
                .map(|(text, _)| text.len())
                .sum::<usize>()
                <= DRAFT_LIMIT,
            "Drafts exceed the 256 KiB storage limit; changes were not saved"
        );
        ensure!(
            self.mentions
                .iter()
                .all(|(room, ids)| self.drafts.contains_key(room)
                    && ids.len() <= 32
                    && ids.iter().all(|id| !id.is_empty()
                        && id.len() <= 128
                        && !id.chars().any(char::is_control))),
            "Invalid saved mention recipients"
        );
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profile(pub String);
impl Profile {
    pub fn new(server: &str, user: &str) -> Self {
        let mut hash = Sha256::new();
        hash.update(server);
        hash.update([0]);
        hash.update(user);
        Self(format!("{:x}", hash.finalize()))
    }
}
#[derive(Clone)]
pub struct Backend {
    root: PathBuf,
}
impl Backend {
    pub fn default_location() -> Result<Self> {
        let base = std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
            .context("Local state directory is unavailable")?;
        Ok(Self::at(base.join("apron-gpui")))
    }
    pub fn at(root: PathBuf) -> Self {
        Self { root }
    }
    fn path(&self, p: &Profile) -> Result<PathBuf> {
        ensure!(
            p.0.len() == 64 && p.0.bytes().all(|b| b.is_ascii_hexdigit()),
            "Invalid profile key"
        );
        Ok(self.root.join(format!("{}.json", p.0)))
    }
    fn check_dir(&self) -> Result<bool> {
        let m = match fs::symlink_metadata(&self.root) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(e.into()),
        };
        // Linux-only client; validate ownership and prohibit symlink traversal at the app boundary.
        ensure!(
            m.is_dir()
                && !m.file_type().is_symlink()
                && m.mode() & 0o077 == 0
                && m.uid() == unsafe { libc::geteuid() },
            "Local state directory must be private and owned by you"
        );
        Ok(true)
    }
    pub fn load(&self, p: &Profile) -> Result<Option<Snapshot>> {
        if !self.check_dir()? {
            return Ok(None);
        }
        let f = match OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(self.path(p)?)
        {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        let m = f.metadata()?;
        ensure!(
            m.is_file()
                && m.mode() & 0o077 == 0
                && m.uid() == unsafe { libc::geteuid() }
                && m.len() <= FILE_LIMIT as u64,
            "Local state is not a private bounded regular file"
        );
        let mut bytes = Vec::new();
        f.take((FILE_LIMIT + 1) as u64).read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() <= FILE_LIMIT,
            "Local state exceeds its size limit"
        );
        let state: Snapshot = serde_json::from_slice(&bytes)
            .map_err(|_| anyhow::anyhow!("Local state is corrupt; reset it in Local data"))?;
        state.validate()?;
        Ok(Some(state))
    }
    pub fn save(&self, p: &Profile, state: &Snapshot) -> Result<()> {
        state.validate()?;
        let bytes = serde_json::to_vec(state)?;
        ensure!(
            bytes.len() <= FILE_LIMIT,
            "Encoded local state exceeds 512 KiB"
        );
        if !self.check_dir()? {
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(&self.root)?;
            self.check_dir()?;
        }
        let path = self.path(p)?;
        if fs::symlink_metadata(&path).is_ok() {
            self.load(p)?;
        } else {
            ensure!(
                fs::read_dir(&self.root)?
                    .filter_map(Result::ok)
                    .filter(|e| e.path().extension().is_some_and(|e| e == "json"))
                    .take(16)
                    .count()
                    < 16,
                "Local storage has reached its 16-profile limit"
            );
        }
        let mut tmp = tempfile::NamedTempFile::new_in(&self.root)?;
        tmp.write_all(&bytes)?;
        tmp.as_file().sync_all()?;
        tmp.persist(path)?;
        fs::File::open(&self.root)?.sync_all()?;
        Ok(())
    }
    pub fn forget(&self, p: &Profile) -> Result<()> {
        if !self.check_dir()? {
            return Ok(());
        }
        let path = self.path(p)?;
        match fs::symlink_metadata(&path) {
            Ok(m) => {
                ensure!(
                    m.is_file() && !m.file_type().is_symlink(),
                    "Refusing to reset an unexpected local-state entry"
                );
                fs::remove_file(path)?;
                fs::File::open(&self.root)?.sync_all()?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        };
        Ok(())
    }
}
pub enum Operation {
    Load(Profile),
    Save(Profile, Snapshot),
    Forget(Profile),
}
type Reply = async_channel::Sender<Result<Option<Snapshot>, String>>;
#[derive(Clone)]
pub struct Worker {
    tx: mpsc::SyncSender<(Operation, Reply)>,
}
impl Worker {
    pub fn start(backend: Result<Backend>) -> Self {
        let (tx, rx) = mpsc::sync_channel::<(Operation, Reply)>(8);
        std::thread::Builder::new()
            .name("apron-local-state".into())
            .spawn(move || {
                while let Ok((op, reply)) = rx.recv() {
                    let result = match &backend {
                        Ok(b) => match op {
                            Operation::Load(p) => b.load(&p),
                            Operation::Save(p, s) => b.save(&p, &s).map(|_| None),
                            Operation::Forget(p) => b.forget(&p).map(|_| None),
                        },
                        Err(_) => Err(anyhow::anyhow!("Local storage is unavailable")),
                    }
                    .map_err(|e| e.to_string());
                    let _ = reply.send_blocking(result);
                }
            })
            .expect("local-state worker");
        Self { tx }
    }
    pub fn submit(
        &self,
        op: Operation,
    ) -> Result<async_channel::Receiver<Result<Option<Snapshot>, String>>> {
        let (tx, rx) = async_channel::bounded(1);
        self.tx
            .try_send((op, tx))
            .map_err(|_| anyhow::anyhow!("Local storage is busy; changes are not yet saved"))?;
        Ok(rx)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn durable_preferences_are_private_versioned_and_resettable() {
        let tmp = tempfile::tempdir().unwrap();
        let backend = Backend::at(tmp.path().join("state"));
        let profile = Profile::new("wss://synthetic/ws", "synthetic-user");
        let state = Snapshot {
            version: 1,
            preferences: Some(Preferences {
                dark: true,
                notifications: true,
                suppress_typing: true,
                ..Default::default()
            }),
            ..Default::default()
        };
        backend.save(&profile, &state).unwrap();
        assert_eq!(backend.load(&profile).unwrap(), Some(state.clone()));
        assert!(serde_json::to_vec(&state).unwrap().len() < 1024);
        let mut invalid = state.clone();
        invalid
            .drafts
            .insert("r".into(), ("private text".into(), None));
        assert!(backend.save(&profile, &invalid).is_err());
        invalid = state;
        invalid.version = 2;
        assert!(backend.save(&profile, &invalid).is_err());
        backend.forget(&profile).unwrap();
        assert!(backend.load(&profile).unwrap().is_none());
    }
    #[test]
    fn disabling_storage_erases_queued_private_data_but_keeps_preferences() {
        let tmp = tempfile::tempdir().unwrap();
        let backend = Backend::at(tmp.path().join("state"));
        let profile = Profile::new("synthetic", "user");
        let worker = Worker::start(Ok(backend.clone()));
        let old = Snapshot {
            version: 1,
            drafts: BTreeMap::from([("r".into(), ("synthetic draft".into(), None))]),
            ..Default::default()
        };
        let saved = worker
            .submit(Operation::Save(profile.clone(), old))
            .unwrap();
        let clean = Snapshot {
            version: 1,
            preferences: Some(Preferences {
                dark: true,
                ..Default::default()
            }),
            ..Default::default()
        };
        let disabled = worker
            .submit(Operation::Save(profile.clone(), clean.clone()))
            .unwrap();
        saved.recv_blocking().unwrap().unwrap();
        disabled.recv_blocking().unwrap().unwrap();
        assert_eq!(backend.load(&profile).unwrap(), Some(clean));
    }
    #[test]
    fn opt_in_scoping_reset_and_privacy() {
        let tmp = tempfile::tempdir().unwrap();
        let b = Backend::at(tmp.path().join("state"));
        let p = Profile::new("wss://a/ws", "u");
        assert!(b.load(&p).unwrap().is_none());
        assert!(!b.root.exists());
        let s = Snapshot {
            version: 1,
            preferences: None,
            drafts: BTreeMap::from([("r".into(), ("synthetic draft".into(), Some(3)))]),
            reads: BTreeMap::from([("r".into(), 7)]),
            mentions: BTreeMap::new(),
            attention_evidence: BTreeMap::new(),
            attention_pending: BTreeMap::new(),
        };
        b.save(&p, &s).unwrap();
        assert_eq!(b.load(&p).unwrap(), Some(s));
        assert!(b.load(&Profile::new("wss://b/ws", "u")).unwrap().is_none());
        assert!(b.load(&Profile::new("wss://a/ws", "v")).unwrap().is_none());
        assert_eq!(
            fs::metadata(b.path(&p).unwrap()).unwrap().mode() & 0o777,
            0o600
        );
        b.forget(&p).unwrap();
        assert!(b.load(&p).unwrap().is_none());
    }
    #[test]
    fn rejects_corruption_symlinks_and_bounds() {
        let tmp = tempfile::tempdir().unwrap();
        let b = Backend::at(tmp.path().join("state"));
        let p = Profile::new("a", "u");
        let mut s = Snapshot {
            version: 1,
            ..Default::default()
        };
        b.save(&p, &s).unwrap();
        fs::write(b.path(&p).unwrap(), b"{corrupt").unwrap();
        assert!(b.load(&p).is_err());
        b.forget(&p).unwrap();
        std::os::unix::fs::symlink(tmp.path().join("victim"), b.path(&p).unwrap()).unwrap();
        assert!(b.load(&p).is_err());
        assert!(b.forget(&p).is_err());
        fs::remove_file(b.path(&p).unwrap()).unwrap();
        s.drafts.insert("r".into(), ("x".repeat(32001), None));
        assert!(b.save(&p, &s).is_err());
    }
    #[test]
    fn ordered_reset_cannot_resurrect_saved_state() {
        let tmp = tempfile::tempdir().unwrap();
        let b = Backend::at(tmp.path().join("state"));
        let p = Profile::new("a", "u");
        let w = Worker::start(Ok(b.clone()));
        let saved = w
            .submit(Operation::Save(
                p.clone(),
                Snapshot {
                    version: 1,
                    ..Default::default()
                },
            ))
            .unwrap();
        let reset = w.submit(Operation::Forget(p.clone())).unwrap();
        assert!(saved.recv_blocking().unwrap().is_ok());
        assert!(reset.recv_blocking().unwrap().is_ok());
        assert!(b.load(&p).unwrap().is_none());
    }
}
