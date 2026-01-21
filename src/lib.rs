mod error;
mod g2p;
mod session_pool;
mod stream;
mod synthesizer;
mod tokenizer;
mod transcription;
mod voice;

use {
    crate::session_pool::SessionPool,
    bincode::{config::standard, decode_from_slice},
    ort::{execution_providers::CUDAExecutionProvider, session::Session},
    std::{collections::HashMap, path::Path, sync::Arc, time::Duration},
    tokio::{fs::read, sync::Mutex},
};
pub use {error::*, g2p::*, stream::*, tokenizer::*, transcription::*, voice::*};

pub struct KokoroTts {
    model: Arc<SessionPool>,
    voices: Arc<HashMap<String, Vec<Vec<Vec<f32>>>>>,
}

impl KokoroTts {
    pub async fn new_with_pool<P: AsRef<Path>>(
        model_path: P,
        voices_path: P,
        pool_size: usize,
    ) -> Result<Self, KokoroError> {
        let voices = read(voices_path).await?;
        let (voices, _) = decode_from_slice(&voices, standard())?;

        let mut sessions = Vec::with_capacity(pool_size);
        for _ in 0..pool_size {
            let s = Session::builder()?
                .with_execution_providers([CUDAExecutionProvider::default().build()])?
                .commit_from_file(model_path.as_ref())?;
            sessions.push(s);
        }

        Ok(Self {
            model: Arc::new(SessionPool::new(sessions)),
            voices,
        })
    }

    pub async fn new<P: AsRef<Path>>(model_path: P, voices_path: P) -> Result<Self, KokoroError> {
        Self::new_with_pool(model_path, voices_path, 1).await
    }

    pub async fn synth<S>(&self, text: S, voice: Voice) -> Result<(Vec<f32>, Duration), KokoroError>
    where
        S: AsRef<str>,
    {
        let name = voice.get_name();
        let pack = self
            .voices
            .get(name)
            .ok_or(KokoroError::VoiceNotFound(name.to_owned()))?;
        synthesizer::synth(Arc::downgrade(&self.model), text, pack, voice).await
    }

    pub fn stream<S>(&self, voice: Voice) -> (SynthSink<S>, SynthStream)
    where
        S: AsRef<str> + Send + 'static,
    {
        let voices = Arc::downgrade(&self.voices);
        let model = Arc::downgrade(&self.model);

        start_synth_session(voice, move |text, voice| {
            let voices = voices.clone();
            let model = model.clone();
            async move {
                let name = voice.get_name();
                let voices = voices.upgrade().ok_or(KokoroError::ModelReleased)?;
                let pack = voices
                    .get(name)
                    .ok_or(KokoroError::VoiceNotFound(name.to_owned()))?;
                synthesizer::synth(model, text, pack, voice).await
            }
        })
    }
}
