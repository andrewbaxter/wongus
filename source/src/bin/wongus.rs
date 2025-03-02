use {
    aargvark::{
        help::{
            HelpPattern,
            HelpPatternElement,
            HelpState,
        },
        traits_impls::AargvarkFromStr,
        vark,
        Aargvark,
    },
    flowcontrol::superif,
    gtk::{
        gdk::Screen,
        glib::CastNone,
        prelude::{
            ContainerExt,
            GtkWindowExt,
            MonitorExt,
            WidgetExt,
        },
        ApplicationWindow,
    },
    gtk_layer_shell::LayerShell,
    http::{
        header::CONTENT_TYPE,
        Request,
        Response,
    },
    http_body_util::BodyExt,
    htwrap::htserve::responses::{
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
        Log,
        ResultContext,
    },
    serde::Deserialize,
    serde_json::json,
    std::{
        borrow::Cow,
        cell::RefCell,
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
        fs::read_dir,
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
    wongus::{
        Config,
        P2,
    },
    wry::{
        PageLoadEvent,
        WebViewBuilder,
        WebViewBuilderExtUnix,
    },
};

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

    fn build_help_pattern(_state: &mut HelpState) -> HelpPattern {
        return HelpPattern(vec![HelpPatternElement::Type("KEY=VALUE".to_string())]);
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
struct IPCReqDetachedCommand {
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
    ListDir(PathBuf),
    FileExists(PathBuf),
    Read(PathBuf),
    RunCommand(IPCReqCommand),
    RunDetachedCommand(IPCReqDetachedCommand),
    StreamCommand(IPCReqStreamCommand),
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
struct WindowIpc {
    id: usize,
    body: IPCReqBody,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
enum ExternalIpcResp {
    Ok(serde_json::Value),
    Err(String),
}

fn main() {
    fn inner() -> Result<(), loga::Error> {
        let args = vark::<Args>();
        let log = Log::new_root(if args.debug.is_some() {
            loga::DEBUG
        } else {
            loga::INFO
        });
        let content_root = args.content_root.canonicalize().context("Error making content path absolute")?;
        let config_path = content_root.join("config.json");
        let config =
            serde_json::from_slice::<Config>(
                &std::fs::read(
                    &config_path,
                ).context_with("Error reading config", ea!(path = config_path.to_string_lossy()))?,
            ).context_with("Error parsing config as json", ea!(path = config_path.to_string_lossy()))?;
        if config.attach_left && config.attach_right {
            if config.width.is_some() {
                return Err(
                    loga::err(
                        "Both left and right sides of the window are attached to edges, width cannot be used but it is set in the config (should be null)",
                    ),
                );
            }
        } else if config.width.is_none() {
            return Err(
                loga::err(
                    "Left or right window edge attachments aren't set so the width is not decided but width is missing from the config",
                ),
            );
        }
        if config.attach_top && config.attach_bottom {
            if config.height.is_some() {
                return Err(
                    loga::err(
                        "Both left and right sides of the window are attached to edges, height cannot be used but it is set in the config (should be null)",
                    ),
                );
            }
        } else if config.height.is_none() {
            return Err(
                loga::err(
                    "Left or right window edge attachments aren't set so the height is not decided but height is missing from the config",
                ),
            );
        }

        // Event loop
        enum UserEvent {
            Script(String),
            ExternalScript(String, oneshot::Sender<ExternalIpcResp>),
            ErrExit(loga::Error),
            Exit,
        }

        let mut event_loop = event_loop::EventLoopBuilder::<UserEvent>::with_user_event().build();

        // Window
        let gtk_window = gtk::ApplicationWindow::new(event_loop.deref().gtk_app());
        gtk_window.init_layer_shell();
        gtk_window.display().connect_monitor_removed({
            let log = log.clone();
            let event_loop = event_loop.create_proxy();
            move |_display, monitor| {
                log.log(
                    loga::DEBUG,
                    format!("Monitor detached: {:?} {:?}", monitor.manufacturer(), monitor.model()),
                );
                _ = event_loop.send_event(UserEvent::Exit);
            }
        });
        superif!({
            let display = gtk_window.display();
            let mut monitors = vec![];
            for i in 0 .. display.n_monitors() {
                monitors.push(display.monitor(i).and_downcast::<gtk::gdk::Monitor>().unwrap());
            }
            if let Some(want_i) = config.monitor_index {
                for (i, m) in monitors.iter().enumerate() {
                    if want_i == i {
                        break 'found m.clone();
                    }
                }
            }
            if let Some(text) = &config.monitor_model {
                for m in &monitors {
                    if m.model().unwrap_or_default().to_ascii_lowercase().contains(&text.to_ascii_lowercase()) {
                        break 'found m.clone();
                    }
                }
            }
            if let Some(m) = display.primary_monitor() {
                break 'found m;
            }
            if let Some(m) = monitors.into_iter().next() {
                break 'found m;
            }
            return Err(loga::err("No suitable monitor found"));
        } monitor ='found {
            gtk_window.set_monitor(&monitor);

            // Namespace - unknown, unused; workaround for gtklayershell (c) issue 135 and the
            // rust bindings issue 37 to force remap after monitor lost
            let have_geom = monitor.geometry();
            if let Some(width) = config.width {
                gtk_window.set_width_request(match width {
                    P2::Logical(p) => p,
                    P2::Percent(p) => (have_geom.width() as f64 * p / 100.).ceil() as i32,
                    P2::Cm(p) => (have_geom.height() as f64 / monitor.width_mm() as f64 / 10. * p) as i32,
                });
            }
            if let Some(height) = config.height {
                gtk_window.set_height_request(match height {
                    P2::Logical(p) => p,
                    P2::Percent(p) => (have_geom.height() as f64 * p / 100.).ceil() as i32,
                    P2::Cm(p) => (have_geom.height() as f64 / monitor.height_mm() as f64 / 10. * p) as i32,
                });
            }
        });
        {
            fn update_screen(window: &ApplicationWindow, screen: &Screen) {
                if let Some(visual) = screen.rgba_visual() {
                    window.set_visual(Some(&visual));
                }
            }

            // Cb
            gtk_window.connect_screen_changed({
                move |window, screen| {
                    eprintln!("ev screen changed");
                    if let Some(screen) = screen {
                        update_screen(&window, screen);
                    }
                }
            });

            // Immediate
            if let Some(screen) = GtkWindowExt::screen(&gtk_window) {
                update_screen(&gtk_window, &screen);
            }
        }
        gtk_window.set_layer(gtk_layer_shell::Layer::Top);
        gtk_window.auto_exclusive_zone_enable();
        gtk_window.set_anchor(gtk_layer_shell::Edge::Top, config.attach_top);
        gtk_window.set_anchor(gtk_layer_shell::Edge::Right, config.attach_right);
        gtk_window.set_anchor(gtk_layer_shell::Edge::Bottom, config.attach_bottom);
        gtk_window.set_anchor(gtk_layer_shell::Edge::Left, config.attach_left);
        gtk_window.set_skip_pager_hint(true);
        gtk_window.set_deletable(false);
        gtk_window.set_keyboard_interactivity(config.enable_keyboard);
        gtk_window.set_resizable(false);
        gtk_window.set_app_paintable(true);
        gtk_window.set_decorated(false);
        gtk_window.stick();
        gtk_window.set_title(config.title.as_ref().map(|x| x.as_str()).unwrap_or("This is a wongus"));
        let default_vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
        gtk_window.add(&default_vbox);
        gtk_window.show_all();
        let window = tao::window::Window::new_from_gtk_window(event_loop.deref(), gtk_window).unwrap();
        window.set_skip_taskbar(true).unwrap();

        // For killing running subprocs
        let navigated = Arc::new(tokio::sync::Notify::new());

        // Webview
        let (ipc_req_tx, mut ipc_req_rx) = unbounded_channel::<Vec<u8>>();
        let webview = {
            let mut webview = WebViewBuilder::new();
            webview = webview.with_transparent(true);
            webview = webview.with_ipc_handler({
                move |req| {
                    let body = req.into_body().into_bytes();
                    ipc_req_tx.send(body).ignore();
                }
            });
            webview = webview.with_initialization_script(include_str!("../setup.js"));
            webview = webview.with_back_forward_navigation_gestures(false);
            webview = webview.with_devtools(true);

            // Custom proto: `filex://xPATH`
            //
            // 1. to avoid panic due to triple-slash in `file:///`:
            //    https://github.com/tauri-apps/wry/issues/1255
            //
            // 2. to intercept and log errors
            webview = webview.with_asynchronous_custom_protocol("filex".into(), {
                let log = log.clone();
                let content_root = content_root.clone();
                move |_, request, responder| {
                    match (|| -> Result<http::Response<Cow<[u8]>>, loga::Error> {
                        let path = content_root.join(request.uri().path());
                        return Ok(
                            Response::builder()
                                .header(
                                    CONTENT_TYPE,
                                    mime_guess::from_path(&path).first_or_text_plain().essence_str(),
                                )
                                .body(
                                    Cow::Owned(
                                        std::fs::read(
                                            &path,
                                        ).context_with("Error reading requested file", ea!(path = path.dbg_str()))?,
                                    ),
                                )
                                .unwrap(),
                        );
                    })() {
                        Ok(r) => responder.respond(r),
                        Err(e) => {
                            let e = e.context("Error making request");
                            log.log_err(loga::WARN, e.clone());
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
                webview = webview.with_initialization_script(&script.join(""));
            }
            webview = webview.with_on_page_load_handler({
                let navigated = navigated.clone();
                move |ev, _| {
                    let PageLoadEvent::Started = ev else {
                        return;
                    };
                    navigated.notify_waiters();
                }
            });
            webview.build_gtk(&default_vbox).context("Error initializing webview")?
        };

        // For killing thread when program exits
        let exited = Arc::new(tokio::sync::Notify::new());

        // Start thread for async/background processing (ipc, subcommands)
        spawn({
            let exited = exited.clone();
            let event_loop = event_loop.create_proxy();
            let rt =
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .context("Error starting ipc processor")?;
            let log = log.clone();

            // Handle ipc requests via js
            let window_ipc = {
                let log = log.fork(ea!(ipc = "window"));
                let event_loop = event_loop.clone();
                async move {
                    while let Some(req) = ipc_req_rx.recv().await {
                        let req = match serde_json::from_slice::<WindowIpc>(&req) {
                            Ok(r) => r,
                            Err(e) => {
                                log.log_err(
                                    loga::WARN,
                                    e.context_with(
                                        "Assertion! Error parsing IPC request",
                                        ea!(req = String::from_utf8_lossy(&req)),
                                    ),
                                );
                                return;
                            },
                        };
                        tokio::spawn({
                            let ipc_resp = event_loop.clone();
                            let log = log.clone();
                            let navigated = navigated.clone();
                            async move {
                                let resp = match async {
                                    match req.body {
                                        IPCReqBody::Log(message) => {
                                            log.log(loga::INFO, format!("wongus.log: {}", message));
                                            return Ok(json!({ }));
                                        },
                                        IPCReqBody::ListDir(path) => {
                                            let mut entries =
                                                read_dir(&path)
                                                    .await
                                                    .context_with(
                                                        "Error listing directory",
                                                        ea!(path = path.dbg_str()),
                                                    )?;
                                            let mut out = vec![];
                                            while let Some(entry) =
                                                entries
                                                    .next_entry()
                                                    .await
                                                    .context("Error reading directory entries")? {
                                                let entry_path = entry.path();
                                                let entry_path = match entry_path.to_str() {
                                                    Some(e) => e,
                                                    None => {
                                                        log.log_with(
                                                            loga::WARN,
                                                            "Directory entry not valid utf-8, skipping",
                                                            ea!(path = entry.path().to_string_lossy()),
                                                        );
                                                        continue;
                                                    },
                                                };
                                                out.push(entry_path.to_string());
                                            }
                                            return Ok(serde_json::to_value(&out).unwrap());
                                        },
                                        IPCReqBody::FileExists(path) => {
                                            return Ok(json!(path.exists()));
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
                                            let log = Log::new().fork(ea!(command = command.dbg_str()));
                                            let res = select!{
                                                res = command.output() => res,
                                                _ = navigated.notified() => {
                                                    return Err(loga::err("Navigation occurred"));
                                                },
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
                                            return Ok(json!({
                                                "stdout": stdout,
                                                "stderr": stderr
                                            }));
                                        },
                                        IPCReqBody::RunDetachedCommand(req) => {
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
                                                let log = Log::new().fork(ea!(command = command.dbg_str()));
                                                let mut proc =
                                                    command.spawn().stack_context(&log, "Error starting command")?;
                                                async move {
                                                    let work = async {
                                                        let reader = BufReader::new(proc.stdout.take().unwrap());
                                                        let mut lines = reader.lines();
                                                        loop {
                                                            match lines.next_line().await {
                                                                Ok(Some(line)) => {
                                                                    match ipc_resp.send_event(
                                                                        UserEvent::Script(
                                                                            format!(
                                                                                "(window._wongus.stream_cbs.get({}))({});",
                                                                                req.id,
                                                                                serde_json::to_string(&line).unwrap()
                                                                            ),
                                                                        ),
                                                                    ) {
                                                                        Ok(_) => (),
                                                                        Err(_) => (),
                                                                    };
                                                                },
                                                                Ok(None) => {
                                                                    break;
                                                                },
                                                                Err(e) => {
                                                                    return Err(
                                                                        e.stack_context(&log, "Error reading lines"),
                                                                    );
                                                                },
                                                            }
                                                        }
                                                        return Ok(());
                                                    };
                                                    let do_log = |level, m| {
                                                        log.log(level, &m);
                                                        match ipc_resp.send_event(
                                                            UserEvent::Script(
                                                                format!(
                                                                    "console.log({});",
                                                                    serde_json::to_string(&m).unwrap()
                                                                ),
                                                            ),
                                                        ) {
                                                            Ok(_) => (),
                                                            Err(_) => (),
                                                        }
                                                    };
                                                    match select!{
                                                        _ = navigated.notified() => {
                                                            Err(loga::err("Navigation occurred"))
                                                        },
                                                        w = work => {
                                                            w
                                                        }
                                                    } {
                                                        Ok(_) => {
                                                            do_log(
                                                                loga::INFO,
                                                                format!(
                                                                    "Streaming command [{:?}] exited normally",
                                                                    command
                                                                ),
                                                            );
                                                        },
                                                        Err(e) => {
                                                            do_log(
                                                                loga::WARN,
                                                                format!(
                                                                    "Streaming command [{:?}] failed with error: {}",
                                                                    command,
                                                                    e
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
                                    Err(e) => {
                                        let out = json!({
                                            "err": e.to_string()
                                        });
                                        log.log_err(loga::DEBUG, e.context("Error processing IPC message"));
                                        out
                                    },
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
                                    Err(e) => {
                                        log.log_err(
                                            loga::DEBUG,
                                            loga::err(e).context("Error sending IPC response"),
                                        );
                                    },
                                };
                            }
                        });
                    }
                }
            };

            // Handle requests from curl via uds
            let external_ipc = {
                struct State {
                    log: loga::Log,
                    ipc_resp: EventLoopProxy<UserEvent>,
                    ids: AtomicUsize,
                }

                let state = Arc::new(State {
                    log: log.fork(ea!(ipc = "external")),
                    ipc_resp: event_loop.clone(),
                    ids: AtomicUsize::new(1),
                });
                async move {
                    if let Some(listen) = config.listen {
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
                            match state
                                .ipc_resp
                                .send_event(
                                    UserEvent::ExternalScript(
                                        format!(
                                            "return window._wongus.external_ipc({}, {});",
                                            id,
                                            serde_json::to_string(&req).unwrap()
                                        ),
                                        res_tx,
                                    ),
                                ) {
                                Ok(_) => { },
                                Err(_) => {
                                    return Ok(response_503());
                                },
                            };
                            match res_rx.await {
                                Ok(r) => {
                                    match r {
                                        ExternalIpcResp::Ok(v) => {
                                            return Ok(response_200_json(v));
                                        },
                                        ExternalIpcResp::Err(v) => {
                                            return Ok(response_503_text(v));
                                        },
                                    }
                                },
                                Err(e) => {
                                    state
                                        .log
                                        .log_err(loga::WARN, e.context("External ipc request to window failed"));
                                    return Ok(response_503());
                                },
                            }
                        }

                        remove_file(
                            &listen,
                        ).log_with(
                            &state.log,
                            loga::WARN,
                            "Failed to clean up old usd socket",
                            ea!(path = listen.to_string_lossy()),
                        );
                        let listener = UnixSocket::new_stream()?;
                        listener.bind(&listen).stack_context(&state.log, "Error binding to uds socket address")?;
                        let listener = listener.listen(10).context("Error starting to listen on uds socket")?;
                        while let Some((conn, _)) = listener.accept().await.ok() {
                            tokio::spawn({
                                let state = state.clone();
                                async move {
                                    match async {
                                        hyper_util::server::conn::auto::Builder::new(
                                            hyper_util::rt::TokioExecutor::new(),
                                        )
                                            .serve_connection(
                                                hyper_util::rt::TokioIo::new(conn),
                                                hyper::service::service_fn({
                                                    let state = state.clone();
                                                    move |req| handle_req(state.clone(), req)
                                                }),
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
                                            state.log.log_err(loga::DEBUG, e.context("Error serving connection"));
                                        },
                                    }
                                }
                            });
                        }
                    } else {
                        std::future::pending::<()>().await;
                    }
                    return Ok(()) as Result<(), loga::Error>;
                }
            };

            // Keep thread alive while stuff's going on
            move || rt.block_on(async move {
                select!{
                    _ = exited.notified() => {
                    },
                    r = external_ipc => match r {
                        Ok(_) => {
                            log.log(loga::WARN, "External async IPC task exited!");
                        },
                        Err(e) => {
                            log.log(loga::DEBUG, "External async IPC task exited with error!");
                            match event_loop.send_event(UserEvent::ErrExit(e)) {
                                Ok(_) => { },
                                Err(_) => { },
                            };
                        }
                    },
                    _ = window_ipc => {
                        log.log(loga::WARN, "Window async IPC task exited!");
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
                                        log.log_err(loga::WARN, e.context("Error executing ipc response script"));
                                    },
                                };
                            },
                            UserEvent::ExternalScript(script, resp) => {
                                match webview.evaluate_script_with_callback(&script, {
                                    let resp = RefCell::new(Some(resp));
                                    move |json| {
                                        resp
                                            .borrow_mut()
                                            .take()
                                            .unwrap()
                                            .send(serde_json::from_str(&json).unwrap())
                                            .map_err(|_| loga::err(""))
                                            .ignore();
                                    }
                                }) {
                                    Ok(_) => { },
                                    Err(e) => {
                                        log.log_err(loga::WARN, e.context("Error executing external ipc cb"));
                                    },
                                };
                            },
                            UserEvent::ErrExit(e) => {
                                *err.lock().unwrap() = Some(e);
                                *control_flow = ControlFlow::Exit;
                            },
                            UserEvent::Exit => {
                                *control_flow = ControlFlow::Exit;
                            },
                        }
                    },
                    Event::WindowEvent { event, .. } => {
                        match event {
                            WindowEvent::CloseRequested | WindowEvent::Destroyed => {
                                *control_flow = ControlFlow::Exit;
                            },
                            _ => { },
                        }
                    },
                    _ => (),
                }
            }
        });
        exited.notify_waiters();
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
