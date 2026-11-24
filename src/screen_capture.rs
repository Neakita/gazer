use ashpd::desktop::{
    CreateSessionOptions,
    screencast::{
        OpenPipeWireRemoteOptions, Screencast, SelectSourcesOptions, StartCastOptions, Stream,
    },
};
use pipewire::{
    context::ContextBox,
    keys::{MEDIA_CATEGORY, MEDIA_ROLE, MEDIA_TYPE},
    main_loop::MainLoopRc,
    properties::properties,
    spa::{
        param::{
            ParamType,
            format::{FormatProperties, MediaSubtype, MediaType},
            format_utils,
            video::{VideoFormat, VideoInfoRaw},
        },
        pod::{
            self, Pod,
            serialize::{GenError, PodSerializer},
        },
        utils::{Direction, Fraction, Rectangle, SpaTypes},
    },
    stream::{StreamBox, StreamFlags},
};
use snafu::{OptionExt, ResultExt, Snafu};
use std::{cell::RefCell, io::Cursor, os::fd::OwnedFd, rc::Rc};
use tracing::{debug, error, warn};

pub(super) async fn capture<F, E>(callback: F) -> Result<()>
where
    E: std::error::Error + Send + Sync + 'static,
    F: FnMut(&VideoInfoRaw, &mut [u8]) -> Result<(), E> + Send + 'static,
{
    let (streams, descriptor) = open_portal().await.context(OpenPortalSnafu)?;
    let [stream] = streams.as_ref() else {
        return Err(Error::UnexpectedStreamCount {
            count: streams.len(),
        });
    };
    let node_id = stream.pipe_wire_node_id();
    run_pipe(descriptor, node_id, callback)?;
    Ok(())
}

fn run_pipe<F, E>(descriptor: OwnedFd, node_id: u32, mut callback: F) -> Result<()>
where
    E: std::error::Error + Send + Sync + 'static,
    F: FnMut(&VideoInfoRaw, &mut [u8]) -> Result<(), E> + Send + 'static,
{
    let main_loop = MainLoopRc::new(None).context(PipewireSnafu)?;
    let context = ContextBox::new(main_loop.loop_(), None).context(PipewireSnafu)?;
    let core = context
        .connect_fd(descriptor, None)
        .context(PipewireSnafu)?;
    let properties = properties! {
        *MEDIA_TYPE => "Video",
        *MEDIA_CATEGORY => "Capture",
        *MEDIA_ROLE => "Screen",
    };
    let callback_error: Rc<RefCell<Option<E>>> = Rc::new(RefCell::new(None));
    let stream = StreamBox::new(&core, "gazer-capture", properties).context(PipewireSnafu)?;
    let _listener = {
        let main_loop = main_loop.clone();
        let callback_error = Rc::clone(&callback_error);
        stream
            .add_local_listener_with_user_data(VideoInfoRaw::default())
            .state_changed(|_, _, old, new| debug!(?old, ?new, "stream state changed"))
            .param_changed(|_, info, param_type, param| {
                parse_video_info(info, ParamType(param_type), param);
            })
            .process(move |stream, video_info| {
                let Some(mut buffer) = stream.dequeue_buffer() else {
                    debug!("out of buffers");
                    return;
                };
                let datas = buffer.datas_mut();
                let [data] = datas else {
                    error!(datas_count = datas.len(), "unexpected datas count");
                    return;
                };
                let Some(data) = data.data() else {
                    error!("unable to get data");
                    return;
                };
                if let Err(error) = callback(video_info, data) {
                    *callback_error.borrow_mut() = Some(error);
                    main_loop.quit();
                }
            })
            .register()
            .context(PipewireSnafu)?
    };
    let params = create_params();
    let params = serialize_params(params).context(GeneratorSnafu)?;
    let params = params_as_pod(&params).context(ParamsPodSnafu)?;
    connect_stream(node_id, &stream, params).context(PipewireSnafu)?;
    main_loop.run();
    if let Some(error) = callback_error.borrow_mut().take() {
        return Err(Error::Callback {
            source: Box::new(error),
        });
    }
    Ok(())
}

