mod annotation;
mod capture;
mod input;
mod screen_capture;
mod vision;

use clap::{Parser, ValueEnum};
use evdev::KeyCode;
use image::codecs::png::CompressionType;
use snafu::{ResultExt, Snafu};
use std::{fmt::Display, path::Path};
use tracing_subscriber::{
    EnvFilter,
    fmt::{self, format::FmtSpan},
    layer::SubscriberExt,
    util::SubscriberInitExt,
};

#[derive(Parser)]
#[command(version, about, long_about = None)]
enum Command {
    Capture {
        #[arg(short, long)]
        node: Option<Box<Path>>,
        #[arg(short, long, default_value = "BTN_LEFT")]
        key: KeyCode,
        #[arg(short, long, default_value_t = Compression::Best)]
        compression: Compression,
    },
    Annotate,
}

#[tokio::main]
#[snafu::report]
async fn main() -> Result<(), Error> {
    registry_subscriber();
    match Command::parse() {
        Command::Capture {
            node,
            key,
            compression,
        } => capture::capture(node, key, compression.into())
            .await
            .context(CaptureSnafu),
        Command::Annotate => {
            annotation::run_gui();
            Ok(())
        }
    }
}

fn registry_subscriber() {
    tracing_subscriber::registry()
        .with(fmt::layer().with_span_events(FmtSpan::CLOSE))
        .with(EnvFilter::from_default_env())
        .init();
}

#[derive(Clone, ValueEnum)]
enum Compression {
    Fast,
    Best,
}

impl From<Compression> for CompressionType {
    fn from(value: Compression) -> Self {
        match value {
            Compression::Fast => Self::Fast,
            Compression::Best => Self::Best,
        }
    }
}

impl Display for Compression {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Compression::Fast => "fast",
            Compression::Best => "best",
        };
        f.write_str(s)
    }
}

#[derive(Debug, Snafu)]
enum Error {
    Capture { source: capture::Error },
}
