use anyhow::{ensure, Context, Result};
use std::path::Path;
use voicebro_core::{
    paths,
    wake::{
        spotter::PhraseSpotter, vosk::VoskEngine, worker::SpeakerState, ModelSource, PhraseKind,
        PhraseSet, WakeEngine,
    },
    Settings,
};

pub fn run(data: &Path, settings: &Settings, wav: &Path) -> Result<()> {
    let mut reader = hound::WavReader::open(wav)?;
    let spec = reader.spec();
    ensure!(
        spec.sample_rate == 16000
            && spec.channels == 1
            && spec.bits_per_sample == 16
            && spec.sample_format == hound::SampleFormat::Int,
        "expected 16 kHz mono signed 16-bit PCM"
    );
    let assets = paths::assets_dir(data);
    let source = ModelSource::from_id(&settings.wake.model).context("unknown wake model")?;
    let model = VoskEngine.load(
        &assets,
        &source.directory(&assets, &paths::models_dir(data)),
    )?;
    let mut speaker = SpeakerState::new(model.new_recognizer()?, PhraseSpotter::new());
    let phrases = PhraseSet::from_settings(settings);
    // Feed 20 ms frames, then allow the recognizer to finalize the last word.
    let mut samples = reader
        .samples::<i16>()
        .chain(std::iter::repeat_with(|| Ok(0)).take(24000));
    loop {
        let frame = samples.by_ref().take(320).collect::<Result<Vec<_>, _>>()?;
        if frame.is_empty() {
            break;
        }
        for (kind, phrase, _) in speaker.accept_mono(&frame, &phrases) {
            if kind == PhraseKind::Activate {
                println!("Activated on {phrase}");
                return Ok(());
            }
        }
    }
    anyhow::bail!("no configured wake phrase recognized in {}", wav.display())
}
