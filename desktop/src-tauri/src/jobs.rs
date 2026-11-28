//! Background work the Jobs screen lists: uploads (from preparing the video to the server's
//! compression), imports, Combine, exports, downloads and moving the clips folder.
//!
//! The list lives here, not in the window, so a window that was closed (released to save memory)
//! and opened again still sees every job. Each change is sent as `jobs-changed` with the whole
//! list; finished jobs stay until the user clears them or Klipt quits.
//!
//! A job's `progress` is the whole job, 0..=1. Work with several steps (an upload) gives each step
//! a slice of it ([`Job::step`]); a step that can't be measured (the server finishing up) holds
//! the bar at the start of its slice. The time left is only worked out for measured steps.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::commands::lock;

pub const JOBS_CHANGED: &str = "jobs-changed";

/// Progress changes are sent at most this often per job (a step or state change goes at once).
const EMIT_EVERY: Duration = Duration::from_millis(250);
/// Finished jobs kept at most; older ones drop off first.
const MAX_FINISHED: usize = 200;
/// A measured step reports a time left once it has run this long and moved this far.
const ETA_AFTER: Duration = Duration::from_secs(2);
const ETA_MIN_MOVED: f64 = 0.02;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum JobKind {
    Upload,
    Import,
    Combine,
    Export,
    Download,
    Move,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum JobState {
    Running,
    Done,
    Failed,
    Cancelled,
}

/// How much of the current step is done, for the line under the bar.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "unit")]
pub enum Amount {
    /// Bytes; `rate` in bytes per second once known.
    Bytes { done: u64, total: u64, rate: Option<f64> },
    /// Files or clips.
    Items { done: u64, total: u64 },
}

/// A job as the window shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobView {
    pub id: u64,
    pub kind: JobKind,
    pub title: String,
    /// The library clip it works on, if any.
    pub clip_id: Option<String>,
    pub state: JobState,
    /// What it's doing now ("Uploading"), or how it ended.
    pub step: String,
    /// The whole job, 0..=1.
    pub progress: f64,
    pub eta_secs: Option<f64>,
    pub amount: Option<Amount>,
    /// The error of a failed job, or a short result ("Imported 12 clips").
    pub message: Option<String>,
    /// Unix ms.
    pub started_at_ms: i64,
    pub finished_at_ms: Option<i64>,
}

struct Entry {
    view: JobView,
    /// The slice of `progress` the current step fills.
    range: (f64, f64),
    measured: bool,
    step_started: Instant,
    /// The step's own fraction when it started (an upload resumed halfway starts at 0.5).
    step_from: Option<f64>,
    /// An estimate the work gave itself (an import knows its files better than a straight line).
    own_eta: Option<f64>,
    bytes_from: Option<u64>,
    last_emit: Option<Instant>,
}

impl Entry {
    fn set_fraction(&mut self, fraction: f64) {
        let fraction = if fraction.is_finite() { fraction.clamp(0.0, 1.0) } else { 0.0 };
        let (from, to) = self.range;
        self.view.progress = from + (to - from) * fraction;
        let start = *self.step_from.get_or_insert(fraction);
        self.view.eta_secs = self.own_eta.or_else(|| eta(self.measured, self.step_started.elapsed(), start, fraction));
    }
}

