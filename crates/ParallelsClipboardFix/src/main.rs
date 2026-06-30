#![cfg_attr(not(windows), allow(dead_code))]

mod clipboard;
mod config;
mod logger;
mod startup;
mod tray;
mod watcher;

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
    }
}

#[cfg(not(windows))]
fn run() -> Result<(), AppError> {
    Err(AppError::UnsupportedPlatform)
}

#[cfg(windows)]
fn run() -> Result<(), AppError> {
    use std::sync::mpsc;
    use std::thread;

    let mut config = config::AppConfig::load_or_create()?;
    let logger = logger::Logger::open(config.log_level)?;
    logger.info("ParallelsClipboardFix starting");

    let startup = startup::StartupShortcut::new()?;
    let mut clipboard = clipboard::ClipboardService::new();

    let (app_sender, app_receiver) = mpsc::channel::<AppEvent>();
    let (watcher_sender, watcher_receiver) = mpsc::channel::<watcher::WatcherEvent>();
    let (tray_sender, tray_receiver) = mpsc::channel::<tray::TrayCommand>();

    let _watcher = watcher::ClipboardWatcher::start(&config, watcher_sender)?;
    let _tray = tray::TrayController::start(
        tray_sender,
        tray::TrayState {
            enabled: config.enabled,
            start_with_windows: startup.is_installed(),
        },
    )?;

    forward_watcher_events(watcher_receiver, app_sender.clone())?;
    forward_tray_commands(tray_receiver, app_sender.clone())?;

    while let Ok(event) = app_receiver.recv() {
        match event {
            AppEvent::ClipboardChanged | AppEvent::RetryRequested => {
                match clipboard.fix_now(&config) {
                    Ok(outcome) => logger.debug(format!("clipboard fix outcome: {outcome:?}")),
                    Err(error) => logger.warn(format!("clipboard fix failed: {error}")),
                }
            }
            AppEvent::TrayCommand(tray::TrayCommand::FixNow) => match clipboard.fix_now(&config) {
                Ok(outcome) => logger.info(format!("manual fix outcome: {outcome:?}")),
                Err(error) => logger.warn(format!("manual fix failed: {error}")),
            },
            AppEvent::TrayCommand(tray::TrayCommand::ToggleEnabled) => {
                config.enabled = !config.enabled;
                config.save_to(config::default_config_path()?)?;
                logger.info(format!("fixer enabled: {}", config.enabled));
            }
            AppEvent::TrayCommand(tray::TrayCommand::ToggleStartup) => {
                if startup.is_installed() {
                    startup.uninstall()?;
                    logger.info("startup shortcut removed");
                } else {
                    startup.install(std::env::current_exe()?)?;
                    logger.info("startup shortcut installed");
                }
            }
            AppEvent::TrayCommand(tray::TrayCommand::Quit) => {
                logger.info("ParallelsClipboardFix exiting");
                break;
            }
        }
    }

    let _ = thread::panicking();
    Ok(())
}

#[cfg(windows)]
fn forward_watcher_events(
    receiver: std::sync::mpsc::Receiver<watcher::WatcherEvent>,
    sender: std::sync::mpsc::Sender<AppEvent>,
) -> Result<(), AppError> {
    std::thread::Builder::new()
        .name("ParallelsClipboardFix-watcher-forwarder".to_owned())
        .spawn(move || {
            while let Ok(event) = receiver.recv() {
                let app_event = match event {
                    watcher::WatcherEvent::ClipboardChanged => AppEvent::ClipboardChanged,
                    watcher::WatcherEvent::RetryRequested { .. } => AppEvent::RetryRequested,
                };
                if sender.send(app_event).is_err() {
                    break;
                }
            }
        })
        .map(|_| ())
        .map_err(|_| AppError::ThreadStart("watcher forwarder"))
}

#[cfg(windows)]
fn forward_tray_commands(
    receiver: std::sync::mpsc::Receiver<tray::TrayCommand>,
    sender: std::sync::mpsc::Sender<AppEvent>,
) -> Result<(), AppError> {
    std::thread::Builder::new()
        .name("ParallelsClipboardFix-tray-forwarder".to_owned())
        .spawn(move || {
            while let Ok(command) = receiver.recv() {
                if sender.send(AppEvent::TrayCommand(command)).is_err() {
                    break;
                }
            }
        })
        .map(|_| ())
        .map_err(|_| AppError::ThreadStart("tray forwarder"))
}

#[cfg(windows)]
#[derive(Debug)]
enum AppEvent {
    ClipboardChanged,
    RetryRequested,
    TrayCommand(tray::TrayCommand),
}

#[derive(Debug)]
enum AppError {
    UnsupportedPlatform,
    Config(config::ConfigError),
    Logger(logger::LoggerError),
    Startup(startup::StartupError),
    Watcher(watcher::WatcherError),
    Tray(tray::TrayError),
    Io(std::io::Error),
    ThreadStart(&'static str),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedPlatform => {
                formatter.write_str("ParallelsClipboardFix is a Windows-only application")
            }
            Self::Config(error) => write!(formatter, "{error}"),
            Self::Logger(error) => write!(formatter, "{error}"),
            Self::Startup(error) => write!(formatter, "{error}"),
            Self::Watcher(error) => write!(formatter, "{error}"),
            Self::Tray(error) => write!(formatter, "{error}"),
            Self::Io(error) => write!(formatter, "{error}"),
            Self::ThreadStart(name) => write!(formatter, "failed to start {name} thread"),
        }
    }
}

impl std::error::Error for AppError {}

impl From<config::ConfigError> for AppError {
    fn from(error: config::ConfigError) -> Self {
        Self::Config(error)
    }
}

impl From<logger::LoggerError> for AppError {
    fn from(error: logger::LoggerError) -> Self {
        Self::Logger(error)
    }
}

impl From<startup::StartupError> for AppError {
    fn from(error: startup::StartupError) -> Self {
        Self::Startup(error)
    }
}

impl From<watcher::WatcherError> for AppError {
    fn from(error: watcher::WatcherError) -> Self {
        Self::Watcher(error)
    }
}

impl From<tray::TrayError> for AppError {
    fn from(error: tray::TrayError) -> Self {
        Self::Tray(error)
    }
}

impl From<std::io::Error> for AppError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}
