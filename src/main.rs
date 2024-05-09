use {
    aargvark::{
        vark,
        Aargvark,
        AargvarkFromStr,
        HelpPatternElement,
    },
    http::{
        header::CONTENT_TYPE,
        Response,
    },
    loga::{
        ea,
        fatal,
        DebugDisplay,
        ErrContext,
        ResultContext,
        StandardFlag,
        StandardLog,
    },
    serde::Deserialize,
    serde_json::json,
    std::{
        borrow::Cow,
        collections::HashMap,
        env,
        path::PathBuf,
        thread::spawn,
        time::Duration,
    },
    tao::{
        event::{
            Event,
            WindowEvent,
        },
        event_loop::{
            self,
            ControlFlow,
        },
        platform::unix::WindowExtUnix,
    },
    tokio::{
        io::{
            AsyncBufReadExt,
            BufReader,
        },
        process::Command,
        select,
        sync::mpsc::unbounded_channel,
        time::sleep,
    },
    wry::WebViewBuilderExtUnix,
};

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum VecMode {
    /// Pixels corresponding to device pixels
    Physical,
    /// Modified by scaling settings to produce physical pixels
    Logical,
    /// Percent of monitor size
    Percent,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
struct XY {
    mode: VecMode,
    x: f64,
    y: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
struct Config {
    /// Monitor to put the wongus on.
    monitor: Option<String>,
    /// Where to place the wongus on the monitor.
    ///
    /// Note that if you use percent location, the percent is used for both the monitor
    /// location and window origin, that is: `(0, 0)` will put the top-left corner of
    /// the window in the top-left of the monitor, `(100, 100)` will put the
    /// bottom-right corner of the window at the bottom-right of the monitor.
    position: XY,
    /// How big to make the wongus.
    size: XY,
    /// Display wongus on all workspaces, if you want
    #[serde(default)]
    all_workspaces: bool,
}

struct ArgKv {
    k: String,
    v: String,
}

impl AargvarkFromStr for ArgKv {
    fn from_str(s: &str) -> Result<Self, String> {
        let Some((k, v)) = s.split_once("=") else {
            return Err(format!("All arguments must be in the form KEY=VALUE here, but got [{}]", s));
        };
        return Ok(ArgKv {
            k: k.to_string(),
            v: v.to_string(),
        });
    }

    fn build_help_pattern(_state: &mut aargvark::HelpState) -> aargvark::HelpPattern {
        return aargvark::HelpPattern(vec![HelpPatternElement::Type("KEY=VALUE".to_string())]);
    }
}

#[derive(Aargvark)]
struct Args {
    /// Directory containing config.json, index.html and any other assets.
    content_root: PathBuf,
    debug: Option<()>,
    /// Additional arguments to be passed to the script.
    args: Vec<ArgKv>,
}

#[derive(Deserialize)]
struct IPCReqCommand {
    command: Vec<String>,
    #[serde(default)]
    working_dir: Option<String>,
    #[serde(default)]
    environment: HashMap<String, String>,
    /// Timeout command if it takes too long; defaults to 10s.
    #[serde(default)]
    timeout_secs: Option<u64>,
}

#[derive(Deserialize)]
struct IPCReqStreamCommand {
    id: String,
    command: Vec<String>,
    #[serde(default)]
    working_dir: Option<String>,
    #[serde(default)]
    environment: HashMap<String, String>,
}

#[derive(Deserialize)]
enum IPCReqBody {
    Log(String),
    Read(PathBuf),
    RunCommand(IPCReqCommand),
    StreamCommand(IPCReqStreamCommand),
}

#[derive(Deserialize)]
struct IPCReq {
    id: String,
    body: IPCReqBody,
}

fn main() {
    fn inner() -> Result<(), loga::Error> {
        let args = vark::<Args>();
        let log = StandardLog::new().with_flags(if args.debug.is_some() {
            &[StandardFlag::Error, StandardFlag::Warning, StandardFlag::Info, StandardFlag::Debug]
        } else {
            &[StandardFlag::Error, StandardFlag::Warning, StandardFlag::Info]
        });
        let content_root = args.content_root.canonicalize().context("Error making content path absolute")?;
        let config_path = content_root.join("config.json");
        let config =
            serde_json::from_slice::<Config>(
                &std::fs::read(
                    &config_path,
                ).context_with("Error reading config", ea!(path = config_path.to_string_lossy()))?,
            ).context_with("Error parsing config as json", ea!(path = config_path.to_string_lossy()))?;

        // Event loop
        let event_loop = event_loop::EventLoopBuilder::<String>::with_user_event().build();

        // Window
        let monitor = 'found_monitor : loop {
            if let Some(want_monitor) = config.monitor {
                for m in event_loop.available_monitors() {
                    if Some(&want_monitor) == m.name().as_ref() {
                        break 'found_monitor m;
                    }
                }
            }
            if let Some(m) = event_loop.primary_monitor() {
                break 'found_monitor m;
            };
            if let Some(m) = event_loop.available_monitors().next() {
                break 'found_monitor m;
            };
            return Err(loga::err("No monitors found"));
        };
        let monitor_size = monitor.size();
        let monitor_position = monitor.position();
        let size = match config.size.mode {
            VecMode::Physical => (config.size.x, config.size.y),
            VecMode::Logical => (config.size.x * monitor.scale_factor(), config.size.y * monitor.scale_factor()),
            VecMode::Percent => (
                monitor_size.width as f64 * config.size.x,
                monitor_size.height as f64 * config.size.y,
            ),
        };
        let window = {
            #[allow(unused_mut)]
            let mut builder =
                tao::window::WindowBuilder::new()
                    .with_title("wongus")
                    .with_decorations(false)
                    .with_transparent(true)
                    .with_resizable(false)
                    .with_maximizable(false)
                    .with_visible_on_all_workspaces(config.all_workspaces)
                    .with_inner_size(tao::dpi::PhysicalSize {
                        width: size.0,
                        height: size.1,
                    })
                    .with_position(match config.position.mode {
                        VecMode::Physical => tao::dpi::PhysicalPosition {
                            x: monitor_position.x as f64 + config.position.x,
                            y: monitor_position.y as f64 + config.position.y,
                        },
                        VecMode::Logical => tao::dpi::PhysicalPosition {
                            x: monitor_position.y as f64 + config.position.x * monitor.scale_factor(),
                            y: monitor_position.y as f64 + config.position.y * monitor.scale_factor(),
                        },
                        VecMode::Percent => {
                            tao::dpi::PhysicalPosition {
                                x: (monitor_size.width as f64 - size.0) * config.position.x,
                                y: (monitor_size.height as f64 - size.1) * config.position.y,
                            }
                        },
                    });
            #[cfg(target_os = "windows")]
            {
                use tao::platform::windows::WindowBuilderExtWindows;

                builder = builder.with_undecorated_shadow(false);
            }
            let window = builder.build(&event_loop).unwrap();
            #[cfg(target_os = "windows")]
            {
                use tao::platform::windows::WindowExtWindows;

                window.set_undecorated_shadow(true);
            }
            window
        };

        // Webview
        let (ipc_req_tx, mut ipc_req_rx) = unbounded_channel::<Vec<u8>>();
        let webview = {
            #[cfg(any(target_os = "windows", target_os = "macos", target_os = "ios", target_os = "android"))]
            let builder = WebViewBuilder::new(&window);
            #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "ios", target_os = "android")))]
            const PROTO: &str = "local";
            let webview = wry::WebViewBuilder::new_gtk(window.default_vbox().unwrap())
                //. .
                .with_transparent(true)
                //. .
                .with_ipc_handler({
                    move |req| {
                        _ = ipc_req_tx.send(req.into_body().into_bytes());
                    }
                })
                //. .
                .with_initialization_script(include_str!("ipc_setup.js"))
                //. .
                .with_back_forward_navigation_gestures(false)
                // Needed to intercept non-page errors
                .with_custom_protocol(PROTO.to_string(), {
                    let log = log.clone();
                    move |request| {
                        match (|| -> Result<http::Response<Cow<'static, [u8]>>, loga::Error> {
                            let path = request.uri().path();
                            eprintln!("req {}", request.uri());
                            return Ok(
                                Response::builder()
                                    .header(
                                        CONTENT_TYPE,
                                        mime_guess::from_path(&path).first_or_text_plain().essence_str(),
                                    )
                                    .body(Cow::Owned(std::fs::read(&path).context("Error reading requested path")?))
                                    .unwrap(),
                            );
                        })() {
                            Ok(r) => {
                                return r;
                            },
                            Err(e) => {
                                let e = e.context("Failed to load local path");
                                log.log_err(StandardFlag::Warning, e.clone());
                                return http::Response::builder()
                                    .header(CONTENT_TYPE, "text/plain")
                                    .status(500)
                                    .body(Cow::Owned(e.to_string().as_bytes().to_vec()))
                                    .unwrap();
                            },
                        }
                    }
                })
                //. .
                .with_url(
                    format!(
                        "{}://{}",
                        PROTO,
                        content_root.join("index.html").to_str().context("Content root path must be utf-8")?
                    ),
                )
                //. .
                .build()
                .context("Error initializing webview")?;
            webview
        };

        // IPC processing
        webview.evaluate_script(include_str!("ipc_setup.js")).context("Error executing ipc setup script")?;
        {
            let mut script = vec![];
            for (k, v) in env::vars() {
                script.push(
                    format!(
                        "wongus.env.set({}, {});\n",
                        serde_json::to_string(&k).unwrap(),
                        serde_json::to_string(&v).unwrap()
                    ),
                );
            }
            for kv in args.args {
                script.push(
                    format!(
                        "wongus.args.set({}, {});\n",
                        serde_json::to_string(&kv.k).unwrap(),
                        serde_json::to_string(&kv.v).unwrap()
                    ),
                );
            }
            webview.evaluate_script(&script.join("")).context("Error executing env/args setup script")?;
        }
        spawn({
            let ipc_resp = event_loop.create_proxy();
            let rt = tokio::runtime::Builder::new_current_thread().build().context("Error starting ipc processor")?;
            let log = log.clone();
            move || rt.block_on(async move {
                while let Some(req) = ipc_req_rx.recv().await {
                    let req = match serde_json::from_slice::<IPCReq>(&req) {
                        Ok(r) => r,
                        Err(e) => {
                            log.log_err(StandardFlag::Warning, e.context("Assertion! Error parsing IPC request"));
                            return;
                        },
                    };
                    tokio::spawn({
                        let ipc_resp = ipc_resp.clone();
                        let log = log.clone();
                        async move {
                            let resp = match async {
                                match req.body {
                                    IPCReqBody::Log(message) => {
                                        log.log(StandardFlag::Info, format!("console.log: {}", message));
                                        return Ok("{}".to_string());
                                    },
                                    IPCReqBody::Read(path) => {
                                        return Ok(
                                            serde_json::to_string(
                                                &tokio::fs::read(&path)
                                                    .await
                                                    .context("Error performing read command")?,
                                            ).unwrap(),
                                        );
                                    },
                                    IPCReqBody::RunCommand(req) => {
                                        if req.command.is_empty() {
                                            return Err(loga::err("Commandline is empty"));
                                        }
                                        let mut command = Command::new(&req.command[0]);
                                        command.args(&req.command[1..]);
                                        if let Some(cwd) = req.working_dir {
                                            command.current_dir(&cwd);
                                        }
                                        for (k, v) in req.environment {
                                            command.env(k, v);
                                        }
                                        let log = StandardLog::new().fork(ea!(command = command.dbg_str()));
                                        let res = select!{
                                            res = command.output() => res,
                                            _ = sleep(Duration::from_secs(req.timeout_secs.unwrap_or(10))) => {
                                                return Err(
                                                    loga::err("Command execution duration exceeded timeout"),
                                                );
                                            }
                                        }.stack_context(&log, "Error starting command")?;
                                        let log = log.fork(ea!(output = res.dbg_str()));
                                        if !res.status.success() {
                                            return Err(log.err("Command exited with unsuccessful status"));
                                        }
                                        let stdout =
                                            String::from_utf8(
                                                res.stdout,
                                            ).stack_context(&log, "stdout was not valid utf-8")?;
                                        let stderr =
                                            String::from_utf8(
                                                res.stderr,
                                            ).stack_context(&log, "stderr was not valid utf-8")?;
                                        return Ok(serde_json::to_string(&json!({
                                            "stdout": stdout,
                                            "stderr": stderr
                                        })).unwrap());
                                    },
                                    IPCReqBody::StreamCommand(req) => {
                                        tokio::spawn({
                                            let ipc_resp = ipc_resp.clone();
                                            let log = log.clone();
                                            async move {
                                                match async {
                                                    if req.command.is_empty() {
                                                        return Err(loga::err("Commandline is empty"));
                                                    }
                                                    let mut command = Command::new(&req.command[0]);
                                                    command.args(&req.command[1..]);
                                                    if let Some(cwd) = req.working_dir {
                                                        command.current_dir(&cwd);
                                                    }
                                                    for (k, v) in req.environment {
                                                        command.env(k, v);
                                                    }
                                                    let log =
                                                        StandardLog::new().fork(ea!(command = command.dbg_str()));
                                                    let mut proc = command.spawn().context("Error starting command")?;
                                                    let reader = BufReader::new(proc.stdout.take().unwrap());
                                                    let mut lines = reader.lines();
                                                    loop {
                                                        match lines.next_line().await {
                                                            Ok(Some(line)) => {
                                                                match ipc_resp.send_event(
                                                                    format!(
                                                                        "(window._wongus.stream_cbs.get(\"{}\"))(\"{}\");",
                                                                        req.id,
                                                                        line
                                                                    ),
                                                                ) {
                                                                    Ok(_) => { },
                                                                    Err(e) => {
                                                                        log.log_err(
                                                                            StandardFlag::Error,
                                                                            e.context("Error sending line to window"),
                                                                        );
                                                                    },
                                                                };
                                                            },
                                                            Ok(None) => {
                                                                break;
                                                            },
                                                            Err(e) => {
                                                                return Err(e.context("Error reading lines"));
                                                            },
                                                        }
                                                    }
                                                    return Ok(());
                                                }.await {
                                                    Ok(_) => {
                                                        log.log(
                                                            StandardFlag::Info,
                                                            "Streaming command exited normally",
                                                        );
                                                    },
                                                    Err(e) => {
                                                        log.log_err(
                                                            StandardFlag::Warning,
                                                            e.context("Streaming command failed with error"),
                                                        );
                                                    },
                                                }
                                            }
                                        });
                                        return Ok(serde_json::to_string(&json!({ })).unwrap());
                                    },
                                }
                            }.await {
                                Ok(r) => r,
                                Err(e) => serde_json::to_string(&json!({
                                    "err": e.to_string()
                                })).unwrap(),
                            };
                            match ipc_resp.send_event(
                                format!("(window._wongus.responses.get(\"{}\"))(\"{}\");", req.id, resp),
                            ) {
                                Ok(_) => { },
                                Err(e) => {
                                    log.log_err(StandardFlag::Error, e.context("Error sending ipc response"));
                                },
                            };
                        }
                    });
                }
            })
        });

        // Run event loop
        event_loop.run(move |event, _, control_flow| {
            *control_flow = event_loop::ControlFlow::Wait;
            match event {
                Event::UserEvent(script) => {
                    match webview.evaluate_script(&script) {
                        Ok(_) => todo!(),
                        Err(e) => {
                            log.log_err(StandardFlag::Error, e.context("Error executing ipc response script"));
                        },
                    };
                },
                Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => *control_flow = ControlFlow::Exit,
                _ => (),
            }
        });
    }

    match inner() {
        Ok(_) => { },
        Err(e) => fatal(e),
    }
}