/// Seconds left in a step, from how fast it moved so far; None before it says enough.
fn eta(measured: bool, elapsed: Duration, from: f64, now: f64) -> Option<f64> {
    let moved = now - from;
    if !measured || elapsed < ETA_AFTER || moved < ETA_MIN_MOVED {
        return None;
    }
    Some(elapsed.as_secs_f64() * (1.0 - now) / moved)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// Every job since Klipt started (managed state).
#[derive(Default, Clone)]
pub struct JobList(Arc<Mutex<Inner>>);

#[derive(Default)]
struct Inner {
    next_id: u64,
    entries: Vec<Entry>,
}

impl JobList {
    pub fn views(&self) -> Vec<JobView> {
        lock(&self.0).entries.iter().map(|e| e.view.clone()).collect()
    }

    fn add(&self, kind: JobKind, title: &str, clip_id: Option<&str>) -> u64 {
        let mut inner = lock(&self.0);
        inner.next_id += 1;
        let id = inner.next_id;
        inner.entries.push(Entry {
            view: JobView {
                id,
                kind,
                title: title.to_string(),
                clip_id: clip_id.map(str::to_string),
                state: JobState::Running,
                step: "Starting".into(),
                progress: 0.0,
                eta_secs: None,
                amount: None,
                message: None,
                started_at_ms: now_ms(),
                finished_at_ms: None,
            },
            range: (0.0, 1.0),
            measured: true,
            step_started: Instant::now(),
            step_from: None,
            own_eta: None,
            bytes_from: None,
            last_emit: None,
        });
        let finished = inner.entries.iter().filter(|e| e.view.state != JobState::Running).count();
        if finished > MAX_FINISHED {
            let mut drop = finished - MAX_FINISHED;
            inner.entries.retain(|e| {
                let old = drop > 0 && e.view.state != JobState::Running;
                drop -= usize::from(old);
                !old
            });
        }
        id
    }

    /// Changes job `id`; true when the change should be sent now (`force`, or the last send was a
    /// while ago).
    fn update(&self, id: u64, force: bool, change: impl FnOnce(&mut Entry)) -> bool {
        let mut inner = lock(&self.0);
        let Some(entry) = inner.entries.iter_mut().find(|e| e.view.id == id) else { return false };
        if entry.view.state != JobState::Running {
            return false;
        }
        change(entry);
        let due = force || entry.last_emit.is_none_or(|at| at.elapsed() >= EMIT_EVERY);
        if due {
            entry.last_emit = Some(Instant::now());
        }
        due
    }

    /// Drops the finished jobs.
    pub fn clear_finished(&self) {
        lock(&self.0).entries.retain(|e| e.view.state == JobState::Running);
    }
}

/// Reports on one job; cheap to clone into the threads and callbacks of the work.
#[derive(Clone)]
pub struct Reporter {
    app: Option<AppHandle>,
    list: JobList,
    id: u64,
}

impl Reporter {
    pub fn id(&self) -> u64 {
        self.id
    }

    fn send(&self) {
        if let Some(app) = &self.app {
            if let Err(e) = app.emit(JOBS_CHANGED, self.list.views()) {
                crate::elog!("klipt: can't send {JOBS_CHANGED} to the window: {e}");
            }
        }
    }

    fn change(&self, force: bool, change: impl FnOnce(&mut Entry)) {
        if self.list.update(self.id, force, change) {
            self.send();
        }
    }

    /// Starts a step that fills `from..to` of the bar. `measured`: it reports progress (and so gets
    /// a time left); otherwise the bar waits at `from` until the next step.
    pub fn step(&self, label: &str, from: f64, to: f64, measured: bool) {
        self.change(true, |e| {
            e.view.step = label.to_string();
            e.range = (from, to.max(from));
            e.measured = measured;
            e.step_started = Instant::now();
            e.own_eta = None;
            e.bytes_from = None;
            e.view.amount = None;
            e.set_fraction(0.0);
            e.step_from = None;
        });
    }

    /// How far the current step is, 0..=1.
    pub fn progress(&self, fraction: f64) {
        self.change(false, |e| e.set_fraction(fraction));
    }

    /// Like [`Reporter::progress`], with the work's own estimate of the seconds left.
    pub fn progress_eta(&self, fraction: f64, eta_secs: Option<f64>) {
        self.change(false, |e| {
            e.own_eta = eta_secs.filter(|s| s.is_finite() && *s >= 0.0);
            e.set_fraction(fraction);
        });
    }

    /// Bytes of the step done so far (sets the progress and the speed too).
    pub fn bytes(&self, done: u64, total: u64) {
        self.change(false, |e| {
            let from = *e.bytes_from.get_or_insert(done);
            let secs = e.step_started.elapsed().as_secs_f64();
            let rate = (secs >= 1.0 && done > from).then(|| (done - from) as f64 / secs);
            e.view.amount = Some(Amount::Bytes { done, total, rate });
            e.set_fraction(if total > 0 { done as f64 / total as f64 } else { 0.0 });
        });
    }

    /// Items of the step done so far (shown as "3 of 40"; the progress is set separately).
    pub fn items(&self, done: u64, total: u64) {
        self.change(false, |e| e.view.amount = Some(Amount::Items { done, total }));
    }

    fn finish(&self, state: JobState, step: &str, message: Option<String>) {
        self.change(true, |e| {
            e.view.state = state;
            e.view.step = step.to_string();
            e.view.message = message;
            e.view.eta_secs = None;
            e.view.finished_at_ms = Some(now_ms());
            if state == JobState::Done {
                e.view.progress = 1.0;
                if let Some(Amount::Bytes { total, rate, .. }) = e.view.amount {
                    e.view.amount = Some(Amount::Bytes { done: total, total, rate });
                }
            }
        });
    }
}

/// One job, owned by the work it describes. Dropping it while it still runs marks it failed, so a
/// job never stays "running" after its work is gone (a panic, an early return).
pub struct Job(Reporter);

impl std::ops::Deref for Job {
    type Target = Reporter;
    fn deref(&self) -> &Reporter {
        &self.0
    }
}

impl Job {
    /// Adds a running job and tells the window.
    pub fn start(app: &AppHandle, kind: JobKind, title: &str, clip_id: Option<&str>) -> Job {
        let list = app.state::<JobList>().inner().clone();
        let id = list.add(kind, title, clip_id);
        let job = Job(Reporter { app: Some(app.clone()), list, id });
        job.send();
        job
    }

    #[cfg(test)]
    fn detached(list: &JobList, kind: JobKind, title: &str) -> Job {
        Job(Reporter { app: None, list: list.clone(), id: list.add(kind, title, None) })
    }

    pub fn reporter(&self) -> Reporter {
        self.0.clone()
    }

    pub fn done(self, message: Option<String>) {
        self.finish(JobState::Done, "Done", message);
    }

    pub fn failed(self, message: impl Into<String>) {
        self.finish(JobState::Failed, "Failed", Some(message.into()));
    }

    pub fn cancelled(self, message: Option<String>) {
        self.finish(JobState::Cancelled, "Cancelled", message);
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        // A finished job ignores this (only running jobs change).
        self.finish(JobState::Failed, "Failed", Some("It stopped unexpectedly.".into()));
    }
}

#[tauri::command]
pub fn list_jobs(jobs: tauri::State<'_, JobList>) -> Vec<JobView> {
    jobs.views()
}

#[tauri::command]
pub fn clear_finished_jobs(app: AppHandle) -> Vec<JobView> {
    let jobs = app.state::<JobList>();
    jobs.clear_finished();
    let views = jobs.views();
    if let Err(e) = app.emit(JOBS_CHANGED, &views) {
        crate::elog!("klipt: can't send {JOBS_CHANGED} to the window: {e}");
    }
    views
}

#[cfg(test)]
mod tests {
    use super::*;

    fn only(list: &JobList) -> JobView {
        let views = list.views();
        assert_eq!(views.len(), 1);
        views[0].clone()
    }

    #[test]
    fn steps_fill_their_slice_of_the_bar() {
        let list = JobList::default();
        let job = Job::detached(&list, JobKind::Upload, "ace");
        job.step("Preparing video", 0.0, 0.4, true);
        job.progress(0.5);
        assert!((only(&list).progress - 0.2).abs() < 1e-9);
        job.step("Finishing up", 0.9, 0.95, false);
        let v = only(&list);
        assert_eq!(v.step, "Finishing up");
        assert!((v.progress - 0.9).abs() < 1e-9);
        assert_eq!(v.eta_secs, None);
        job.done(None);
        let v = only(&list);
        assert_eq!((v.state, v.progress), (JobState::Done, 1.0));
        assert!(v.finished_at_ms.is_some());
    }

    #[test]
    fn a_dropped_running_job_ends_failed_and_a_finished_one_stays_as_it_ended() {
        let list = JobList::default();
        drop(Job::detached(&list, JobKind::Combine, "a"));
        assert_eq!(only(&list).state, JobState::Failed);
        list.clear_finished();
        assert!(list.views().is_empty());

        Job::detached(&list, JobKind::Export, "b").cancelled(None);
        assert_eq!(only(&list).state, JobState::Cancelled);
    }

    #[test]
    fn clearing_keeps_running_jobs() {
        let list = JobList::default();
        let running = Job::detached(&list, JobKind::Import, "a");
        Job::detached(&list, JobKind::Export, "b").done(None);
        list.clear_finished();
        assert_eq!(only(&list).id, running.id());
    }

    #[test]
    fn old_finished_jobs_drop_off() {
        let list = JobList::default();
        let first = Job::detached(&list, JobKind::Export, "first");
        let first_id = first.id();
        first.done(None);
        for _ in 0..MAX_FINISHED {
            Job::detached(&list, JobKind::Export, "x").done(None);
        }
        let _running = Job::detached(&list, JobKind::Export, "running");
        let views = list.views();
        assert_eq!(views.len(), MAX_FINISHED + 1);
        assert!(views.iter().all(|v| v.id != first_id));
    }

    #[test]
    fn time_left_comes_from_how_fast_the_step_moved() {
        assert_eq!(eta(true, Duration::from_secs(1), 0.0, 0.5), None, "too early");
        assert_eq!(eta(false, Duration::from_secs(10), 0.0, 0.5), None, "not measured");
        assert_eq!(eta(true, Duration::from_secs(10), 0.5, 0.505), None, "barely moved");
        // Resumed at 50%, now 75% after 10 s: 25% per 10 s, 25% left.
        assert_eq!(eta(true, Duration::from_secs(10), 0.5, 0.75), Some(10.0));
    }

    #[test]
    fn bytes_set_the_progress_and_finish_full() {
        let list = JobList::default();
        let job = Job::detached(&list, JobKind::Download, "a");
        job.step("Downloading", 0.0, 1.0, true);
        job.bytes(25, 100);
        let v = only(&list);
        assert!((v.progress - 0.25).abs() < 1e-9);
        assert_eq!(v.amount, Some(Amount::Bytes { done: 25, total: 100, rate: None }));
        job.done(None);
        assert_eq!(only(&list).amount, Some(Amount::Bytes { done: 100, total: 100, rate: None }));
    }
}
