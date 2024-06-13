use std::ops::Add;
use std::time::{Duration, SystemTime};
use std::{cell::Cell, ffi::OsStr, fs::Metadata, ops::Deref};

use anyhow::{anyhow, Result};
use tokio::fs;

use crate::fs::SCHEMES;
use crate::{
	fs::{Cha, ChaKind, Url},
	theme::IconCache,
};

#[derive(Clone, Debug, Default)]
pub struct File {
	pub url: Url,
	pub cha: Cha,
	pub link_to: Option<Url>,
	pub icon: Cell<IconCache>,
}

impl Deref for File {
	type Target = Cha;

	#[inline]
	fn deref(&self) -> &Self::Target {
		&self.cha
	}
}

impl File {
	#[inline]
	pub async fn from(url: Url) -> Result<Self> {
		if url.is_remote().is_some() {
			return Self::from_remote(url).await;
		}

		let meta = fs::symlink_metadata(&url).await?;
		Ok(Self::from_meta(url, meta).await)
	}

	pub async fn from_meta(url: Url, mut meta: Metadata) -> Self {
		let mut ck = ChaKind::empty();
		let (is_link, mut link_to) = (meta.is_symlink(), None);

		if is_link {
			meta = fs::metadata(&url).await.unwrap_or(meta);
			link_to = fs::read_link(&url).await.map(Url::from).ok();
		}

		if is_link && meta.is_symlink() {
			ck |= ChaKind::ORPHAN;
		} else if is_link {
			ck |= ChaKind::LINK;
		}

		#[cfg(unix)]
		if url.is_hidden() {
			ck |= ChaKind::HIDDEN;
		}
		#[cfg(windows)]
		{
			use std::os::windows::fs::MetadataExt;
			if meta.file_attributes() & 2 != 0 {
				ck |= ChaKind::HIDDEN;
			}
		}

		Self { url, cha: Cha::from(meta).with_kind(ck), link_to, icon: Default::default() }
	}

	/// Build a new file from remote.
	pub async fn from_remote(url: Url) -> Result<Self> {
		let scheme = url.is_remote().ok_or(anyhow!("not a remote file"))?;
		let op = SCHEMES.get(scheme)?;

		let meta = op.stat(&url.as_path().to_string_lossy()).await?;
		let mut kind = ChaKind::default();
		if meta.is_dir() {
			kind |= ChaKind::DIR;
		}
		let cha = Cha {
			kind,
			len: meta.content_length(),
			accessed: None,
			created: None,
			modified: meta
				.last_modified()
				.map(|v| SystemTime::UNIX_EPOCH.add(Duration::from_micros(v.timestamp_micros() as u64))),
			// Always return 774 for remote files.
			#[cfg(unix)]
			permissions: 0774,
			// Always return current user for remote files.
			#[cfg(unix)]
			uid: unsafe { libc::getuid().into() },
			// Always return current group for remote files.
			#[cfg(unix)]
			gid: unsafe { libc::getgid().into() },
		};

		Ok(Self { url, cha, link_to: None, icon: Default::default() })
	}

	#[inline]
	pub fn from_dummy(url: &Url) -> Self {
		Self { url: url.to_owned(), ..Default::default() }
	}
}

impl File {
	// --- Url
	#[inline]
	pub fn url(&self) -> Url {
		self.url.clone()
	}

	#[inline]
	pub fn name(&self) -> Option<&OsStr> {
		self.url.file_name()
	}

	#[inline]
	pub fn stem(&self) -> Option<&OsStr> {
		self.url.file_stem()
	}

	#[inline]
	pub fn parent(&self) -> Option<Url> {
		self.url.parent_url()
	}
}
