use ort::session::Session;
use parking_lot::Mutex;
use std::{
    ops::{Deref, DerefMut},
    sync::Arc,
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

#[derive(Debug)]
pub struct SessionPool {
    sem: Arc<Semaphore>,
    sessions: Mutex<Vec<Session>>,
}

impl SessionPool {
    pub fn new(sessions: Vec<Session>) -> Self {
        let n = sessions.len();
        Self {
            sem: Arc::new(Semaphore::new(n)),
            sessions: Mutex::new(sessions),
        }
    }

    pub async fn acquire(&self) -> SessionHandle<'_> {
        let permit = self
            .sem
            .clone()
            .acquire_owned()
            .await
            .expect("semaphore closed");
        let session = self
            .sessions
            .lock()
            .pop()
            .expect("semaphore permits guarantee a session is available");
        SessionHandle {
            pool: self,
            session: Some(session),
            _permit: permit,
        }
    }

    pub fn end_profiling(&self) {
        let mut sessions = self.sessions.lock();
        for session in sessions.iter_mut() {
            session.end_profiling().unwrap();
        }
    }
}

impl Drop for SessionPool {
    fn drop(&mut self) {
        let sessions = self.sessions.get_mut();
        for session in sessions {
            session.end_profiling().unwrap();
        }
    }
}

pub struct SessionHandle<'a> {
    pool: &'a SessionPool,
    session: Option<Session>,
    _permit: OwnedSemaphorePermit,
}

impl Deref for SessionHandle<'_> {
    type Target = Session;
    fn deref(&self) -> &Self::Target {
        self.session.as_ref().unwrap()
    }
}

impl DerefMut for SessionHandle<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.session.as_mut().unwrap()
    }
}

impl Drop for SessionHandle<'_> {
    fn drop(&mut self) {
        if let Some(session) = self.session.take() {
            self.pool.sessions.lock().push(session);
        }
        // permit drops here, returning capacity
    }
}
