use kokoro_tts::{KokoroTts, Voice};
use rodio::{OutputStreamBuilder, Sink, buffer::SamplesBuffer};
use std::{collections::BTreeMap, sync::Arc};
use tokio::task::JoinSet;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_level(true)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("Init tracing failed");

    // NEW: create a TTS instance backed by a *pool* of ORT Sessions.
    // Pool size is how many synth calls can run truly in parallel inside the crate.
    let tts = Arc::new(
        KokoroTts::new_with_pool(
            "assets/kokoro-v1.1-zh.onnx",
            "assets/voices-v1.1-zh.bin",
            4, // pool size
        )
        .await?,
    );

    // Example batch
    let inputs = vec![
        "只是前卫部队中的一小部分而已",
        "天气不错，我们出去走走吧。",
        "并行合成会更快一些。",
        "最后一句用来测试顺序。",
    ];

    // Run synth concurrently; results can complete out-of-order
    let mut set = JoinSet::new();
    for (idx, text) in inputs.iter().enumerate() {
        let tts = Arc::clone(&tts);
        let text = text.to_string();

        set.spawn(async move {
            let (audio, took) = tts.synth(text, Voice::Zm045(1)).await?;
            anyhow::Ok((idx, audio, took))
        });
    }

    // Collect and re-order by original index
    let mut out: BTreeMap<usize, (Vec<f32>, std::time::Duration)> = BTreeMap::new();
    while let Some(res) = set.join_next().await {
        let (idx, audio, took) = res??;
        println!("item #{idx} synth took: {took:?}");
        out.insert(idx, (audio, took));
    }

    // Play back in original order (sequential)
    for (idx, (audio, _took)) in out {
        println!("Playing item #{idx}");
        play_sound(&audio);
    }

    Ok(())
}

fn play_sound(data: &[f32]) {
    let output_stream_builder = OutputStreamBuilder::from_default_device().unwrap();
    let output_stream = output_stream_builder.open_stream().unwrap();
    let stream_handle = output_stream.mixer();
    let player = Sink::connect_new(stream_handle);

    player.append(SamplesBuffer::new(1, 24000, data.to_vec()));
    player.sleep_until_end()
}
