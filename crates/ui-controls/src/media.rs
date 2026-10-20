//! Nonblocking retained media host with a bounded latest-request/frame mailbox.
use rust_desktop_ui_core::Bitmap;
pub use rust_desktop_ui_core::media::*;
use std::{
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
mod view;
pub use view::*;
pub type MediaFactory =
    Arc<dyn Fn(&AtomicBool) -> Result<Box<dyn MediaProvider>, MediaError> + Send + Sync>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaState {
    Empty,
    Opening,
    Paused,
    Playing,
    Stopped,
    Ended,
    Error,
}
#[derive(Clone, Copy)]
enum Request {
    Frame(u64, Duration),
    Volume(u64, f32),
}
enum Reply {
    Frame(u64, Result<Option<MediaFrame>, MediaError>),
    Volume(u64, f32, Result<(), MediaError>),
}
#[derive(Default)]
struct Mailbox {
    request: Option<Request>,
    reply: Option<Reply>,
    info: Option<Result<MediaInfo, MediaError>>,
    volume: Option<f32>,
    volume_reply: Option<f32>,
}
struct Worker {
    cancel: Arc<AtomicBool>,
    shared: Arc<(Mutex<Mailbox>, Condvar)>,
    thread: std::thread::JoinHandle<()>,
}
impl Worker {
    fn new(factory: MediaFactory) -> Result<Self, MediaError> {
        let cancel = Arc::new(AtomicBool::new(false));
        let shared = Arc::new((Mutex::new(Mailbox::default()), Condvar::new()));
        let c = cancel.clone();
        let s = shared.clone();
        let thread = std::thread::Builder::new()
            .name("ui-media-provider".into())
            .spawn(move || {
                // Provider failures are contained and the provider drops on this thread.
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let mut provider = match factory(&c) {
                        Ok(p) => p,
                        Err(e) => {
                            s.0.lock().unwrap().info = Some(Err(e));
                            return;
                        }
                    };
                    if c.load(Ordering::Acquire) {
                        return;
                    }
                    s.0.lock().unwrap().info = Some(Ok(provider.info()));
                    loop {
                        let request = {
                            let mut mailbox = s.0.lock().unwrap();
                            while mailbox.request.is_none()
                                && mailbox.volume.is_none()
                                && !c.load(Ordering::Acquire)
                            {
                                mailbox = s.1.wait(mailbox).unwrap();
                            }
                            if c.load(Ordering::Acquire) {
                                break;
                            }
                            if let Some(v) = mailbox.volume.take() {
                                Request::Volume(0, v)
                            } else {
                                mailbox.request.take().unwrap()
                            }
                        };
                        let reply = match request {
                            Request::Frame(token, time) => {
                                Reply::Frame(token, provider.frame_at(time, &c))
                            }
                            Request::Volume(token, v) => {
                                Reply::Volume(token, v, provider.set_volume(v))
                            }
                        };
                        if c.load(Ordering::Acquire) {
                            break;
                        }
                        match &reply {
                            Reply::Frame(_, Err(e)) | Reply::Volume(_, _, Err(e)) => {
                                // Failure terminates this provider even when the request
                                // was superseded. Never leave a newer request waiting forever.
                                s.0.lock().unwrap().info = Some(Err(e.clone()));
                                break;
                            }
                            Reply::Volume(_, v, Ok(())) => {
                                s.0.lock().unwrap().volume_reply = Some(*v)
                            }
                            _ => s.0.lock().unwrap().reply = Some(reply),
                        }
                    }
                }));
                if outcome.is_err()
                    && let Ok(mut m) = s.0.lock()
                {
                    m.info = Some(Err(MediaError("Media provider panicked".into())));
                }
            })
            .map_err(|e| MediaError(e.to_string()))?;
        Ok(Self {
            cancel,
            shared,
            thread,
        })
    }
    fn request(&self, r: Request) {
        let mut m = self.shared.0.lock().unwrap();
        m.request = Some(r);
        m.reply = None;
        self.shared.1.notify_one();
    }
    fn volume(&self, v: f32) {
        let mut m = self.shared.0.lock().unwrap();
        m.volume = Some(v);
        self.shared.1.notify_one();
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
        self.shared.1.notify_all();
    }
}

