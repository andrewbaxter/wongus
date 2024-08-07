use {
    aargvark::{
        vark,
        Aargvark,
        AargvarkFromStr,
        HelpPatternElement,
    },
    gtk::{
        glib::object::CastNone,
        prelude::{
            ContainerExt,
            GtkWindowExt,
            MonitorExt,
            WidgetExt,
        },
    },
    gtk_layer_shell::LayerShell,
    http::{
        header::CONTENT_TYPE,
        Request,
        Response,
    },
    http_body_util::BodyExt,
    htwrap::htserve::{
        response_200_json,
        response_400,
        response_503,
        response_503_text,
        Body,
    },
    hyper::body::Incoming,
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
        convert::Infallible,
        env,
        fs::remove_file,
        ops::Deref,
        path::PathBuf,
        sync::{
            atomic::{
                AtomicUsize,
                Ordering,
            },
            Arc,
            Mutex,
        },
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
            EventLoopProxy,
        },
        platform::{
            run_return::EventLoopExtRunReturn,
            unix::{
                EventLoopWindowTargetExtUnix,
                WindowExtUnix,
            },
        },
    },
    tokio::{
        io::{
            AsyncBufReadExt,
            BufReader,
        },
        net::UnixSocket,
        process::Command,
        select,
        sync::{
            mpsc::unbounded_channel,
            oneshot,
        },
        time::sleep,
    },
    wry::WebViewBuilderExtUnix,
};

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum P2 {
    /// Not pixels, but a delusion that will become a pixel once a scaling factor is
    /// applied.
    Logical(i32),
    /// Percent of monitor size (0-100).
    Percent(f64),
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
struct Config {
    /// Monitor to put the wongus on.
    #[serde(default)]
    monitor_index: Option<usize>,
    /// Monitor to put the wongus on. Any monitor with the model containing this string
    /// will match (case insensitive).
    #[serde(default)]
    monitor_model: Option<String>,
    /// Attach the top of the window to the top of the screen, stretching if the
    /// opposite is also attached.
    #[serde(default)]
    attach_top: bool,
    /// Attach the right of the window to the right of the screen, stretching if the
    /// opposite is also attached.
    #[serde(default)]
    attach_right: bool,
    /// Attach the bottom of the window to the bottom of the screen, stretching if the
    /// opposite is also attached.
    #[serde(default)]
    attach_bottom: bool,
    /// Attach the left of the window to the left of the screen, stretching if the
    /// opposite is also attached.
    #[serde(default)]
    attach_left: bool,
    /// If left or right aren't attached, specify the window width.
    #[serde(default)]
    width: Option<P2>,
    /// If top or bottom aren't attached, specify the window height.
    #[serde(default)]
    height: Option<P2>,
    /// Enable keyboard interaction (enables keyboard focus, required for keyboard
    /// interaction).
    #[serde(default)]
    enable_keyboard: bool,
    /// Window title.
    #[serde(default)]
    title: Option<String>,
    /// Http over unix domain socket for `curl`-based IPC.
    #[serde(default)]
    listen: Option<PathBuf>,
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
    /// URL of a server to serve content from instead of `content_root`. `content_root`
    /// will still be used for the config json, but the remaining files will be ignored.
    server: Option<String>,
    debug: Option<()>,
    /// Additional arguments to be passed to the script.
    args: Vec<ArgKv>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
struct IPCReqCommand {
    command: Vec<String>,
    /// By default uses the working directory of `wongus`.
    #[serde(default)]
    working_dir: Option<String>,
    /// Add to environment inherited from `wongus` process.
    #[serde(default)]
    environment: HashMap<String, String>,
    /// Timeout command if it takes too long; defaults to 10s.
    #[serde(default)]
    timeout_secs: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
struct IPCReqIndependentCommand {
    command: Vec<String>,
    /// By default uses the working directory of `wongus`.
    #[serde(default)]
    working_dir: Option<String>,
    /// Add to environment inherited from `wongus` process.
    #[serde(default)]
    environment: HashMap<String, String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
struct IPCReqStreamCommand {
    id: usize,
    command: Vec<String>,
    /// By default uses the working directory of `wongus`.
    #[serde(default)]
    working_dir: Option<String>,
    /// Add to environment inherited from `wongus` process.
    #[serde(default)]
    environment: HashMap<String, String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum IPCReqBody {
    Log(String),
    Read(PathBuf),
    RunCommand(IPCReqCommand),
    RunIndependent(IPCReqIndependentCommand),
    StreamCommand(IPCReqStreamCommand),
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
struct WindowIpcWindow {
    id: usize,
    body: IPCReqBody,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
enum WindowIpcExternalBody {
    Ok(serde_json::Value),
    Err(String),
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
struct WindowIpcExternal {
    id: usize,
    body: WindowIpcExternalBody,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum WindowIpc {
    Window(WindowIpcWindow),
    External(WindowIpcExternal),
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
        enum UserEvent {
            Script(String),
            ErrExit(loga::Error),
        }

        let mut event_loop = event_loop::EventLoopBuilder::<UserEvent>::with_user_event().build();

        // Window
        let display = gtk::gdk::Display::default().context("Couldn't open connection to display/windowing system")?;
        let monitor = 'found_monitor : loop {
            let mut monitors = vec![];
            for i in 0 .. display.n_monitors() {
                let m = display.monitor(i).and_downcast::<gtk::gdk::Monitor>().unwrap();
                monitors.push(m);
            }
            if let Some(want_i) = config.monitor_index {
                for (i, m) in monitors.iter().enumerate() {
                    if want_i == i {
                        break 'found_monitor m.clone();
                    }
                }
            }
            if let Some(text) = &config.monitor_model {
                for m in &monitors {
                    if m.model().unwrap_or_default().to_ascii_lowercase().contains(&text.to_ascii_lowercase()) {
                        break 'found_monitor m.clone();
                    }
                }
            }
            if let Some(m) = display.primary_monitor() {
                break 'found_monitor m;
            }
            if let Some(m) = monitors.into_iter().next() {
                break 'found_monitor m;
            }
            return Err(loga::err("No monitors found"));
        };
        let gtk_window = gtk::ApplicationWindow::new(event_loop.deref().gtk_app());
        gtk_window.init_layer_shell();
        gtk_window.set_monitor(&monitor);
        gtk_window.set_layer(gtk_layer_shell::Layer::Top);
        gtk_window.auto_exclusive_zone_enable();
        gtk_window.set_anchor(gtk_layer_shell::Edge::Top, config.attach_top);
        gtk_window.set_anchor(gtk_layer_shell::Edge::Right, config.attach_right);
        gtk_window.set_anchor(gtk_layer_shell::Edge::Bottom, config.attach_bottom);
        gtk_window.set_anchor(gtk_layer_shell::Edge::Left, config.attach_left);
        if config.attach_left && config.attach_right {
            if config.width.is_some() {
                return Err(
                    loga::err(
                        "Both left and right sides of the window are attached to edges, width cannot be used but it is set in the config (should be null)",
                    ),
                );
            }
        } else {
            let Some(width) = config.width else {
                return Err(
                    loga::err(
                        "Left or right window edge attachments aren't set so the width is not decided but width is missing from the config",
                    ),
                );
            };
            gtk_window.set_width_request(match width {
                P2::Logical(x) => x,
                P2::Percent(p) => (monitor.geometry().width() as f64 * p / 100.).ceil() as i32,
            });
        }
        if config.attach_top && config.attach_bottom {
            if config.height.is_some() {
                return Err(
                    loga::err(
                        "Both left and right sides of the window are attached to edges, height cannot be used but it is set in the config (should be null)",
                    ),
                );
            }
        } else {
            let Some(height) = config.height else {
                return Err(
                    loga::err(
                        "Left or right window edge attachments aren't set so the height is not decided but height is missing from the config",
                    ),
                );
            };
            gtk_window.set_height_request(match height {
                P2::Logical(x) => x,
                P2::Percent(p) => (monitor.geometry().height() as f64 * p / 100.).ceil() as i32,
            });
        }
        gtk_window.set_skip_pager_hint(true);
        gtk_window.set_deletable(false);
        gtk_window.set_keyboard_interactivity(config.enable_keyboard);
        gtk_window.set_resizable(false);
        if let Some(screen) = GtkWindowExt::screen(&gtk_window) {
            if let Some(visual) = screen.rgba_visual() {
                gtk_window.set_visual(Some(&visual));
            }
        }
        gtk_window.set_app_paintable(true);
        gtk_window.set_decorated(false);
        gtk_window.stick();
        gtk_window.set_title(config.title.as_ref().map(|x| x.as_str()).unwrap_or("This is a wongus"));
        let default_vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
        gtk_window.add(&default_vbox);
        gtk_window.show_all();
        let window = tao::window::Window::new_from_gtk_window(event_loop.deref(), gtk_window).unwrap();
        window.set_skip_taskbar(true);

        // Webview
        let (ipc_req_tx, mut ipc_req_rx) = unbounded_channel::<Vec<u8>>();
        let webview = {
            let mut webview = wry::WebViewBuilder::new_gtk(&default_vbox);
            webview = webview.with_transparent(true);
            webview = webview.with_ipc_handler({
                move |req| {
                    _ = ipc_req_tx.send(req.into_body().into_bytes());
                }
            });
            webview = webview.with_initialization_script(include_str!("setup.js"));
            webview = webview.with_back_forward_navigation_gestures(false);
            webview = webview.with_devtools(true);

            // Custom proto:
            //
            // 1. to avoid panic due to triple-slash in `file:///`:
            //    https://github.com/tauri-apps/wry/issues/1255
            //
            // 2. to intercept and log errors
            webview = webview.with_asynchronous_custom_protocol("filex".into(), {
                let log = log.clone();
                move |request, responder| {
                    match (|| -> Result<http::Response<Cow<[u8]>>, loga::Error> {
                        let path = request.uri().path();
                        return Ok(
                            Response::builder()
                                .header(
                                    CONTENT_TYPE,
                                    mime_guess::from_path(&path).first_or_text_plain().essence_str(),
                                )
                                .body(
                                    Cow::Owned(
                                        std::fs::read(
                                            path,
                                        ).context_with("Error reading requested file", ea!(path = path))?,
                                    ),
                                )
                                .unwrap(),
                        );
                    })() {
                        Ok(r) => responder.respond(r),
                        Err(e) => {
                            let e = e.context("Error making request");
                            log.log_err(StandardFlag::Warning, e.clone());
                            responder.respond(
                                http::Response::builder()
                                    .header(CONTENT_TYPE, "text/plain")
                                    .status(500)
                                    .body(e.to_string().as_bytes().to_vec())
                                    .unwrap(),
                            );
                        },
                    }
                }
            });
            if let Some(url) = args.server {
                webview = webview.with_url(url);
            } else {
                webview = webview.with_url(format!(
                    "filex://x{}",
                    //. PROTO,
                    content_root.join("index.html").to_str().context("Content root path must be utf-8")?
                ));
            }
            webview.build().context("Error initializing webview")?
        };

        // More js initialization (dynamic)
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

        // Start thread for async/background processing (ipc, subcommands)
        spawn({
            let ipc_resp = event_loop.create_proxy();
            let rt =
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .context("Error starting ipc processor")?;
            let log = log.clone();
            let external_ipc_futures =
                Arc::new(Mutex::new(HashMap::<usize, oneshot::Sender<WindowIpcExternalBody>>::new()));

            // Handle ipc requests via js
            let window_ipc = {
                let log = log.fork(ea!(ipc = "window"));
                let external_ipc_futures = external_ipc_futures.clone();
                let ipc_resp = ipc_resp.clone();
                async move {
                    while let Some(req) = ipc_req_rx.recv().await {
                        let req = match serde_json::from_slice::<WindowIpc>(&req) {
                            Ok(r) => r,
                            Err(e) => {
                                log.log_err(
                                    StandardFlag::Warning,
                                    e.context_with(
                                        "Assertion! Error parsing IPC request",
                                        ea!(req = String::from_utf8_lossy(&req)),
                                    ),
                                );
                                return;
                            },
                        };
                        match req {
                            WindowIpc::Window(req) => {
                                tokio::spawn({
                                    let ipc_resp = ipc_resp.clone();
                                    let log = log.clone();
                                    async move {
                                        let resp = match async {
                                            match req.body {
                                                IPCReqBody::Log(message) => {
                                                    log.log(StandardFlag::Info, format!("wongus.log: {}", message));
                                                    return Ok(json!({ }));
                                                },
                                                IPCReqBody::Read(path) => {
                                                    return Ok(
                                                        json!(
                                                            &String::from_utf8(
                                                                tokio::fs::read(&path)
                                                                    .await
                                                                    .context("Error performing read command")?,
                                                            ).context("File isn't valid utf-8")?
                                                        ),
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
                                                    let log =
                                                        StandardLog::new().fork(ea!(command = command.dbg_str()));
                                                    let res = select!{
                                                        res = command.output() => res,
                                                        _ = sleep(
                                                            Duration::from_secs(req.timeout_secs.unwrap_or(10))
                                                        ) => {
                                                            return Err(
                                                                loga::err(
                                                                    "Command execution duration exceeded timeout",
                                                                ),
                                                            );
                                                        }
                                                    }.stack_context(&log, "Error starting command")?;
                                                    let log = log.fork(ea!(output = res.dbg_str()));
                                                    if !res.status.success() {
                                                        return Err(
                                                            log.err("Command exited with unsuccessful status"),
                                                        );
                                                    }
                                                    let stdout =
                                                        String::from_utf8(
                                                            res.stdout,
                                                        ).stack_context(&log, "stdout was not valid utf-8")?;
                                                    let stderr =
                                                        String::from_utf8(
                                                            res.stderr,
                                                        ).stack_context(&log, "stderr was not valid utf-8")?;
                                                    return Ok(json!({
                                                        "stdout": stdout,
                                                        "stderr": stderr
                                                    }));
                                                },
                                                IPCReqBody::RunIndependent(req) => {
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
                                                    let pid =
                                                        command
                                                            .spawn()
                                                            .context_with(
                                                                "Error starting command",
                                                                ea!(command = command.dbg_str()),
                                                            )?
                                                            .id();
                                                    return Ok(json!({
                                                        "pid": pid
                                                    }));
                                                },
                                                IPCReqBody::StreamCommand(req) => {
                                                    tokio::spawn({
                                                        let ipc_resp = ipc_resp.clone();
                                                        if req.command.is_empty() {
                                                            return Err(loga::err("Commandline is empty"));
                                                        }
                                                        let mut command = Command::new(&req.command[0]);
                                                        command.stdout(std::process::Stdio::piped());
                                                        command.args(&req.command[1..]);
                                                        if let Some(cwd) = req.working_dir {
                                                            command.current_dir(&cwd);
                                                        }
                                                        for (k, v) in req.environment {
                                                            command.env(k, v);
                                                        }
                                                        let log =
                                                            StandardLog
                                                            ::new().fork(ea!(command = command.dbg_str()));
                                                        let mut proc =
                                                            command
                                                                .spawn()
                                                                .stack_context(&log, "Error starting command")?;
                                                        async move {
                                                            match async {
                                                                let reader =
                                                                    BufReader::new(proc.stdout.take().unwrap());
                                                                let mut lines = reader.lines();
                                                                loop {
                                                                    match lines.next_line().await {
                                                                        Ok(Some(line)) => {
                                                                            match ipc_resp.send_event(
                                                                                UserEvent::Script(
                                                                                    format!(
                                                                                        "(window._wongus.stream_cbs.get({}))({});",
                                                                                        req.id,
                                                                                        serde_json::to_string(
                                                                                            &line,
                                                                                        ).unwrap()
                                                                                    ),
                                                                                ),
                                                                            ) {
                                                                                Ok(_) => { },
                                                                                Err(_) => { },
                                                                            };
                                                                        },
                                                                        Ok(None) => {
                                                                            break;
                                                                        },
                                                                        Err(e) => {
                                                                            return Err(
                                                                                e.stack_context(
                                                                                    &log,
                                                                                    "Error reading lines",
                                                                                ),
                                                                            );
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
                                                                        e.context(
                                                                            "Streaming command failed with error",
                                                                        ),
                                                                    );
                                                                },
                                                            }
                                                        }
                                                    });
                                                    return Ok(json!({ }));
                                                },
                                            }
                                        }.await {
                                            Ok(r) => r,
                                            Err(e) => json!({
                                                "err": e.to_string()
                                            }),
                                        };
                                        match ipc_resp.send_event(
                                            UserEvent::Script(
                                                format!(
                                                    "(window._wongus.responses.get({}))({});",
                                                    req.id,
                                                    serde_json::to_string(&resp).unwrap()
                                                ),
                                            ),
                                        ) {
                                            Ok(_) => { },
                                            Err(_) => { },
                                        };
                                    }
                                });
                            },
                            WindowIpc::External(resp) => {
                                external_ipc_futures
                                    .lock()
                                    .unwrap()
                                    .remove(&resp.id)
                                    .unwrap()
                                    .send(resp.body)
                                    .unwrap();
                            },
                        }
                    }
                }
            };

            // Handle requests from curl via uds
            let external_ipc = {
                struct State {
                    log: loga::StandardLog,
                    ipc_resp: EventLoopProxy<UserEvent>,
                    external_ipc: Arc<Mutex<HashMap<usize, oneshot::Sender<WindowIpcExternalBody>>>>,
                    ids: AtomicUsize,
                }

                let state = Arc::new(State {
                    log: log.fork(ea!(ipc = "external")),
                    ipc_resp: ipc_resp.clone(),
                    external_ipc: external_ipc_futures.clone(),
                    ids: AtomicUsize::new(1),
                });
                let listen = config.listen;
                async move {
                    if let Some(listen) = listen {
                        async fn handle_req(
                            state: Arc<State>,
                            req: Request<Incoming>,
                        ) -> Result<Response<Body>, Infallible> {
                            let id = state.ids.fetch_add(1, Ordering::Relaxed);
                            let (res_tx, res_rx) = oneshot::channel();
                            let req =
                                match req
                                    .into_body()
                                    .collect()
                                    .await
                                    .context("Error reading request body")
                                    .and_then(
                                        |r| serde_json::from_slice::<serde_json::Value>(
                                            &r.to_bytes(),
                                        ).context("Error parsing request body as JSON"),
                                    ) {
                                    Ok(r) => r,
                                    Err(e) => {
                                        return Ok(response_400(e));
                                    },
                                };
                            state.external_ipc.lock().unwrap().insert(id, res_tx);
                            match state
                                .ipc_resp
                                .send_event(
                                    UserEvent::Script(
                                        format!(
                                            "window._wongus.external_ipc({}, {});",
                                            id,
                                            serde_json::to_string(&req).unwrap()
                                        ),
                                    ),
                                ) {
                                Ok(_) => { },
                                Err(_) => {
                                    state.external_ipc.lock().unwrap().remove(&id);
                                    return Ok(response_503());
                                },
                            };
                            match res_rx.await {
                                Ok(r) => {
                                    match r {
                                        WindowIpcExternalBody::Ok(v) => {
                                            return Ok(response_200_json(v));
                                        },
                                        WindowIpcExternalBody::Err(v) => {
                                            return Ok(response_503_text(v));
                                        },
                                    }
                                },
                                Err(e) => {
                                    state
                                        .log
                                        .log_err(
                                            StandardFlag::Error,
                                            e.context("External ipc request to window failed"),
                                        );
                                    return Ok(response_503());
                                },
                            }
                        }

                        remove_file(
                            &listen,
                        ).log_with(
                            &log,
                            StandardFlag::Warning,
                            "Failed to clean up old usd socket",
                            ea!(path = listen.to_string_lossy()),
                        );
                        let listener = UnixSocket::new_stream()?;
                        listener.bind(&listen).stack_context(&log, "Error binding to uds socket address")?;
                        let listener = listener.listen(10).context("Error starting to listen on uds socket")?;
                        while let Some((conn, _)) = listener.accept().await.ok() {
                            tokio::spawn({
                                let state = state.clone();
                                async move {
                                    let log = state.log.clone();
                                    let state = state.clone();
                                    match async move {
                                        hyper_util::server::conn::auto::Builder::new(
                                            hyper_util::rt::TokioExecutor::new(),
                                        )
                                            .serve_connection(
                                                hyper_util::rt::TokioIo::new(conn),
                                                hyper::service::service_fn(
                                                    move |req| handle_req(state.clone(), req),
                                                ),
                                            )
                                            .await
                                            .map_err(
                                                |e| loga::err_with(
                                                    "Error serving HTTP on connection",
                                                    ea!(err = e.to_string()),
                                                ),
                                            )?;
                                        return Ok(()) as Result<(), loga::Error>;
                                    }.await {
                                        Ok(_) => (),
                                        Err(e) => {
                                            log.log_err(StandardFlag::Debug, e.context("Error serving connection"));
                                        },
                                    }
                                }
                            });
                        }
                    }
                    return Ok(()) as Result<(), loga::Error>;
                }
            };

            // Keep thread alive while stuff's going on
            move || rt.block_on(async move {
                select!{
                    r = external_ipc => match r {
                        Ok(_) => {
                        },
                        Err(e) => {
                            match ipc_resp.send_event(UserEvent::ErrExit(e)) {
                                Ok(_) => { },
                                Err(_) => { },
                            };
                        }
                    },
                    _ = window_ipc => {
                    },
                }
            })
        });

        // Run event loop in main thread
        let err = Arc::new(Mutex::new(None));
        event_loop.run_return({
            let err = err.clone();
            move |event, _, control_flow| {
                *control_flow = event_loop::ControlFlow::Wait;
                match event {
                    Event::UserEvent(e) => {
                        match e {
                            UserEvent::Script(script) => {
                                match webview.evaluate_script(&script) {
                                    Ok(_) => { },
                                    Err(e) => {
                                        log.log_err(
                                            StandardFlag::Error,
                                            e.context("Error executing ipc response script"),
                                        );
                                    },
                                };
                            },
                            UserEvent::ErrExit(e) => {
                                *err.lock().unwrap() = Some(e);
                                *control_flow = ControlFlow::Exit;
                            },
                        }
                    },
                    Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => *control_flow = ControlFlow::Exit,
                    _ => (),
                }
            }
        });
        let err = err.lock().unwrap().take();
        if let Some(e) = err {
            return Err(e);
        } else {
            return Ok(());
        }
    }

    match inner() {
        Ok(_) => { },
        Err(e) => fatal(e),
    }
}
