//! Headless shell for the Codex realtime voice engine.
mod check_wake;

use anyhow::{ensure, Context, Result};
use std::{fs, sync::Arc};
use voicebro_core::{
    assistant::codex::CodexAssistant,
    discord::ConnectionState,
    paths,
    secrets::MemoryStore,
    settings::{store::SettingsStore, AccessMode, AssistantEngine, AssistantMode},
    status::ConnectionErrors,
    AppEvent, EventBus, Runtime, SecretKey, SecretStore, Settings,
};

fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let check = match args.next() {
        None => None,
        Some(flag) if flag == "--check-wake" => {
            Some(args.next().context("expected a 16 kHz mono WAV path")?)
        }
        _ => anyhow::bail!("usage: voicebro-server [--check-wake WAV]"),
    };
    ensure!(args.next().is_none(), "unexpected extra argument");
    let data = paths::data_dir();
    fs::create_dir_all(&data)?;
    let store = SettingsStore::new(paths::settings_path(&data));
    let settings = match store.load()? {
        Some(saved) => saved,
        None => {
            let mut defaults = Settings::default();
            defaults.codex.executable = "/usr/local/bin/codex".into();
            defaults
                .wake
                .phrases
                .entry(defaults.wake.model.clone())
                .or_default()
                .activation = Some(vec!["hey hank".into()]);
            store.save(&defaults)?;
            defaults
        }
    };
    settings.validate()?;
    ensure!(
        settings.assistant.engine == AssistantEngine::Codex
            && settings.assistant.mode == AssistantMode::Realtime,
        "the server image supports Codex realtime voice"
    );
    ensure!(
        settings.discord.access != AccessMode::Owner,
        "owner linking requires the desktop interface"
    );

    if let Some(wav) = check {
        return check_wake::run(&data, &settings, std::path::Path::new(&wav));
    }

    let token_path = std::env::var_os("DISCORD_TOKEN_FILE")
        .unwrap_or_else(|| "/run/secrets/discord_token".into());
    let token = fs::read_to_string(&token_path)
        .with_context(|| format!("read Discord bot token from {token_path:?}"))?;
    ensure!(!token.trim().is_empty(), "Discord bot token is empty");
    let secrets = Arc::new(MemoryStore::default());
    secrets.set(SecretKey::DiscordToken, token.trim())?;
    drop(token);

    let executor = tokio::runtime::Runtime::new()?;
    let handle = executor.handle().clone();
    let events = EventBus::default();
    let mut messages = events.subscribe();
    let errors = Arc::new(ConnectionErrors::new(events.clone()));
    let assistant = CodexAssistant::new(
        handle.clone(),
        events.clone(),
        errors.clone(),
        &data,
        &settings,
    );
    let runtime = Runtime::with_assistant(
        env!("CARGO_PKG_VERSION"),
        data,
        store,
        secrets,
        events,
        handle,
        errors,
        Arc::new(assistant),
    );
    runtime.start()?;
    let result = executor.block_on(async {
        #[cfg(unix)]
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        let stop = async {
            #[cfg(unix)]
            tokio::select! {
                result = tokio::signal::ctrl_c() => result?,
                _ = terminate.recv() => {}
            }
            #[cfg(not(unix))]
            tokio::signal::ctrl_c().await?;
            Ok::<_, std::io::Error>(())
        };
        tokio::pin!(stop);
        let mut connection = None;
        loop {
            tokio::select! {
                result = &mut stop => { result?; break; }
                event = messages.recv() => match event {
                    Ok(AppEvent::Log { line }) => eprintln!("{line}"),
                    Ok(AppEvent::ConnectionError { source, message: Some(reason) }) =>
                        eprintln!("{source}: {reason}"),
                    Ok(AppEvent::Discord(status)) => {
                        ensure!(status.state != ConnectionState::Error, "Discord: {}",
                            status.last_error.as_deref().unwrap_or("connection failed"));
                        if connection != Some(status.state) {
                            eprintln!("Discord: {:?}", status.state);
                            connection = Some(status.state);
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    _ => {}
                }
            }
        }
        Ok(())
    });
    runtime.shutdown();
    result
}