pub struct MediaSession {
    worker: Option<Worker>,
    retiring: Vec<Worker>,
    factory: Option<MediaFactory>,
    state: MediaState,
    info: Option<MediaInfo>,
    frame: Option<MediaFrame>,
    position: Duration,
    anchor: Instant,
    anchor_position: Duration,
    token: u64,
    pending: Option<u64>,
    error: Option<MediaError>,
    volume: f32,
    autoplay: bool,
}
impl Default for MediaSession {
    fn default() -> Self {
        Self {
            worker: None,
            retiring: Vec::new(),
            factory: None,
            state: MediaState::Empty,
            info: None,
            frame: None,
            position: Duration::ZERO,
            anchor: Instant::now(),
            anchor_position: Duration::ZERO,
            token: 0,
            pending: None,
            error: None,
            volume: 1.0,
            autoplay: false,
        }
    }
}
impl MediaSession {
    fn retire(&mut self) {
        self.retiring.retain(|w| !w.thread.is_finished());
        if let Some(worker) = self.worker.take() {
            worker.cancel.store(true, Ordering::Release);
            worker.shared.1.notify_all();
            self.retiring.push(worker);
        }
    }
    pub fn state(&self) -> MediaState {
        self.state
    }
    pub fn info(&self) -> Option<&MediaInfo> {
        self.info.as_ref()
    }
    pub fn frame(&self) -> Option<&MediaFrame> {
        self.frame.as_ref()
    }
    pub fn position(&self) -> Duration {
        self.position
    }
    pub fn error(&self) -> Option<&MediaError> {
        self.error.as_ref()
    }
    pub fn volume(&self) -> f32 {
        self.volume
    }
    fn advance(&mut self) -> Result<u64, MediaError> {
        self.token = self
            .token
            .checked_add(1)
            .ok_or_else(|| MediaError("Media generation exhausted".into()))?;
        Ok(self.token)
    }
    pub fn open(&mut self, factory: MediaFactory) -> Result<(), MediaError> {
        self.retiring.retain(|w| !w.thread.is_finished());
        if !self.retiring.is_empty() {
            return Err(MediaError("Previous provider is still closing".into()));
        }
        let worker = Worker::new(factory.clone())?;
        self.advance()?;
        self.retire();
        self.worker = Some(worker);
        self.factory = Some(factory);
        self.pending = None;
        self.state = MediaState::Opening;
        self.info = None;
        self.frame = None;
        self.position = Duration::ZERO;
        self.error = None;
        self.autoplay = false;
        Ok(())
    }
    pub fn play(&mut self, now: Instant) -> Result<(), MediaError> {
        if matches!(self.state, MediaState::Stopped | MediaState::Ended) {
            let factory = self
                .factory
                .clone()
                .ok_or_else(|| MediaError("No media source".into()))?;
            self.open(factory)?;
            self.autoplay = true;
            return Ok(());
        }
        if self.state == MediaState::Opening {
            self.autoplay = true;
            return Ok(());
        }
        if !matches!(self.state, MediaState::Playing | MediaState::Paused) {
            return Err(MediaError("No playable source".into()));
        }
        if self.state != MediaState::Playing {
            self.anchor = now;
            self.anchor_position = self.position;
            self.state = MediaState::Playing;
        }
        Ok(())
    }
    pub fn pause(&mut self, now: Instant) -> Result<(), MediaError> {
        self.autoplay = false;
        if self.state == MediaState::Playing {
            self.position = self.current_position(now);
            self.state = MediaState::Paused;
            self.request_frame(self.position)?;
        }
        Ok(())
    }
    /// Stops and releases the worker/provider. The source factory permits explicit replay.
    pub fn stop(&mut self) {
        self.retire();
        self.pending = None;
        self.frame = None;
        self.position = Duration::ZERO;
        self.state = MediaState::Stopped;
        self.autoplay = false;
    }
    pub fn close(&mut self) {
        self.stop();
        self.factory = None;
        self.info = None;
        self.error = None;
        self.state = MediaState::Empty;
    }
    pub fn seek(&mut self, position: Duration, now: Instant) -> Result<(), MediaError> {
        let info = self
            .info
            .as_ref()
            .ok_or_else(|| MediaError("Media is not ready".into()))?;
        if !info.capabilities.seek
            || info.duration.is_some_and(|d| position >= d)
            || !matches!(self.state, MediaState::Paused | MediaState::Playing)
        {
            return Err(MediaError("Seek unavailable or outside duration".into()));
        }
        self.request_frame(position)?;
        self.position = position;
        self.anchor = now;
        self.anchor_position = position;
        Ok(())
    }
    pub fn set_volume(&mut self, volume: f32) -> Result<(), MediaError> {
        if !volume.is_finite()
            || !(0.0..=1.0).contains(&volume)
            || !self.info.as_ref().is_some_and(|i| i.capabilities.volume)
            || self.worker.is_none()
        {
            return Err(MediaError("Volume unavailable or invalid".into()));
        }
        self.worker.as_ref().unwrap().volume(volume);

        Ok(())
    }
    /// Returns an immutable copy of the last displayed frame; saving is explicit host I/O.
    pub fn capture(&self) -> Result<Bitmap, MediaError> {
        if !self.info.as_ref().is_some_and(|i| i.capabilities.capture) {
            return Err(MediaError("Capture unavailable".into()));
        }
        self.frame
            .as_ref()
            .map(|f| f.bitmap.clone())
            .ok_or_else(|| MediaError("No displayed frame".into()))
    }
    fn request_frame(&mut self, time: Duration) -> Result<(), MediaError> {
        let token = self.advance()?;
        let worker = self
            .worker
            .as_ref()
            .ok_or_else(|| MediaError("Media is stopped".into()))?;
        self.pending = Some(token);
        worker.request(Request::Frame(token, time));
        Ok(())
    }
    fn current_position(&self, now: Instant) -> Duration {
        let p = self
            .anchor_position
            .saturating_add(now.saturating_duration_since(self.anchor));
        self.info
            .as_ref()
            .and_then(|i| i.duration)
            .map_or(p, |d| p.min(d))
    }
    fn fail(&mut self, error: MediaError) {
        self.retire();
        self.pending = None;
        self.frame = None;
        self.state = MediaState::Error;
        self.error = Some(error);
    }
    /// Poll on the UI thread; never waits on decoding or joins a provider thread.
    pub fn poll(&mut self, now: Instant) -> Result<bool, MediaError> {
        self.retiring.retain(|w| !w.thread.is_finished());
        let Some(worker) = &self.worker else {
            return Ok(false);
        };
        let (info, reply, volume) = {
            let mut m = worker.shared.0.lock().unwrap();
            (m.info.take(), m.reply.take(), m.volume_reply.take())
        };
        let mut changed = false;
        if let Some(v) = volume {
            self.volume = v;
            changed = true;
        }
        if let Some(info) = info {
            match info {
                Err(e) => {
                    self.fail(e);
                    return Ok(true);
                }
                Ok(i) => {
                    if i.title.len() > 1024
                        || i.title.chars().any(char::is_control)
                        || i.duration
                            .is_some_and(|d| d.is_zero() || d > Duration::from_secs(7 * 24 * 3600))
                    {
                        self.fail(MediaError("Invalid provider metadata".into()));
                        return Ok(true);
                    }
                    self.info = Some(i);
                    self.state = MediaState::Paused;
                    self.request_frame(Duration::ZERO)?;
                    if self.autoplay {
                        self.play(now)?;
                    }
                    changed = true;
                }
            }
        }
        if let Some(reply) = reply {
            let token = match &reply {
                Reply::Frame(t, _) | Reply::Volume(t, _, _) => *t,
            };
            if self.pending == Some(token) {
                self.pending = None;
                changed = true;
                match reply {
                    Reply::Frame(_, Ok(Some(frame))) => {
                        if self
                            .info
                            .as_ref()
                            .and_then(|i| i.duration)
                            .is_some_and(|d| frame.position > d)
                        {
                            self.fail(MediaError("Frame timestamp outside source".into()));
                            return Ok(true);
                        }
                        self.frame = Some(frame);
                    }
                    Reply::Frame(_, Ok(None)) => {
                        self.retire();
                        self.state = MediaState::Ended;
                    }
                    Reply::Frame(_, Err(e)) | Reply::Volume(_, _, Err(e)) => {
                        self.fail(e);
                        return Ok(true);
                    }
                    Reply::Volume(_, volume, Ok(())) => {
                        self.volume = volume;
                    }
                }
            }
        }
        if self.state == MediaState::Playing {
            let p = self.current_position(now);
            self.position = p;
            if self
                .info
                .as_ref()
                .and_then(|i| i.duration)
                .is_some_and(|d| p >= d)
            {
                self.retire();
                self.pending = None;
                self.state = MediaState::Ended;
                return Ok(true);
            }
            if self.pending.is_none()
                && self
                    .frame
                    .as_ref()
                    .is_none_or(|f| p.saturating_sub(f.position) >= Duration::from_millis(33))
            {
                self.request_frame(p)?;
            }
        }
        Ok(changed)
    }
}