fn parse_video_info(info: &mut VideoInfoRaw, param_type: ParamType, param: Option<&Pod>) {
    let Some(param) = param else {
        debug!("no param received");
        return;
    };
    if param_type != ParamType::Format {
        debug!(?param_type, "param format");
        return;
    }
    let (media_type, media_subtype) = match format_utils::parse_format(param) {
        Ok(value) => value,
        Err(error) => {
            warn!(?error, "unable to parse format");
            return;
        }
    };
    if media_type != MediaType::Video || media_subtype != MediaSubtype::Raw {
        warn!(
            ?media_type,
            ?media_subtype,
            "unexpected media type or subtype"
        );
        return;
    }
    _ = info
        .parse(param)
        .inspect_err(|error| warn!(?error, "unable to parse format"));
    debug!(?info, "parsed video info");
}

fn connect_stream(
    node_id: u32,
    stream: &StreamBox<'_>,
    mut params: [&Pod; 1],
) -> Result<(), pipewire::Error> {
    stream.connect(
        Direction::Input,
        Some(node_id),
        StreamFlags::AUTOCONNECT | StreamFlags::MAP_BUFFERS,
        &mut params,
    )
}

fn params_as_pod(params: &[u8]) -> Option<[&Pod; 1]> {
    Pod::from_bytes(params).map(|x| [x])
}

fn serialize_params(params: pod::Object) -> Result<Vec<u8>, GenError> {
    PodSerializer::serialize(Cursor::new(Vec::new()), &pod::Value::Object(params))
        .map(|x| x.0.into_inner())
}

fn create_params() -> pod::Object {
    pod::object!(
        SpaTypes::ObjectParamFormat,
        ParamType::EnumFormat,
        pod::property!(FormatProperties::MediaType, Id, MediaType::Video),
        pod::property!(FormatProperties::MediaSubtype, Id, MediaSubtype::Raw),
        pod::property!(
            FormatProperties::VideoFormat,
            Choice,
            Enum,
            Id,
            VideoFormat::RGBx,
            VideoFormat::BGRx,
        ),
        pod::property!(
            FormatProperties::VideoSize,
            Choice,
            Range,
            Rectangle,
            Rectangle {
                width: 320,
                height: 320
            },
            Rectangle {
                width: 1,
                height: 1
            },
            Rectangle {
                width: 4096,
                height: 4096
            }
        ),
        pod::property!(
            FormatProperties::VideoFramerate,
            Choice,
            Range,
            Fraction,
            Fraction { num: 25, denom: 1 },
            Fraction { num: 0, denom: 1 },
            Fraction {
                num: 1000,
                denom: 1
            }
        ),
    )
}

async fn open_portal() -> Result<(Box<[Stream]>, OwnedFd), ashpd::Error> {
    let screencast = Screencast::new().await?;
    let options = CreateSessionOptions::default();
    let session = screencast.create_session(options).await?;
    let options = SelectSourcesOptions::default();
    screencast.select_sources(&session, options).await?;
    let options = StartCastOptions::default();
    let response = screencast
        .start(&session, None, options)
        .await?
        .response()?;
    let streams = response.streams();
    let options = OpenPipeWireRemoteOptions::default();
    let descriptor = screencast.open_pipe_wire_remote(&session, options).await?;
    let streams = streams.to_owned().into_boxed_slice();
    Ok((streams, descriptor))
}

#[derive(Debug, Snafu)]
pub(super) enum Error {
    #[snafu(display("Unable to open portal"))]
    OpenPortal {
        source: ashpd::Error,
    },

    #[snafu(display("Expected exactly one stream, got {count}"))]
    UnexpectedStreamCount {
        count: usize,
    },

    Pipewire {
        source: pipewire::Error,
    },

    Generator {
        source: GenError,
    },

    ParamsPod,

    #[snafu(display("Unable to process data"))]
    Callback {
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}

pub(super) type Result<T, E = Error> = std::result::Result<T, E>;
