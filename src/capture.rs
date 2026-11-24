use crate::{input, screen_capture};
use evdev::KeyCode;
use garb::{SizeError, bytes::bgra_to_rgba_inplace};
use image::{
    ExtendedColorType, ImageEncoder,
    codecs::png::{CompressionType, FilterType, PngEncoder},
};
use pipewire::spa::{param::video::VideoFormat, utils::Rectangle};
use snafu::{ResultExt, Snafu};
use std::{
    fs::{self, File},
    io::{self, BufWriter},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{SystemTime, SystemTimeError},
};
use tracing::{error, instrument};

pub(super) async fn capture(
    node: Option<Box<Path>>,
    key: KeyCode,
    compression: CompressionType,
) -> Result<()> {
    pipewire::init();
    let should_capture = Arc::new(AtomicBool::new(false));
    {
        let should_capture = Arc::clone(&should_capture);
        thread::spawn(move || {
            input::capture(node, key, |pressed| {
                should_capture.store(pressed, Ordering::Release);
            })
            .inspect_err(|error| error!(%error))
        });
    }
    screen_capture::capture::<_, CallbackError>(move |info, data| {
        if !should_capture.load(Ordering::Acquire) {
            return Ok(());
        }
        convert_to_rgba(info.format(), data)?;
        save(data, info.size(), compression)?;
        Ok(())
    })
    .await
    .context(ScreenCaptureSnafu)
}

#[instrument(skip(data), level = "trace")]
fn convert_to_rgba(source_format: VideoFormat, data: &mut [u8]) -> CallbackResult<()> {
    if source_format == VideoFormat::BGRx {
        bgra_to_rgba_inplace(data).context(SizeSnafu)
    } else {
        Err(CallbackError::UnknownFormat {
            format: source_format,
        })
    }
}

#[instrument(skip(data), level = "trace")]
fn save(data: &mut [u8], size: Rectangle, compression: CompressionType) -> CallbackResult<()> {
    const TEMP_NAME: &str = "temp.png";
    let Rectangle { width, height } = size;
    encoder(TEMP_NAME, compression)?
        .write_image(data, width, height, ExtendedColorType::Rgba8)
        .context(ImageWritingSnafu {
            data_length: data.len(),
            size,
        })?;
    let name = permament_name()?;
    fs::rename(TEMP_NAME, &name).with_context(|_| RenameSnafu {
        old: TEMP_NAME,
        new: name,
    })?;
    Ok(())
}

fn encoder(
    path: impl AsRef<Path>,
    compression: CompressionType,
) -> CallbackResult<PngEncoder<BufWriter<File>>> {
    let file = File::create(&path).with_context(|_| CreateFileSnafu {
        path: path.as_ref(),
    })?;
    let writer = BufWriter::new(file);
    Ok(PngEncoder::new_with_quality(
        writer,
        compression,
        FilterType::Adaptive,
    ))
}

fn permament_name() -> CallbackResult<String> {
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .context(SystemTimeSnafu)?
        .as_nanos();
    Ok(format!("{timestamp}.png"))
}

#[derive(Debug, Snafu)]
pub(super) enum Error {
    ScreenCapture { source: screen_capture::Error },
}

type Result<T, E = Error> = std::result::Result<T, E>;
type CallbackResult<T, E = CallbackError> = std::result::Result<T, E>;

#[derive(Debug, Snafu)]
enum CallbackError {
    #[snafu(display("An unknown format {format:?} was received"))]
    UnknownFormat {
        format: VideoFormat,
    },

    #[snafu(display("Pixel buffer size or alignment error"))]
    Size {
        source: SizeError,
    },

    #[snafu(display("Unable to write image with size {size:?} and data length {data_length}"))]
    ImageWriting {
        source: image::ImageError,
        data_length: usize,
        size: Rectangle,
    },

    #[snafu(display("Unable to rename {old:?} to {new:?}"))]
    Rename {
        source: io::Error,
        old: PathBuf,
        new: PathBuf,
    },

    #[snafu(display("Unable to create {path:?}"))]
    CreateFile {
        source: io::Error,
        path: PathBuf,
    },

    SystemTime {
        source: SystemTimeError,
    },
}
