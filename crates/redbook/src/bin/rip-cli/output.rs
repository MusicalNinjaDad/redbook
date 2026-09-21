use std::{fs, path::PathBuf, process::Termination as _T};

use tracing_subscriber::{
    filter::LevelFilter, fmt::layer, prelude::*, registry, util::TryInitError,
};

use crate::{Exit, Rip, cli::LogLevel};

impl Rip {
    #[cfg_attr(not(target_family = "windows"), expect(dead_code))]
    pub fn init_tracing(&self) -> Exit<()> {
        let stdout_level = match self.verbosity() {
            ..=-1 => LevelFilter::OFF,
            0 => LevelFilter::INFO,
            1 => LevelFilter::DEBUG,
            2.. => LevelFilter::TRACE,
        };
        let stdout = layer().with_filter(stdout_level);

        let stderr_level = match self.verbosity() {
            ..=-2 => LevelFilter::OFF,
            -1.. => LevelFilter::WARN,
        };
        let stderr = layer()
            .with_writer(std::io::stderr)
            .with_filter(stderr_level);

        let file = if self.loglevel != LogLevel::Off {
            let path = self.log.clone().unwrap_or_else(|| PathBuf::from("rip.log"));
            let file = fs::File::options().append(true).create(true).open(path)?;
            let loglevel = LevelFilter::from(&self.loglevel);
            let json = match self.format {
                crate::cli::LogFormat::Human => None,
                crate::cli::LogFormat::Json => Some(layer().json()),
            };
            Some(
                layer()
                    .with_writer(file)
                    .with_filter(loglevel)
                    .and_then(json),
            )
        } else {
            None
        };

        registry().with(stdout).with(stderr).with(file).try_init()?;

        Exit::Ok(())
    }
}

impl From<&LogLevel> for LevelFilter {
    fn from(level: &LogLevel) -> Self {
        match level {
            LogLevel::Off => LevelFilter::OFF,
            LogLevel::Error => LevelFilter::ERROR,
            LogLevel::Warn => LevelFilter::WARN,
            LogLevel::Info => LevelFilter::INFO,
            LogLevel::Debug => LevelFilter::DEBUG,
            LogLevel::Trace => LevelFilter::TRACE,
        }
    }
}

impl<T: _T> From<TryInitError> for Exit<T> {
    fn from(error: TryInitError) -> Self {
        Self::Logging(error.to_string())
    }
}
