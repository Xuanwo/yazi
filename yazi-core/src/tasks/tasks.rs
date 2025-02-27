use std::{collections::{BTreeMap, HashMap, HashSet}, ffi::OsStr, path::Path, sync::Arc, time::Duration};

use tokio::time::sleep;
use tracing::debug;
use yazi_config::{manager::SortBy, open::Opener, popup::InputCfg, OPEN};
use yazi_scheduler::{Scheduler, TaskSummary};
use yazi_shared::{fs::{File, Url}, term::Term, MimeKind};

use super::{TasksProgress, TASKS_PADDING, TASKS_PERCENT};
use crate::{files::Files, input::Input};

pub struct Tasks {
	pub(super) scheduler: Arc<Scheduler>,

	pub visible:  bool,
	pub cursor:   usize,
	pub progress: TasksProgress,
}

impl Tasks {
	pub fn start() -> Self {
		let tasks = Self {
			scheduler: Arc::new(Scheduler::start()),
			visible:   false,
			cursor:    0,
			progress:  Default::default(),
		};

		let running = tasks.scheduler.running.clone();
		tokio::spawn(async move {
			let mut last = TasksProgress::default();
			loop {
				sleep(Duration::from_millis(500)).await;

				let new = TasksProgress::from(&*running.read());
				if last != new {
					last = new;
					Tasks::_update(new);
				}
			}
		});

		tasks
	}

	#[inline]
	pub fn limit() -> usize {
		(Term::size().rows * TASKS_PERCENT / 100).saturating_sub(TASKS_PADDING) as usize
	}

	pub fn paginate(&self) -> Vec<TaskSummary> {
		let running = self.scheduler.running.read();
		running.values().take(Self::limit()).map(Into::into).collect()
	}

	pub fn file_open(&self, targets: &[(impl AsRef<Path>, impl AsRef<str>)]) -> bool {
		let mut openers = BTreeMap::new();
		for (path, mime) in targets {
			if let Some(opener) = OPEN.openers(path, mime).and_then(|o| o.first().copied()) {
				openers.entry(opener).or_insert_with(Vec::new).push(path.as_ref().as_os_str());
			}
		}
		for (opener, args) in openers {
			self.file_open_with(opener, &args);
		}
		false
	}

	pub fn file_open_with(&self, opener: &Opener, args: &[impl AsRef<OsStr>]) -> bool {
		if opener.spread {
			self.scheduler.process_open(opener, args);
			return false;
		}
		for target in args {
			self.scheduler.process_open(opener, &[target]);
		}
		false
	}

	pub fn file_cut(&self, src: &HashSet<Url>, dest: &Url, force: bool) -> bool {
		for u in src {
			let to = dest.join(u.file_name().unwrap());
			if force && u == &to {
				debug!("file_cut: same file, skipping {:?}", to);
			} else {
				self.scheduler.file_cut(u.clone(), to, force);
			}
		}
		false
	}

	pub fn file_copy(&self, src: &HashSet<Url>, dest: &Url, force: bool) -> bool {
		for u in src {
			let to = dest.join(u.file_name().unwrap());
			if force && u == &to {
				debug!("file_copy: same file, skipping {:?}", to);
			} else {
				self.scheduler.file_copy(u.clone(), to, force);
			}
		}
		false
	}

	pub fn file_link(&self, src: &HashSet<Url>, dest: &Url, relative: bool, force: bool) -> bool {
		for u in src {
			let to = dest.join(u.file_name().unwrap());
			if force && *u == to {
				debug!("file_link: same file, skipping {:?}", to);
			} else {
				self.scheduler.file_link(u.clone(), to, relative, force);
			}
		}
		false
	}

	pub fn file_remove(&self, targets: Vec<Url>, force: bool, permanently: bool) -> bool {
		if force {
			for u in targets {
				if permanently {
					self.scheduler.file_delete(u);
				} else {
					self.scheduler.file_trash(u);
				}
			}
			return false;
		}

		let scheduler = self.scheduler.clone();
		tokio::spawn(async move {
			let mut result = Input::_show(if permanently {
				InputCfg::delete(targets.len())
			} else {
				InputCfg::trash(targets.len())
			});

			if let Some(Ok(choice)) = result.recv().await {
				if choice != "y" && choice != "Y" {
					return;
				}
				for u in targets {
					if permanently {
						scheduler.file_delete(u);
					} else {
						scheduler.file_trash(u);
					}
				}
			}
		});
		false
	}

	#[inline]
	pub fn precache_size(&self, targets: &Files) -> bool {
		if targets.sorter().by != SortBy::Size {
			return false;
		}

		let targets: Vec<_> = targets
			.iter()
			.filter(|f| f.is_dir() && !targets.sizes.contains_key(&f.url))
			.map(|f| &f.url)
			.collect();

		if !targets.is_empty() {
			self.scheduler.precache_size(targets);
		}

		false
	}

	#[inline]
	pub fn precache_mime(&self, targets: &[File], mimetype: &HashMap<Url, String>) -> bool {
		let targets: Vec<_> = targets
			.iter()
			.filter(|f| !f.is_dir() && !mimetype.contains_key(&f.url))
			.map(|f| f.url())
			.collect();

		if !targets.is_empty() {
			self.scheduler.precache_mime(targets);
		}
		false
	}

	pub fn precache_image(&self, mimetype: &BTreeMap<Url, String>) -> bool {
		let targets: Vec<_> = mimetype
			.iter()
			.filter(|(_, m)| MimeKind::new(m) == MimeKind::Image)
			.map(|(u, _)| u.clone())
			.collect();

		if !targets.is_empty() {
			self.scheduler.precache_image(targets);
		}
		false
	}

	pub fn precache_video(&self, mimetype: &BTreeMap<Url, String>) -> bool {
		let targets: Vec<_> = mimetype
			.iter()
			.filter(|(_, m)| MimeKind::new(m) == MimeKind::Video)
			.map(|(u, _)| u.clone())
			.collect();

		if !targets.is_empty() {
			self.scheduler.precache_video(targets);
		}
		false
	}

	pub fn precache_pdf(&self, mimetype: &BTreeMap<Url, String>) -> bool {
		let targets: Vec<_> = mimetype
			.iter()
			.filter(|(_, m)| MimeKind::new(m) == MimeKind::PDF)
			.map(|(u, _)| u.clone())
			.collect();

		if !targets.is_empty() {
			self.scheduler.precache_pdf(targets);
		}
		false
	}
}

impl Tasks {
	#[inline]
	pub fn len(&self) -> usize { self.scheduler.running.read().len() }
}
